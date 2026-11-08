#!/usr/bin/env python3
"""Build fix initramfs and align DTB for Track T705:
- Remove ramdisk-images/system.img from CPIO entirely
- Update early-ramdisk to mount /dev/vda directly as /system (loop0 path gone)
- Update fstab.virt and system/etc/fstab.virt: /dev/block/vda for /system
- Preserve all other files from t699 CPIO (vendor.img, userdata.img, Fix A/B, etc.)
- Re-compress with gzip compresslevel=9, mtime=0.0
- Re-align modified.dtb linux,initrd-end byte-exactly to 0x110000000 + new CPIO size
"""

from __future__ import annotations

import datetime
import gzip
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
BASE_CPIO = WORKTREE / "guest-inputs/t699-systemserver-fb0-fix.cpio.gz"
OUT_CPIO = WORKTREE / "guest-inputs/t705-systemserver-fb0-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t705-modified.dtb"

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
    print("[T705 BUILDER] Starting initramfs rebuild without embedded system.img...")

    # Load early-ramdisk content
    with gzip.open(BASE_CPIO, "rb") as fin:
        early_ramdisk_orig = None
        fstab_orig = None
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110 or not hdr.startswith(b"070701"):
                break
            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name = fin.read(namesize).rstrip(b"\0").decode("utf-8", "replace")
            p_name = pad4(110 + namesize)
            if p_name:
                fin.read(p_name)
            p_file = pad4(filesize)
            if name == "early-ramdisk":
                early_ramdisk_orig = fin.read(filesize)
                if p_file:
                    fin.read(p_file)
            elif name == "fstab.virt":
                fstab_orig = fin.read(filesize)
                if p_file:
                    fin.read(p_file)
            else:
                fin.seek(filesize + p_file, 1)

    assert early_ramdisk_orig is not None, "early-ramdisk not found in base CPIO"
    assert fstab_orig is not None, "fstab.virt not found in base CPIO"

    # Prepare modified early-ramdisk
    # Replace loop0 attachment with /dev/vda mount
    early_str = early_ramdisk_orig.decode("utf-8")
    
    # Target replacement block
    old_loop0_block = """"$TOYBOX" mknod -m 0600 /dev/loop0 b 7 0
"$TOYBOX" mknod -m 0600 /dev/loop1 b 7 1
"$TOYBOX" losetup -r /dev/loop0 /ramdisk-images/system.img
"$TOYBOX" losetup -r /dev/loop1 /ramdisk-images/vendor.img
"$TOYBOX" mknod -m 0600 /dev/block/loop0 b 7 0
"$TOYBOX" mknod -m 0600 /dev/block/loop1 b 7 1
echo "[T552] guest attached embedded images to loop0/loop1" """

    # Check if exact match or line by line
    if 'losetup -r /dev/loop0 /ramdisk-images/system.img' in early_str:
        print("[T705 BUILDER] Found loop0 system.img attachment in early-ramdisk. Replacing...")
        # Replace loop0 lines with /dev/vda configuration logic
        new_vda_block = """# T705: Configure external virtio-blk /dev/vda directly for /system (loop0 path removed)
"$TOYBOX" mknod -m 0600 /dev/loop1 b 7 1
"$TOYBOX" losetup -r /dev/loop1 /ramdisk-images/vendor.img
"$TOYBOX" mknod -m 0600 /dev/block/loop1 b 7 1
echo "[T705] guest attached embedded vendor.img to loop1"

if [ ! -b /dev/vda ]; then
    "$TOYBOX" mknod -m 0600 /dev/vda b 254 0 2>/dev/null || :
fi
"$TOYBOX" ln -sf /dev/vda /dev/block/vda 2>/dev/null || :
"$TOYBOX" mknod -m 0600 /dev/block/vda b 254 0 2>/dev/null || :
echo "[T705] /dev/vda configured for /system mount" """
        
        # Replace old lines
        lines = early_str.splitlines()
        new_lines = []
        skip = False
        replaced = False
        for line in lines:
            if 'mknod -m 0600 /dev/loop0 b 7 0' in line:
                skip = True
                continue
            if skip:
                if 'echo "[T552] guest attached embedded images to loop0/loop1"' in line:
                    skip = False
                    new_lines.append(new_vda_block)
                    replaced = True
                continue
            new_lines.append(line)
        
        assert replaced, "Failed to replace loop0 setup block"
        early_ramdisk_mod = "\n".join(new_lines).encode("utf-8")
    else:
        raise ValueError("Could not find loop0 system.img attachment in early-ramdisk")

    # Prepare modified fstab.virt
    fstab_str = fstab_orig.decode("utf-8")
    fstab_mod = fstab_str.replace("/dev/block/loop0 /system", "/dev/block/vda /system").encode("utf-8")

    # Stream CPIO and write new CPIO
    partial_cpio = OUT_CPIO.with_suffix(".partial")
    members_written = []
    dropped_system_img = False

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

            if name_str == "ramdisk-images/system.img":
                print(f"[T705 BUILDER] DROPPING {name_str} ({filesize} bytes)...")
                # Skip file data and padding
                fin.seek(filesize + p_file, 1)
                dropped_system_img = True
                continue

            content = None
            if name_str == "early-ramdisk":
                print(f"[T705 BUILDER] Updating early-ramdisk ({filesize} -> {len(early_ramdisk_mod)} bytes)...")
                content = early_ramdisk_mod
            elif name_str in ("fstab.virt", "system/etc/fstab.virt"):
                print(f"[T705 BUILDER] Updating {name_str} ({filesize} -> {len(fstab_mod)} bytes)...")
                content = fstab_mod

            if content is not None:
                new_hdr = make_header(hdr, len(content))
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                fout.write(content)
                p_new = pad4(len(content))
                if p_new:
                    fout.write(b"\0" * p_new)
                fin.seek(filesize + p_file, 1)
            else:
                fout.write(hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                # Copy file data
                bytes_left = filesize + p_file
                while bytes_left > 0:
                    chunk = fin.read(min(bytes_left, 8 * 1024 * 1024))
                    if not chunk:
                        break
                    fout.write(chunk)
                    bytes_left -= len(chunk)

            members_written.append(name_str)

    assert dropped_system_img, "CRITICAL: ramdisk-images/system.img was NOT found or dropped!"
    assert "ramdisk-images/system.img" not in members_written

    partial_cpio.replace(OUT_CPIO)
    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"[T705 BUILDER] Rebuilt CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")
    print(f"[T705 BUILDER] Total members in rebuilt CPIO: {len(members_written)}")

    # 4. DTB byte-exact alignment
    print("[T705 BUILDER] Aligning DTB initrd-end...")
    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    end_prop_str = f"<0x{high_end:02x} 0x{low_end:08x}>"
    print(f"[T705 BUILDER] linux,initrd-start = <0x01 0x10000000>")
    print(f"[T705 BUILDER] linux,initrd-end   = {end_prop_str} (decimal: {INITRD_END})")

    # Decompile base DTB to DTS
    dts_content = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(BASE_DTB)]).decode("utf-8")
    
    # Replace linux,initrd-end line and add virtio_mmio@a000000 node
    virtio_node = """\tvirtio_mmio@a000000 {
\t\tdma-coherent;
\t\tinterrupts = <0x00 0x10 0x01>;
\t\treg = <0x00 0xa000000 0x00 0x200>;
\t\tcompatible = "virtio,mmio";
\t};
"""
    new_dts_lines = []
    replaced_dtb = False
    for line in dts_content.splitlines():
        if "linux,initrd-end" in line:
            new_dts_lines.append(f"\t\tlinux,initrd-end = {end_prop_str};")
            replaced_dtb = True
        elif "\taliases {" in line:
            new_dts_lines.append(virtio_node)
            new_dts_lines.append(line)
        else:
            new_dts_lines.append(line)
    assert replaced_dtb, "Failed to find linux,initrd-end in DTB"

    dts_file = OUT_DTB.with_suffix(".dts")
    dts_file.write_text("\n".join(new_dts_lines) + "\n")

    # Compile new DTB
    subprocess.check_call([DTC_BIN, "-I", "dts", "-O", "dtb", "-o", str(OUT_DTB), str(dts_file)])
    dtb_size = OUT_DTB.stat().st_size
    dtb_sha = sha256_file(OUT_DTB)
    print(f"[T705 BUILDER] Rebuilt DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    # Verify decompiled DTB
    verify_dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {end_prop_str};" in verify_dts, "Verification of DTB failed!"
    print(f"[T705 BUILDER] DTB round-trip verified successfully!")
    print(f"[T705 BUILDER] Done in {time.time() - t0:.2f}s")

if __name__ == "__main__":
    main()
