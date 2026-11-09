#!/usr/bin/env python3
"""
Track T708: Rebuild fix initramfs with:
1. PRIMARY: service.sf.prime_shader_cache=0 in system/etc/ramdisk/build.prop,
   second_stage_resources/system/etc/ramdisk/build.prop, system/etc/prop.default,
   build.prop, system/build.prop, and t636-diag.sh.
2. SECONDARY: updated vendor.img (contains Composer client reconnect logic).
3. Re-align DTB linux,initrd-end byte-exactly per T699/T705/T707 procedure.
"""

import gzip
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track708-primecache-disable")
BASE_CPIO = WORKTREE / "guest-inputs/t707-systemserver-fb0-fix.cpio.gz"
NEW_VENDOR = WORKTREE / "guest-inputs/t708-vendor.img"
OUT_CPIO = WORKTREE / "guest-inputs/t708-systemserver-fb0-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/t707-modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t708-modified.dtb"

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
    return hdr[:54] + f"{filesize:08x}".encode("ascii") + hdr[62:]

PROP_ADDITION = b"\n# T708: disable SurfaceFlinger shader cache priming\nservice.sf.prime_shader_cache=0\n"

TARGET_PROPS = {
    "system/etc/ramdisk/build.prop",
    "second_stage_resources/system/etc/ramdisk/build.prop",
    "system/etc/prop.default",
    "build.prop",
    "system/build.prop",
}

def main():
    t0 = time.time()
    print("[T708 BUILDER] Starting initramfs repack...")
    assert BASE_CPIO.is_file(), f"Base CPIO not found: {BASE_CPIO}"
    assert NEW_VENDOR.is_file(), f"New vendor.img not found: {NEW_VENDOR}"
    assert BASE_DTB.is_file(), f"Base DTB not found: {BASE_DTB}"

    vendor_size = NEW_VENDOR.stat().st_size
    vendor_sha = sha256_file(NEW_VENDOR)
    print(f"[T708 BUILDER] New vendor.img: {vendor_size} bytes, sha256: {vendor_sha}")

    partial_cpio = OUT_CPIO.with_suffix(".partial")
    members_written = []
    replaced_vendor = False
    patched_targets = set()
    patched_diag = False

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
                print(f"[T708 BUILDER] Replacing {name_str} ({filesize} -> {vendor_size} bytes)...")
                new_hdr = make_header(hdr, vendor_size)
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                with open(NEW_VENDOR, "rb") as vf:
                    while chunk := vf.read(8 * 1024 * 1024):
                        fout.write(chunk)
                p_new = pad4(vendor_size)
                if p_new:
                    fout.write(b"\0" * p_new)
                fin.seek(filesize + p_file, 1)
                replaced_vendor = True

            elif name_str in TARGET_PROPS:
                data = fin.read(filesize)
                if p_file: fin.read(p_file)
                new_data = data + PROP_ADDITION
                print(f"[T708 BUILDER] Patching {name_str} ({filesize} -> {len(new_data)} bytes)...")
                new_hdr = make_header(hdr, len(new_data))
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name: fout.write(b"\0" * p_name)
                fout.write(new_data)
                p_new = pad4(len(new_data))
                if p_new: fout.write(b"\0" * p_new)
                patched_targets.add(name_str)

            elif name_str == "t636-diag.sh":
                data = fin.read(filesize)
                if p_file: fin.read(p_file)
                text = data.decode("utf-8", "replace")
                addition = "    /system/bin/setprop service.sf.prime_shader_cache 0 2>/dev/null || :\n"
                marker = "/system/bin/setprop dalvik.vm.dexopt.disable true 2>/dev/null || :\n"
                if marker in text:
                    text = text.replace(marker, marker + addition, 1)
                else:
                    text += "\n" + addition
                new_data = text.encode("utf-8")
                print(f"[T708 BUILDER] Patching {name_str} ({filesize} -> {len(new_data)} bytes)...")
                new_hdr = make_header(hdr, len(new_data))
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name: fout.write(b"\0" * p_name)
                fout.write(new_data)
                p_new = pad4(len(new_data))
                if p_new: fout.write(b"\0" * p_new)
                patched_diag = True

            else:
                fout.write(hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                left = filesize + p_file
                while left > 0:
                    chunk = fin.read(min(left, 8 * 1024 * 1024))
                    if not chunk:
                        break
                    fout.write(chunk)
                    left -= len(chunk)

            members_written.append(name_str)

    assert replaced_vendor, "CRITICAL: ramdisk-images/vendor.img was NOT found or replaced!"
    assert patched_targets == TARGET_PROPS, f"CRITICAL: Not all prop targets patched! Found: {patched_targets}"
    assert patched_diag, "CRITICAL: t636-diag.sh was NOT found or patched!"

    partial_cpio.replace(OUT_CPIO)

    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"[T708 BUILDER] Rebuilt CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")
    print(f"[T708 BUILDER] Total members in rebuilt CPIO: {len(members_written)}")

    # DTB alignment
    print("[T708 BUILDER] Aligning DTB initrd-end...")
    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    end_prop_str = f"<0x{high_end:02x} 0x{low_end:08x}>"
    print(f"[T708 BUILDER] linux,initrd-start = <0x01 0x10000000>")
    print(f"[T708 BUILDER] linux,initrd-end   = {end_prop_str} (decimal: {INITRD_END})")

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
    print(f"[T708 BUILDER] Rebuilt DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    # Verify decompiled DTB
    verify_dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {end_prop_str};" in verify_dts, "Verification of DTB failed!"
    print(f"[T708 BUILDER] DTB round-trip verified successfully!")
    print(f"[T708 BUILDER] Done in {time.time() - t0:.2f}s")

if __name__ == "__main__":
    main()
