#!/usr/bin/env python3
"""
Track T707: Rebuild fix initramfs with updated vendor.img and realign DTB byte-exactly.
- Replaces ramdisk-images/vendor.img with guest-inputs/t707-vendor.img in CPIO.
- Re-compresses with gzip compresslevel=9, mtime=0.0.
- Re-aligns DTB linux,initrd-end byte-exactly per T699/T705 procedure.
"""

import gzip
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track707-composer-hotplug-dispatch")
BASE_CPIO = WORKTREE / "guest-inputs/t705-systemserver-fb0-fix.cpio.gz"
NEW_VENDOR = WORKTREE / "guest-inputs/t707-vendor.img"
OUT_CPIO = WORKTREE / "guest-inputs/t707-systemserver-fb0-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/t705-modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t707-modified.dtb"

DTC_BIN = "/opt/homebrew/bin/dtc"

def pad4(n: int) -> int:
    return (4 - (n % 4)) % 4

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def make_header(hdr: bytes, filesize: int) -> bytes:
    # hdr is 110 bytes ASCII hex
    # filesize is at offset 54..62 (8 bytes)
    return hdr[:54] + f"{filesize:08x}".encode("ascii") + hdr[62:]

def main():
    t0 = time.time()
    print("[T707 BUILDER] Starting initramfs repack with new vendor.img...")
    assert BASE_CPIO.is_file(), f"Base CPIO not found: {BASE_CPIO}"
    assert NEW_VENDOR.is_file(), f"New vendor.img not found: {NEW_VENDOR}"
    assert BASE_DTB.is_file(), f"Base DTB not found: {BASE_DTB}"

    vendor_size = NEW_VENDOR.stat().st_size
    vendor_sha = sha256_file(NEW_VENDOR)
    print(f"[T707 BUILDER] New vendor.img: {vendor_size} bytes, sha256: {vendor_sha}")

    partial_cpio = OUT_CPIO.with_suffix(".partial")
    members_written = []
    replaced_vendor = False

    with gzip.open(BASE_CPIO, "rb") as fin, gzip.GzipFile(partial_cpio, "wb", compresslevel=9, mtime=0.0) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110 or not hdr.startswith(b"070701"):
                break
            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes.rstrip(b"\0").decode("utf-8", "replace")
            p_name = pad4(110 + namesize)
            if p_name:
                fin.read(p_name)
            p_file = pad4(filesize)

            if name_str == "ramdisk-images/vendor.img":
                print(f"[T707 BUILDER] Replacing {name_str} ({filesize} -> {vendor_size} bytes)...")
                new_hdr = make_header(hdr, vendor_size)
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                # Stream new vendor.img
                with open(NEW_VENDOR, "rb") as vf:
                    while chunk := vf.read(8 * 1024 * 1024):
                        fout.write(chunk)
                p_new = pad4(vendor_size)
                if p_new:
                    fout.write(b"\0" * p_new)
                # Skip old vendor.img in input stream
                fin.seek(filesize + p_file, 1)
                replaced_vendor = True
            else:
                fout.write(hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                # Copy existing member data
                left = filesize + p_file
                while left > 0:
                    chunk = fin.read(min(left, 8 * 1024 * 1024))
                    if not chunk:
                        break
                    fout.write(chunk)
                    left -= len(chunk)

            members_written.append(name_str)

    assert replaced_vendor, "CRITICAL: ramdisk-images/vendor.img was NOT found or replaced!"
    partial_cpio.replace(OUT_CPIO)

    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"[T707 BUILDER] Rebuilt CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")
    print(f"[T707 BUILDER] Total members in rebuilt CPIO: {len(members_written)}")

    # DTB alignment
    print("[T707 BUILDER] Aligning DTB initrd-end...")
    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    end_prop_str = f"<0x{high_end:02x} 0x{low_end:08x}>"
    print(f"[T707 BUILDER] linux,initrd-start = <0x01 0x10000000>")
    print(f"[T707 BUILDER] linux,initrd-end   = {end_prop_str} (decimal: {INITRD_END})")

    # Decompile base DTB to DTS
    dts_content = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(BASE_DTB)]).decode("utf-8")

    new_dts_lines = []
    replaced_dtb = False
    for line in dts_content.splitlines():
        if "linux,initrd-end" in line:
            new_dts_lines.append(f"\t\tlinux,initrd-end = {end_prop_str};")
            replaced_dtb = True
        else:
            new_dts_lines.append(line)
    assert replaced_dtb, "Failed to find linux,initrd-end in DTB"

    dts_file = OUT_DTB.with_suffix(".dts")
    dts_file.write_text("\n".join(new_dts_lines) + "\n")

    # Compile new DTB
    subprocess.check_call([DTC_BIN, "-I", "dts", "-O", "dtb", "-o", str(OUT_DTB), str(dts_file)])
    dtb_size = OUT_DTB.stat().st_size
    dtb_sha = sha256_file(OUT_DTB)
    print(f"[T707 BUILDER] Rebuilt DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    # Verify decompiled DTB
    verify_dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {end_prop_str};" in verify_dts, "Verification of DTB failed!"
    print(f"[T707 BUILDER] DTB round-trip verified successfully!")
    print(f"[T707 BUILDER] Done in {time.time() - t0:.2f}s")

if __name__ == "__main__":
    main()
