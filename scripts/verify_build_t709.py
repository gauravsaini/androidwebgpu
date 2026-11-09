#!/usr/bin/env python3
import gzip
import hashlib
import os
import subprocess
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track709-logcat-systemserver-cure")
OUT_CPIO = WORKTREE / "guest-inputs/t709-systemserver-fb0-fix.cpio.gz"
OUT_DTB = WORKTREE / "guest-inputs/t709-modified.dtb"
DTC_BIN = "/opt/homebrew/bin/dtc"

def pad4(n: int) -> int:
    return (4 - (n % 4)) % 4

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    if not OUT_CPIO.exists():
        print(f"MISSING: {OUT_CPIO}")
        sys.exit(1)
    if not OUT_DTB.exists():
        print(f"MISSING: {OUT_DTB}")
        sys.exit(1)

    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"VERIFYING CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")

    found_early_ramdisk = False
    found_streamer = False

    with gzip.open(OUT_CPIO, "rb") as fin:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110 or not hdr.startswith(b"070701"):
                break
            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes.rstrip(b"\0").decode("utf-8", "replace")
            p_name = pad4(110 + namesize)
            if p_name: fin.read(p_name)
            p_file = pad4(filesize)

            if name_str == "early-ramdisk":
                data = fin.read(filesize)
                if p_file: fin.read(p_file)
                assert b"/logcat_streamer >/dev/console 2>&1 &" in data, "logcat_streamer launch missing in early-ramdisk!"
                found_early_ramdisk = True
                print("VERIFIED: early-ramdisk contains /logcat_streamer launch")
            elif name_str == "logcat_streamer":
                assert filesize == 663704, f"logcat_streamer size mismatch: {filesize}"
                fin.seek(filesize + p_file, 1)
                found_streamer = True
                print("VERIFIED: logcat_streamer member present in CPIO (663656 bytes)")
            else:
                fin.seek(filesize + p_file, 1)

    assert found_early_ramdisk, "early-ramdisk not found in CPIO!"
    assert found_streamer, "logcat_streamer not found in CPIO!"

    dtb_size = OUT_DTB.stat().st_size
    dtb_sha = sha256_file(OUT_DTB)
    print(f"VERIFYING DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    expected_end_prop = f"<0x{high_end:02x} 0x{low_end:08x}>"

    dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {expected_end_prop};" in dts, f"DTB initrd-end mismatch! Expected: {expected_end_prop}"
    print(f"VERIFIED: DTB linux,initrd-end matches byte-exactly: {expected_end_prop}")

    print("BUILD_T709_VERIFIED_OK")

if __name__ == "__main__":
    main()
