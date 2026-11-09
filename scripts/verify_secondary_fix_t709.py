#!/usr/bin/env python3
"""
Gate G709-5: Verify Secondary Fix application in initramfs & DTB.
Checks:
1. Rebuilt fix initramfs contains early-ramdisk with T709 SettingsProvider database pre-creation commands.
2. Rebuilt fix initramfs contains logcat_streamer binary.
3. Rebuilt DTB contains matching linux,initrd-end byte-aligned to the CPIO size.
4. Outputs SECONDARY_FIX_EVALUATED_OK.
"""

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
    print(f"VERIFYING SECONDARY FIX CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")

    found_early_ramdisk = False
    found_secondary_fix = False
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
            p_name = (4 - ((110 + namesize) % 4)) % 4
            if p_name: fin.read(p_name)
            p_file = (4 - (filesize % 4)) % 4

            if name_str == "early-ramdisk":
                data = fin.read(filesize)
                if p_file: fin.read(p_file)
                found_early_ramdisk = True
                if b"[T709] /data/user_de/0/com.android.providers.settings/databases pre-created" in data:
                    found_secondary_fix = True
                    print("VERIFIED: early-ramdisk contains T709 SettingsProvider directory pre-creation patch")
            elif name_str == "logcat_streamer":
                fin.seek(filesize + p_file, 1)
                found_streamer = True
                print("VERIFIED: logcat_streamer present in CPIO")
            else:
                fin.seek(filesize + p_file, 1)

    assert found_early_ramdisk, "early-ramdisk not found in CPIO!"
    assert found_secondary_fix, "T709 secondary fix not found in early-ramdisk!"
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

    print("SECONDARY_FIX_EVALUATED_OK")

if __name__ == "__main__":
    main()
