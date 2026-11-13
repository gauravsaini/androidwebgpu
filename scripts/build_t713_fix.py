#!/usr/bin/env python3
"""
Track T713: Rebuild fix initramfs with:
1. Option C bypass: system_init.startsensorservice=0 configured across all early boot property files.
2. Property context labeled as system_prop in plat_property_contexts.
3. Early ramdisk logging & setprop backup in run_probe.
4. DTB linux,initrd-end re-aligned byte-exactly to the new CPIO size per T699 procedure.
"""

import gzip
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track713-sensorstart-bypass")
BASE_CPIO = WORKTREE / "guest-inputs/t712-fix.cpio.gz"
OUT_CPIO = WORKTREE / "guest-inputs/t713-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/t712-modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t713-modified.dtb"

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

PROP_APPEND = b"\n# T713: Option C bypass for in-process startSensorServiceNative\nsystem_init.startsensorservice=0\n"
CONTEXT_APPEND = b"\nsystem_init.startsensorservice u:object_r:system_prop:s0 exact bool\n"

def main():
    t0 = time.time()
    print("[T713 BUILDER] Starting Option C bypass initramfs repack...")
    assert BASE_CPIO.is_file(), f"Base CPIO not found: {BASE_CPIO}"
    assert BASE_DTB.is_file(), f"Base DTB not found: {BASE_DTB}"
    assert os.path.exists(DTC_BIN), f"dtc not found at {DTC_BIN}"

    partial_cpio = OUT_CPIO.with_suffix(".partial")
    members_written = []
    modified_members = {}

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

            content = fin.read(filesize)
            if p_file:
                fin.read(p_file)

            # Check if this member should be modified
            new_content = None
            if name_str in (
                "system/etc/prop.default",
                "second_stage_resources/system/etc/ramdisk/build.prop",
                "system/build.prop",
                "system/etc/build.prop",
                "system/etc/ramdisk/build.prop",
                "build.prop",
            ):
                new_content = content + PROP_APPEND
                print(f"[T713 BUILDER] Appended system_init.startsensorservice=0 to {name_str} ({filesize} -> {len(new_content)} bytes)")
                modified_members[name_str] = len(new_content)
            elif name_str in (
                "plat_property_contexts",
                "system/etc/selinux/plat_property_contexts",
            ):
                new_content = content + CONTEXT_APPEND
                print(f"[T713 BUILDER] Appended system_prop context to {name_str} ({filesize} -> {len(new_content)} bytes)")
                modified_members[name_str] = len(new_content)
            elif name_str == "early-ramdisk":
                # Add probe log and backup setprop
                script_text = content.decode("utf-8", "replace")
                old_probe = 'echo "[T552-PROBE] guest in-process probe daemon armed"'
                new_probe = (
                    'echo "[T552-PROBE] guest in-process probe daemon armed"\n'
                    '    /system/bin/setprop system_init.startsensorservice 0 2>/dev/null || true\n'
                    '    echo "[T713-PROBE] early system_init.startsensorservice configured: \'$(/system/bin/getprop system_init.startsensorservice)\'"'
                )
                if old_probe in script_text:
                    script_text = script_text.replace(old_probe, new_probe, 1)
                    new_content = script_text.encode("utf-8")
                    print(f"[T713 BUILDER] Updated early-ramdisk with T713 probe lines ({filesize} -> {len(new_content)} bytes)")
                    modified_members[name_str] = len(new_content)
                else:
                    print("[T713 BUILDER] WARNING: could not locate probe marker in early-ramdisk")

            if new_content is not None:
                new_filesize = len(new_content)
                new_hdr = make_header(hdr, new_filesize)
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                fout.write(new_content)
                p_new = pad4(new_filesize)
                if p_new:
                    fout.write(b"\0" * p_new)
            else:
                fout.write(hdr)
                fout.write(name_bytes)
                if p_name:
                    fout.write(b"\0" * p_name)
                fout.write(content)
                if p_file:
                    fout.write(b"\0" * p_file)

            members_written.append(name_str)

    assert len(modified_members) >= 7, f"CRITICAL: expected >= 7 modified members, got {len(modified_members)}: {list(modified_members.keys())}"

    partial_cpio.replace(OUT_CPIO)

    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"[T713 BUILDER] Rebuilt CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")
    print(f"[T713 BUILDER] Total members in rebuilt CPIO: {len(members_written)}")

    # DTB alignment
    print("[T713 BUILDER] Aligning DTB initrd-end...")
    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    end_prop_str = f"<0x{high_end:02x} 0x{low_end:08x}>"
    print(f"[T713 BUILDER] linux,initrd-start = <0x01 0x10000000>")
    print(f"[T713 BUILDER] linux,initrd-end   = {end_prop_str} (decimal: {INITRD_END}, hex: {hex(INITRD_END)})")

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
    print(f"[T713 BUILDER] Rebuilt DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    # Verify decompiled DTB
    verify_dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {end_prop_str};" in verify_dts, "Verification of DTB failed!"
    print(f"[T713 BUILDER] DTB round-trip verified successfully!")
    print(f"[T713 BUILDER] Done in {time.time() - t0:.2f}s")

if __name__ == "__main__":
    main()
