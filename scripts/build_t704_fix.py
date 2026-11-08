#!/usr/bin/env python3
"""Build fix initramfs and align DTB for Track T704:
Embedded system.img patch (Option 2)

Applies:
- Injects patched Launcher3QuickStep.apk (SHA 8a28e1db, 25654202 B) into embedded ramdisk-images/system.img
- Removes pre-compiled AOT shadow (odex, vdex, oat/arm64, oat) from embedded ramdisk-images/system.img
- Preserves all T699 fixes in initramfs (Fix A: odrefresh log bypass + dalvik properties, Fix B: /dev/fb0 char node)
- Re-compresses with gzip compresslevel=9
- Re-aligns modified.dtb linux,initrd-end byte-exactly to 0x110000000 + new CPIO size
"""

from __future__ import annotations

import datetime
import gzip
import hashlib
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
BASE_CPIO = WORKTREE / "guest-inputs/t699-systemserver-fb0-fix.cpio.gz"
DIFF_FILE = WORKTREE / "system_img_diff.bin"
OUT_CPIO = WORKTREE / "guest-inputs/t704-systemserver-fb0-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t704-modified.dtb"

DTC_BIN = "/opt/homebrew/bin/dtc"
BLOCK_SIZE = 4096
CHUNK = 8 * 1024 * 1024


def pad4(n: int) -> int:
    return (4 - (n % 4)) % 4


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(CHUNK):
            h.update(chunk)
    return h.hexdigest()


def main():
    t0 = time.time()
    print("[T704 BUILDER] Starting initramfs rebuild (Option 2)...")

    # 1. Load diff blocks
    diffs = {}
    with open(DIFF_FILE, "rb") as fdiff:
        while True:
            hdr = fdiff.read(4)
            if not hdr:
                break
            block_idx = struct.unpack("<I", hdr)[0]
            data = fdiff.read(BLOCK_SIZE)
            diffs[block_idx] = data
    print(f"[T704 BUILDER] Loaded {len(diffs)} diff blocks ({len(diffs) * BLOCK_SIZE / (1024*1024):.2f} MB)")

    # 2. Rebuild CPIO stream
    partial_cpio = OUT_CPIO.with_suffix(".partial")
    replaced_system = False
    system_hasher = hashlib.sha256()

    with gzip.open(BASE_CPIO, "rb") as fin, gzip.GzipFile(partial_cpio, "wb", compresslevel=9, mtime=0.0) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if not hdr.startswith(b"070701"):
                break

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes.rstrip(b"\0").decode("utf-8", "replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "ramdisk-images/system.img":
                print(f"[T704 BUILDER] Patching ramdisk-images/system.img on the fly ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                block_idx = 0
                while block_idx * BLOCK_SIZE < filesize:
                    chunk = fin.read(BLOCK_SIZE)
                    if block_idx in diffs:
                        chunk = diffs[block_idx]
                    system_hasher.update(chunk)
                    fout.write(chunk)
                    block_idx += 1

                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

                replaced_system = True
                print(f"[T704 BUILDER] ramdisk-images/system.img patched and written cleanly")
                continue

            # Stream through other files unchanged
            fout.write(hdr)
            fout.write(name_bytes)
            if pad_name:
                fout.write(b"\0" * pad_name)

            remaining = filesize
            while remaining > 0:
                chunk_sz = min(remaining, CHUNK)
                buf = fin.read(chunk_sz)
                if not buf:
                    raise EOFError(f"Unexpected EOF while reading {name_str}")
                fout.write(buf)
                remaining -= len(buf)

            pad_data = pad4(filesize)
            if pad_data:
                pad_buf = fin.read(pad_data)
                fout.write(pad_buf)

    assert replaced_system, "ramdisk-images/system.img was not replaced"
    patched_system_sha = system_hasher.hexdigest()
    print(f"[T704 BUILDER] Patched embedded system.img SHA-256: {patched_system_sha}")
    assert patched_system_sha == "85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc"

    # Replace partial with final
    os.replace(partial_cpio, OUT_CPIO)
    new_cpio_size = OUT_CPIO.stat().st_size
    new_cpio_sha = sha256_file(OUT_CPIO)
    t1 = time.time()
    print(f"[T704 BUILDER] Built {OUT_CPIO} in {t1 - t0:.1f}s")
    print(f"[T704 BUILDER] Size: {new_cpio_size} bytes, SHA-256: {new_cpio_sha}")

    # 3. Re-align DTB
    print("[T704 BUILDER] Re-aligning DTB linux,initrd-end...")
    # Decompile base DTB to DTS
    dts_proc = subprocess.run([DTC_BIN, "-I", "dtb", "-O", "dts", str(BASE_DTB)], capture_output=True, text=True, check=True)
    dts_content = dts_proc.stdout

    # Calculate new initrd-end
    initrd_start = 0x110000000
    initrd_end = initrd_start + new_cpio_size
    end_high = (initrd_end >> 32) & 0xffffffff
    end_low = initrd_end & 0xffffffff

    new_end_line = f"linux,initrd-end = <0x{end_high:02x} 0x{end_low:08x}>;"
    print(f"[T704 BUILDER] initrd_start: {hex(initrd_start)}, initrd_end: {hex(initrd_end)} ({new_end_line})")

    # Replace in DTS
    lines = []
    found_end = False
    for line in dts_content.splitlines():
        if "linux,initrd-end" in line:
            lines.append(f"\t\t{new_end_line}")
            found_end = True
        else:
            lines.append(line)
    assert found_end, "linux,initrd-end not found in DTS"

    modified_dts = "\n".join(lines) + "\n"
    tmp_dts = WORKTREE / "t704-modified.dts"
    tmp_dts.write_text(modified_dts)

    # Compile DTS to DTB
    subprocess.run([DTC_BIN, "-I", "dts", "-O", "dtb", "-o", str(OUT_DTB), str(tmp_dts)], check=True)
    tmp_dts.unlink()

    new_dtb_size = OUT_DTB.stat().st_size
    new_dtb_sha = sha256_file(OUT_DTB)
    print(f"[T704 BUILDER] Built {OUT_DTB}: {new_dtb_size} bytes, SHA-256: {new_dtb_sha}")

    # 4. Verify DTB round-trip
    verify_proc = subprocess.run([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)], capture_output=True, text=True, check=True)
    assert new_end_line in verify_proc.stdout, f"Verification failed: {new_end_line} not in decompiled DTB"
    print("[T704 BUILDER] DTB verification PASSED!")


if __name__ == "__main__":
    main()
