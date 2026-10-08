#!/usr/bin/env python3
"""Run Track 99 AArch64 exception and initramfs checks against QEMU."""

from __future__ import annotations

import argparse
import os
import shutil
import struct
import subprocess
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ASM = ROOT / "tests/qemu/t99_userspace_oracle.S"
RAM_BASE = 0x40000000
INITRD_START = 0x48000000
INITRD_SIZE = 6656
INITRD_END = INITRD_START + INITRD_SIZE
INITRD_SHA256 = "49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38"
QEMU_PASS = "T99-ORACLE-PASS"
QEMU_FAIL = "T99-ORACLE-FAIL"
BOOT_MARKERS = (
    "Trying to unpack rootfs image as initramfs...",
    "Freeing initrd memory",
    "Run /init as init process",
    "STATUS: PASS - USERSPACE INITIALIZATION COMPLETE",
)


def elf_text(path: Path) -> bytes:
    data = path.read_bytes()
    if data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", data, 18)[0] != 183:
        raise RuntimeError(f"{path.name}: expected ELF64 little-endian AArch64 object")
    shoff = struct.unpack_from("<Q", data, 40)[0]
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", data, 58)
    if shentsize != 64 or shstrndx >= shnum:
        raise RuntimeError(f"{path.name}: invalid ELF section table")
    sections = []
    for index in range(shnum):
        header = struct.unpack_from("<IIQQQQIIQQ", data, shoff + index * shentsize)
        sections.append(header)
    names_header = sections[shstrndx]
    names = data[names_header[4] : names_header[4] + names_header[5]]

    def section_name(header: tuple[int, ...]) -> str:
        start = header[0]
        return names[start:].split(b"\0", 1)[0].decode("ascii")

    text_index = next(i for i, header in enumerate(sections) if section_name(header) == ".text")
    text_header = sections[text_index]
    relocations = [
        section_name(header)
        for header in sections
        if header[1] in (4, 9) and header[7] == text_index and header[5] != 0
    ]
    if relocations:
        raise RuntimeError(f"{path.name}: unresolved text relocations: {relocations}")
    start, size = text_header[4], text_header[5]
    if start + size > len(data):
        raise RuntimeError(f"{path.name}: .text extends past object size")
    return data[start : start + size]


def run_micro_oracles() -> None:
    clang = os.environ.get("CLANG") or shutil.which("clang")
    qemu = os.environ.get("QEMU") or shutil.which("qemu-system-aarch64")
    if not clang or not qemu:
        raise RuntimeError("micro-oracles require clang and qemu-system-aarch64 on PATH")
    for case in ("SVC", "ERET", "QPAIR", "CPACR"):
        with tempfile.TemporaryDirectory(prefix=f"t99-{case.lower()}-") as tmp:
            obj = Path(tmp) / f"{case}.o"
            binary = Path(tmp) / f"{case}.bin"
            subprocess.run(
                [clang, "--target=aarch64-none-elf", f"-DT99_ORACLE_{case}", "-c", str(ASM), "-o", str(obj)],
                check=True,
                capture_output=True,
                text=True,
            )
            binary.write_bytes(elf_text(obj))
            command = [
                qemu,
                "-M",
                "virt",
                "-cpu",
                "max",
                "-nographic",
                "-monitor",
                "none",
                "-serial",
                "none",
                "-semihosting-config",
                "enable=on,target=native",
                "-device",
                f"loader,file={binary},addr=0x40100000,cpu-num=0",
                "-no-reboot",
                "-no-shutdown",
            ]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            output = result.stdout + result.stderr
            if QEMU_PASS not in output or QEMU_FAIL in output:
                raise RuntimeError(f"QEMU {case} oracle failed (exit {result.returncode}):\n{output[-4000:]}")
            print(f"PASS QEMU {case}: {QEMU_PASS}")


def fdt_properties(blob: bytes) -> dict[str, bytes]:
    if len(blob) < 40 or struct.unpack_from(">I", blob, 0)[0] != 0xD00DFEED:
        raise RuntimeError("DTB is not a valid flattened device tree")
    total = struct.unpack_from(">I", blob, 4)[0]
    struct_offset, strings_offset = struct.unpack_from(">II", blob, 8)
    strings_size, struct_size = struct.unpack_from(">II", blob, 32)
    strings = blob[strings_offset : strings_offset + strings_size]
    end = min(struct_offset + struct_size, total)
    properties: dict[str, bytes] = {}
    cursor = struct_offset
    while cursor + 4 <= end:
        token = struct.unpack_from(">I", blob, cursor)[0]
        cursor += 4
        if token == 1:  # FDT_BEGIN_NODE
            node_end = blob.index(b"\0", cursor, end) + 1
            cursor = (node_end + 3) & ~3
        elif token == 2:  # FDT_END_NODE
            continue
        elif token == 3:  # FDT_PROP
            length, name_offset = struct.unpack_from(">II", blob, cursor)
            cursor += 8
            value = blob[cursor : cursor + length]
            cursor = (cursor + length + 3) & ~3
            name = strings[name_offset:].split(b"\0", 1)[0].decode("ascii")
            properties[name] = value
        elif token == 4:  # FDT_NOP
            continue
        elif token == 9:  # FDT_END
            break
        else:
            raise RuntimeError(f"invalid FDT structure token {token}")
    return properties


def parse_newc(data: bytes) -> set[str]:
    names: set[str] = set()
    cursor = 0
    trailer_seen = False
    while cursor + 110 <= len(data):
        header = data[cursor : cursor + 110]
        if header[:6] != b"070701":
            if trailer_seen and not any(data[cursor:]):
                break
            raise RuntimeError(f"invalid newc magic at archive offset 0x{cursor:x}")
        fields = [int(header[6 + i * 8 : 14 + i * 8], 16) for i in range(13)]
        file_size, name_size = fields[6], fields[11]
        name_start = cursor + 110
        name_end = name_start + name_size
        if name_size < 1 or name_end > len(data) or data[name_end - 1] != 0:
            raise RuntimeError("invalid newc filename length")
        name = data[name_start : name_end - 1].decode("utf-8")
        data_start = (name_end + 3) & ~3
        data_end = data_start + file_size
        if data_end > len(data):
            raise RuntimeError(f"newc entry {name!r} extends past the archive")
        names.add(name)
        trailer_seen = name == "TRAILER!!!"
        cursor = (data_end + 3) & ~3
        if trailer_seen:
            break
    if not trailer_seen:
        raise RuntimeError("newc archive has no TRAILER!!! entry")
    return names


def validate_initramfs_inputs(input_dir: Path) -> tuple[Path, Path, Path]:
    import hashlib

    image = input_dir / "Image"
    archive = input_dir / "initramfs.cpio"
    dtb = input_dir / "minimal-virt-fixed.dtb"
    for path in (image, archive, dtb):
        if not path.is_file():
            raise RuntimeError(f"missing QEMU input: {path}")
    archive_bytes = archive.read_bytes()
    digest = hashlib.sha256(archive_bytes).hexdigest()
    if len(archive_bytes) != INITRD_SIZE or digest != INITRD_SHA256:
        raise RuntimeError(f"initramfs identity mismatch: {len(archive_bytes)} bytes, SHA256 {digest}")
    if archive_bytes.startswith(b"\x1f\x8b"):
        raise RuntimeError("fixture is gzip-compressed; this oracle expects raw newc CPIO")
    names = parse_newc(archive_bytes)
    if "init" not in names and "./init" not in names:
        raise RuntimeError("initramfs has no init executable")
    props = fdt_properties(dtb.read_bytes())
    for name, expected in (("linux,initrd-start", INITRD_START), ("linux,initrd-end", INITRD_END)):
        raw = props.get(name)
        if raw is None or int.from_bytes(raw, "big") != expected:
            actual = None if raw is None else hex(int.from_bytes(raw, "big"))
            raise RuntimeError(f"DTB {name} mismatch: expected {expected:#x}, got {actual}")
    bootargs = props.get("bootargs", b"").rstrip(b"\0").decode("ascii", errors="replace")
    if "rdinit=/init" not in bootargs:
        raise RuntimeError(f"DTB bootargs do not select /init: {bootargs!r}")
    print(
        f"PASS raw newc CPIO: {len(archive_bytes)} bytes SHA256={digest}; "
        f"range=[{INITRD_START:#x},{INITRD_END:#x}); no gzip layer is present"
    )
    print(f"PASS DTB: linux,initrd-start/end match; bootargs={bootargs!r}")
    return image, archive, dtb


def run_kernel_unpack_oracle(input_dir: Path, timeout_seconds: int) -> None:
    qemu = os.environ.get("QEMU") or shutil.which("qemu-system-aarch64")
    if not qemu:
        raise RuntimeError("kernel unpack oracle requires qemu-system-aarch64 on PATH")
    image, archive, dtb = validate_initramfs_inputs(input_dir)
    with tempfile.TemporaryDirectory(prefix="t99-kernel-boot-") as tmp:
        uart = Path(tmp) / "uart.log"
        command = [
            qemu,
            "-M",
            "virt",
            "-cpu",
            "cortex-a53",
            "-smp",
            "1",
            "-m",
            "1024",
            "-kernel",
            str(image),
            "-dtb",
            str(dtb),
            "-initrd",
            str(archive),
            "-display",
            "none",
            "-monitor",
            "none",
            "-serial",
            f"file:{uart}",
            "-no-reboot",
            "-no-shutdown",
        ]
        proc = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.monotonic() + timeout_seconds
        try:
            while time.monotonic() < deadline:
                if uart.exists() and BOOT_MARKERS[-1] in uart.read_text(errors="replace"):
                    break
                if proc.poll() is not None:
                    break
                time.sleep(0.2)
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
        output = uart.read_text(errors="replace") if uart.exists() else ""
        cursor = -1
        for marker in BOOT_MARKERS:
            found = output.find(marker)
            if found <= cursor:
                tail = output[-5000:]
                raise RuntimeError(f"QEMU kernel unpack sequence missing/out of order at {marker!r}:\n{tail}")
            cursor = found
            print(f"PASS QEMU kernel sequence: {marker}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--micro-only", action="store_true", help="run the four bare-metal QEMU probes")
    group.add_argument("--boot-only", action="store_true", help="run the exact initramfs Linux boot oracle")
    parser.add_argument(
        "--input-dir",
        type=Path,
        default=Path(os.environ.get("T99_GUEST_INPUTS", "/Users/Shared/guest-inputs")),
        help="directory containing Image, initramfs.cpio, and minimal-virt-fixed.dtb",
    )
    parser.add_argument("--timeout", type=int, default=180, help="Linux boot oracle timeout in seconds")
    args = parser.parse_args()
    if not args.boot_only:
        run_micro_oracles()
    if not args.micro_only:
        run_kernel_unpack_oracle(args.input_dir, args.timeout)


if __name__ == "__main__":
    main()
