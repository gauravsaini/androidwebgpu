#!/usr/bin/env python3
"""Build deterministic ARM64 static initramfs.cpio with RAM+UART markers.

Produces an exact newc CPIO archive containing:
  - /dev/ (directory, 0755)
  - /dev/console (character device 5, 1, 0600)
  - /init (statically linked ARM64 ELF executable, 0755)
  - TRAILER!!! record
Padded to exact target size (default 6656 bytes / 13 blocks).

Usage: python3 guest-image/scripts/build_initramfs.py [-o /Users/Shared/initramfs.cpio]
                                                      [--size 6656]
"""
import argparse
import os
import subprocess
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ASM_SOURCE = os.path.join(SCRIPT_DIR, "static_init.s")


def compile_init() -> bytes:
    """Compile static_init.s using clang + rust-lld."""
    tmp_o = "/tmp/static_init.o"
    tmp_elf = "/tmp/static_init.elf"

    # Find rust-lld or lld
    lld_bin = None
    try:
        sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
        candidate = os.path.join(sysroot, "lib/rustlib/aarch64-apple-darwin/bin/rust-lld")
        if os.path.exists(candidate):
            lld_bin = candidate
    except Exception:
        pass

    if not lld_bin:
        for p in ["/opt/homebrew/opt/llvm/bin/ld.lld", "/usr/local/bin/ld.lld", "ld.lld"]:
            if os.path.exists(p) or subprocess.run(["which", p], stdout=subprocess.DEVNULL).returncode == 0:
                lld_bin = p
                break

    if not lld_bin:
        raise RuntimeError("Could not find an ELF linker (rust-lld or ld.lld)")

    # 1. Assemble
    cmd_as = ["clang", "-target", "aarch64-linux-gnu", "-c", ASM_SOURCE, "-o", tmp_o]
    res = subprocess.run(cmd_as, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"Assembler failed: {res.stderr}")

    # 2. Link
    if "rust-lld" in lld_bin:
        cmd_ld = [lld_bin, "-flavor", "gnu", tmp_o, "-o", tmp_elf]
    else:
        cmd_ld = [lld_bin, tmp_o, "-o", tmp_elf]
    res = subprocess.run(cmd_ld, capture_output=True, text=True)
    if res.returncode != 0:
        raise RuntimeError(f"Linker failed: {res.stderr}")

    with open(tmp_elf, "rb") as f:
        return f.read()


def make_cpio_entry(ino: int, mode: int, uid: int, gid: int, nlink: int, mtime: int,
                    filesize: int, maj: int, min_: int, rmaj: int, rmin: int,
                    name: str, body: bytes) -> bytes:
    """Construct a CPIO newc (070701) format entry."""
    namesize = len(name) + 1
    hdr = f"070701{ino:08x}{mode:08x}{uid:08x}{gid:08x}{nlink:08x}{mtime:08x}{filesize:08x}{maj:08x}{min_:08x}{rmaj:08x}{rmin:08x}{namesize:08x}00000000"
    hdr_b = hdr.encode("ascii")
    name_b = name.encode("ascii") + b"\x00"
    pad1 = b"\x00" * ((4 - (len(hdr_b) + len(name_b)) % 4) % 4)
    pad2 = b"\x00" * ((4 - len(body) % 4) % 4)
    return hdr_b + name_b + pad1 + body + pad2


def build_initramfs(init_elf_bytes: bytes, target_size: int = 6656) -> bytes:
    entries = []
    # 1. /dev directory (040755)
    entries.append(make_cpio_entry(1, 0o040755, 0, 0, 2, 0, 0, 0, 0, 0, 0, "dev", b""))
    # 2. /dev/console character device (020600, major 5, minor 1)
    entries.append(make_cpio_entry(2, 0o020600, 0, 0, 1, 0, 0, 0, 0, 5, 1, "dev/console", b""))
    # 3. /init executable (100755)
    entries.append(make_cpio_entry(3, 0o100755, 0, 0, 1, 0, len(init_elf_bytes), 0, 0, 0, 0, "init", init_elf_bytes))
    # 4. Trailer record
    entries.append(make_cpio_entry(0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, "TRAILER!!!", b""))

    blob = b"".join(entries)
    if len(blob) < target_size:
        blob = blob + b"\x00" * (target_size - len(blob))
    elif len(blob) > target_size:
        # Pad to next 512-byte block
        rem = len(blob) % 512
        if rem != 0:
            blob = blob + b"\x00" * (512 - rem)
    return blob


def main():
    ap = argparse.ArgumentParser(description="Build ARM64 static initramfs.cpio")
    ap.add_argument("-o", "--output", default="/Users/Shared/initramfs.cpio",
                    help="Output path (default: /Users/Shared/initramfs.cpio)")
    ap.add_argument("--size", type=int, default=6656,
                    help="Target size in bytes (default: 6656)")
    args = ap.parse_args()

    init_bytes = compile_init()
    cpio_blob = build_initramfs(init_bytes, target_size=args.size)
    with open(args.output, "wb") as f:
        f.write(cpio_blob)
    print(f"wrote {args.output} ({len(cpio_blob)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
