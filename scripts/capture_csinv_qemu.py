#!/usr/bin/env python3
"""Execute the pinned kernel CSINV word once in native QEMU with fixed inputs."""

import hashlib
import json
import os
import select
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = json.loads((ROOT / "MANIFEST.json").read_text())
RAM_BASE = 0x4000_0000
TEXT_OFFSET = 0x80000
INSN_FILE_OFFSET = 0x2D4018
INSTRUCTION_PA = RAM_BASE + TEXT_OFFSET + INSN_FILE_OFFSET
INSTRUCTION_WORD = 0xDA818058
REGION_BASE = 0x4000_0000
INITIAL_X24 = 0xFFFF_FF80_0951_2000
PSTATE = 0xA00001C5


def verify_inputs():
    for role in ("kernel", "dtb", "initramfs"):
        path = Path(MANIFEST[f"{role}_path"])
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        expected = MANIFEST[f"{role}_sha256"]
        if actual != expected:
            raise SystemExit(f"{role} SHA-256 mismatch: {actual} != {expected}")
    qemu = MANIFEST["qemu_binary"]
    version = subprocess.check_output([qemu, "--version"], text=True).splitlines()[0]
    if version != MANIFEST["qemu_version"]:
        raise SystemExit(f"QEMU version mismatch: {version!r}")


def packet_checksum(payload):
    return sum(payload) & 0xFF


def read_byte(proc, deadline):
    while time.monotonic() < deadline:
        ready, _, _ = select.select(
            [proc.stdout], [], [], max(0, deadline - time.monotonic())
        )
        if ready:
            byte = os.read(proc.stdout.fileno(), 1)
            if not byte:
                raise RuntimeError("QEMU closed the GDB stdio stream")
            return byte
    raise TimeoutError("timed out waiting for QEMU GDB response")


def read_packet(proc, timeout=10):
    deadline = time.monotonic() + timeout
    while read_byte(proc, deadline) != b"$":
        pass
    payload = bytearray()
    while True:
        byte = read_byte(proc, deadline)
        if byte == b"#":
            break
        payload.extend(byte)
    read_byte(proc, deadline)
    read_byte(proc, deadline)
    proc.stdin.write(b"+")
    proc.stdin.flush()
    return bytes(payload).decode(errors="replace")


def send(proc, payload, timeout=10):
    raw = payload.encode()
    proc.stdin.write(b"$" + raw + f"#{packet_checksum(raw):02x}".encode())
    proc.stdin.flush()
    return read_packet(proc, timeout)


def read_state(proc, label):
    raw = bytes.fromhex(send(proc, "g"))
    if len(raw) < 268:
        raise RuntimeError(f"short QEMU register packet ({len(raw)} bytes)")

    def xreg(index):
        start = index * 8
        return int.from_bytes(raw[start : start + 8], "little")

    pc = xreg(32)
    pstate = int.from_bytes(raw[264:268], "little")
    state = {
        "pc": pc,
        "pstate": pstate,
        "x1": xreg(1),
        "x2": xreg(2),
        "x8": xreg(8),
        "x24": xreg(24),
    }
    print(
        f"{label}: pc={pc:#018x} pstate={pstate:#010x} "
        f"x1={state['x1']:#018x} x2={state['x2']:#018x} "
        f"x8={state['x8']:#018x} x24={state['x24']:#018x}"
    )
    return state


def write_register(proc, index, value, bits=64):
    value_hex = value.to_bytes(bits // 8, "little").hex()
    response = send(proc, f"P{index:x}={value_hex}")
    if response != "OK":
        raise RuntimeError(f"QEMU rejected register write P{index:x}: {response}")


def qemu_command(uart_log, gdb_stdio=False):
    qemu = MANIFEST["qemu_binary"]
    args = [
        qemu,
        "-machine",
        MANIFEST["qemu_machine"],
        "-cpu",
        MANIFEST["cpu"],
        "-m",
        f"{MANIFEST['ram_mb']}M",
        "-accel",
        MANIFEST["qemu_accel"],
        "-kernel",
        MANIFEST["kernel_path"],
        "-dtb",
        MANIFEST["dtb_path"],
        "-initrd",
        MANIFEST["initramfs_path"],
        "-append",
        MANIFEST["bootargs"],
        "-device",
        MANIFEST["qemu_gpu_device"],
        "-display",
        "none",
        "-serial",
        f"file:{uart_log}",
        "-monitor",
        "none",
        "-no-reboot",
    ]
    if gdb_stdio:
        args.extend(["-S", "-gdb", "stdio"])
    return args


def capture_live(proc):
    target_va = 0xFFFF_FF80_0835_4018
    print("GDB features:", send(proc, "qSupported:multiprocess+;xmlRegisters=aarch64"))
    print("Initial stop:", send(proc, "?"))
    bp = send(proc, f"Z1,{target_va:x},4")
    print(f"Hardware breakpoint at {target_va:#018x}: {bp}")
    if bp != "OK":
        raise RuntimeError("QEMU rejected the CSINV breakpoint")

    for hit in (1, 2):
        stop = send(proc, "c", timeout=120)
        print(f"Live hit {hit} stop: {stop}")
        before = read_state(proc, f"QEMU LIVE HIT {hit} BEFORE")
        if before["pc"] != target_va:
            raise RuntimeError(f"QEMU stopped at {before['pc']:#x}, expected target")
        if hit == 1:
            print("Guest bytes around CSINV:", send(proc, f"m{target_va - 16:x},24"))
        print("Single-step stop:", send(proc, "s"))
        after = read_state(proc, f"QEMU LIVE HIT {hit} AFTER")
        print(
            f"Live hit {hit} X24: {before['x24']:#018x} -> {after['x24']:#018x}; "
            f"X1={before['x1']:#018x} X2={before['x2']:#018x} "
            f"X8={before['x8']:#018x} PSTATE={before['pstate']:#010x}"
        )


def capture_matched_vector(proc):
    print("GDB features:", send(proc, "qSupported:multiprocess+;xmlRegisters=aarch64"))
    print("Initial stop:", send(proc, "?"))
    for index, value in (
        (1, REGION_BASE),
        (2, REGION_BASE),
        (8, (~REGION_BASE) & 0xFFFF_FFFF_FFFF_FFFF),
        (24, INITIAL_X24),
        (32, INSTRUCTION_PA),
        (33, PSTATE),
    ):
        write_register(proc, index, value, bits=32 if index == 33 else 64)

    encoded = send(proc, f"m{INSTRUCTION_PA:x},4")
    print(f"QEMU instruction bytes at {INSTRUCTION_PA:#x}: {encoded}")
    if encoded != INSTRUCTION_WORD.to_bytes(4, "little").hex():
        raise RuntimeError("QEMU did not load the expected guest instruction")
    before = read_state(proc, "QEMU BEFORE")
    print("Single-step stop:", send(proc, "s"))
    after = read_state(proc, "QEMU AFTER")
    if after["x24"] != REGION_BASE or after["pc"] != INSTRUCTION_PA + 4:
        raise RuntimeError("QEMU CSINV result or PC did not match the vector")
    print(
        f"CSINV vector: {INSTRUCTION_WORD:#010x}; "
        f"X24 {before['x24']:#018x} -> {after['x24']:#018x}"
    )


def main():
    verify_inputs()
    mode = sys.argv[1] if len(sys.argv) > 1 else "--matched-vector"
    if mode not in ("--matched-vector", "--live"):
        raise SystemExit("usage: capture_csinv_qemu.py [--matched-vector|--live]")
    uart_name = "qemu-csinv-live.uart.log" if mode == "--live" else "qemu-csinv-micro.uart.log"
    uart_log = ROOT / "evidence/t499" / uart_name
    uart_log.parent.mkdir(parents=True, exist_ok=True)
    uart_log.write_bytes(b"")
    args = qemu_command(uart_log, gdb_stdio=True)
    proc = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )
    try:
        if mode == "--live":
            capture_live(proc)
        else:
            capture_matched_vector(proc)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
        stderr = proc.stderr.read().decode(errors="replace")
        if stderr.strip():
            print("QEMU stderr:", stderr)


if __name__ == "__main__":
    main()
