#!/usr/bin/env python3
"""Extract and verify patched SwiftShader binary from T638 fix image."""

from __future__ import annotations

import gzip
import hashlib
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
T638_INITRD = ROOT / "guest-inputs" / "t638-skia-texture-initramfs.cpio.gz"
OUTPUT_SO = ROOT / "toolchain" / "libGLES_swiftshader_patched.so"
EXPECTED_SO_SHA = "bd57955669865747618d4c532bf17e99e23b474dcb4a9239f9817cbefd63319a"
DEBUGFS = Path("/opt/homebrew/opt/e2fsprogs/sbin/debugfs")
CHUNK = 8 * 1024 * 1024


def pad4(size: int) -> int:
    return (-size) & 3


def read_entry(stream):
    header = stream.read(110)
    if len(header) != 110 or header[:6] != b"070701":
        raise ValueError("invalid or truncated newc CPIO header")
    namesize = int(header[94:102], 16)
    filesize = int(header[54:62], 16)
    name = stream.read(namesize)
    if len(name) != namesize or not name.endswith(b"\0"):
        raise EOFError("truncated or malformed CPIO name")
    padding = stream.read(pad4(110 + namesize))
    if len(padding) != pad4(110 + namesize):
        raise EOFError("truncated CPIO name padding")
    return header, name, name[:-1].decode("latin1"), padding, filesize


def copy_exact(source, target, size: int):
    remaining = size
    while remaining:
        block = source.read(min(CHUNK, remaining))
        if not block:
            raise EOFError(f"truncated CPIO data (wanted {size} bytes)")
        if target is not None:
            target.write(block)
        remaining -= len(block)


def extract_cpio_member(image: Path, wanted: str, out_path: Path):
    with gzip.open(image, "rb") as stream, out_path.open("wb") as target:
        while True:
            _, _, name, _, size = read_entry(stream)
            if name == wanted:
                copy_exact(stream, target, size)
                return
            else:
                copy_exact(stream, None, size)
            padding = stream.read(pad4(size))
            if len(padding) != pad4(size):
                raise EOFError(f"truncated CPIO data padding for {name}")
            if name == "TRAILER!!!":
                break
    raise RuntimeError(f"missing CPIO member: {wanted}")


def main():
    print(f"Extracting patched SwiftShader from {T638_INITRD}...")
    with tempfile.TemporaryDirectory(prefix="t640-extract-", dir="/private/tmp") as tmpdir:
        tmp_vendor = Path(tmpdir) / "vendor.img"
        extract_cpio_member(T638_INITRD, "ramdisk-images/vendor.img", tmp_vendor)
        print(f"Extracted vendor.img ({tmp_vendor.stat().st_size} bytes)")

        # Dump libGLES_swiftshader.so
        dump_cmd = [str(DEBUGFS), "-R", f"dump /lib64/egl/libGLES_swiftshader.so {OUTPUT_SO}", str(tmp_vendor)]
        res = subprocess.run(dump_cmd, capture_output=True, text=True, check=True)

        data = OUTPUT_SO.read_bytes()
        actual_sha = hashlib.sha256(data).hexdigest()
        print(f"Dumped libGLES_swiftshader.so SHA-256: {actual_sha}")
        assert actual_sha == EXPECTED_SO_SHA, f"SHA mismatch! Expected {EXPECTED_SO_SHA}, got {actual_sha}"

        # Verify patch bytes at 0x5afc
        print(f"Bytes at 0x5afc: {data[0x5afc:0x5b00].hex()}")
        assert data[0x5afc:0x5b00] == bytes.fromhex("81 05 00 b4"), "Patched instructions not found at 0x5afc"
        print("VERIFIED: SwiftShader binary extracted, SHA-256 matches, and patch instructions verified at 0x5afc.")


if __name__ == "__main__":
    main()
