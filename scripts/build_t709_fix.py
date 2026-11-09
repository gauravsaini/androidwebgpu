#!/usr/bin/env python3
"""
Track T709: Rebuild fix initramfs with:
1. PRIMARY: logcat_streamer static binary added to initramfs root and launched in early-ramdisk (chroot to PID 1 root, streaming /system/bin/logcat -b all -v time *:V to /dev/console with auto-restart + crash watcher).
2. Re-align DTB linux,initrd-end byte-exactly per T699/T705/T707/T708 procedure.
"""

import gzip
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track709-logcat-systemserver-cure")
BASE_CPIO = WORKTREE / "guest-inputs/t708-systemserver-fb0-fix.cpio.gz"
OUT_CPIO = WORKTREE / "guest-inputs/t709-systemserver-fb0-fix.cpio.gz"
BASE_DTB = WORKTREE / "guest-inputs/t708-modified.dtb"
OUT_DTB = WORKTREE / "guest-inputs/t709-modified.dtb"
STREAMER_BIN = WORKTREE / "guest-inputs/logcat_streamer"

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

def make_new_file_header(name: str, filesize: int, mode: int = 0o100755, ino: int = 1001) -> tuple[bytes, bytes]:
    name_bytes = name.encode("ascii") + b"\0"
    namesize = len(name_bytes)
    hdr = f"070701{ino:08x}{mode:08x}00000000000000000000000100000000{filesize:08x}00000000000000000000000000000000{namesize:08x}00000000".encode("ascii")
    return hdr, name_bytes

def main():
    t0 = time.time()
    print("[T709 BUILDER] Starting initramfs repack...")
    assert BASE_CPIO.is_file(), f"Base CPIO not found: {BASE_CPIO}"
    assert BASE_DTB.is_file(), f"Base DTB not found: {BASE_DTB}"
    assert STREAMER_BIN.is_file(), f"Streamer binary not found: {STREAMER_BIN}"

    streamer_size = STREAMER_BIN.stat().st_size
    streamer_sha = sha256_file(STREAMER_BIN)
    print(f"[T709 BUILDER] Adding logcat_streamer: {streamer_size} bytes, sha256: {streamer_sha}")

    partial_cpio = OUT_CPIO.with_suffix(".partial")
    members_written = []
    patched_early_ramdisk = False
    added_streamer = False

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

            # Insert logcat_streamer before TRAILER!!!
            if name_str == "TRAILER!!!" and not added_streamer:
                print(f"[T709 BUILDER] Inserting logcat_streamer into CPIO...")
                s_hdr, s_name_bytes = make_new_file_header("logcat_streamer", streamer_size)
                fout.write(s_hdr)
                fout.write(s_name_bytes)
                p_s_name = pad4(110 + len(s_name_bytes))
                if p_s_name: fout.write(b"\0" * p_s_name)
                with open(STREAMER_BIN, "rb") as sf:
                    while chunk := sf.read(8 * 1024 * 1024):
                        fout.write(chunk)
                p_s_file = pad4(streamer_size)
                if p_s_file: fout.write(b"\0" * p_s_file)
                members_written.append("logcat_streamer")
                added_streamer = True

            if name_str == "early-ramdisk":
                data = fin.read(filesize)
                if p_file: fin.read(p_file)

                # Secondary Fix: Pre-create /data/user_de/0/com.android.providers.settings/databases
                # to prevent SQLiteCantOpenDatabaseException during system_server SettingsProvider init.
                sec_target = b'echo "[T670] /data/.../com.android.media.swcodec pre-created (0771 1000:1000)"\n'
                assert sec_target in data, "CRITICAL: T670 marker not found in early-ramdisk!"
                sec_patch = b"""
# T709 Secondary Fix: Pre-create SettingsProvider & package databases in ext4 /data
for p in com.android.providers.settings com.android.shell com.android.systemui com.android.launcher3; do
    "$TOYBOX" mkdir -p "/data/user_de/0/$p/databases" "/data/data/$p/databases" "/data/user/0/$p/databases" 2>/dev/null || :
    "$TOYBOX" chmod 0777 "/data/user_de/0/$p" "/data/user_de/0/$p/databases" 2>/dev/null || :
    "$TOYBOX" chown 1000:1000 "/data/user_de/0/$p" "/data/user_de/0/$p/databases" 2>/dev/null || :
    "$TOYBOX" chmod 0777 "/data/data/$p" "/data/data/$p/databases" 2>/dev/null || :
    "$TOYBOX" chown 1000:1000 "/data/data/$p" "/data/data/$p/databases" 2>/dev/null || :
    "$TOYBOX" chmod 0777 "/data/user/0/$p" "/data/user/0/$p/databases" 2>/dev/null || :
    "$TOYBOX" chown 1000:1000 "/data/user/0/$p" "/data/user/0/$p/databases" 2>/dev/null || :
done
"$TOYBOX" mkdir -p /data/system/users/0 /data/system/dropbox /data/system_de/0 /data/system_ce/0 2>/dev/null || :
"$TOYBOX" chmod 0777 /data/user_de /data/user_de/0 /data/data /data/user /data/user/0 /data/system/dropbox /data/system_de /data/system_de/0 /data/system_ce /data/system_ce/0 2>/dev/null || :
"$TOYBOX" chown -R 1000:1000 /data/user_de /data/system_de /data/system_ce /data/system/dropbox 2>/dev/null || :
echo "[T709] /data/user_de/0/com.android.providers.settings/databases pre-created (0777 1000:1000)"
"""
                new_data = data.replace(sec_target, sec_target + sec_patch, 1)

                # Primary Fix: Add /logcat_streamer launch before exec /init
                assert b"exec /init" in new_data, "CRITICAL: exec /init not found in early-ramdisk!"
                launch_marker = b"/logcat_streamer >/dev/console 2>&1 &\nexec /init"
                new_data = new_data.replace(b"exec /init", launch_marker, 1)

                print(f"[T709 BUILDER] Patching {name_str} ({filesize} -> {len(new_data)} bytes)...")
                new_hdr = make_header(hdr, len(new_data))
                fout.write(new_hdr)
                fout.write(name_bytes)
                if p_name: fout.write(b"\0" * p_name)
                fout.write(new_data)
                p_new = pad4(len(new_data))
                if p_new: fout.write(b"\0" * p_new)
                patched_early_ramdisk = True

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

    assert patched_early_ramdisk, "CRITICAL: early-ramdisk was NOT found or patched!"
    assert added_streamer, "CRITICAL: logcat_streamer was NOT added to CPIO!"

    partial_cpio.replace(OUT_CPIO)

    cpio_size = OUT_CPIO.stat().st_size
    cpio_sha = sha256_file(OUT_CPIO)
    print(f"[T709 BUILDER] Rebuilt CPIO: {OUT_CPIO} ({cpio_size} bytes, sha256: {cpio_sha})")
    print(f"[T709 BUILDER] Total members in rebuilt CPIO: {len(members_written)}")

    # DTB alignment
    print("[T709 BUILDER] Aligning DTB initrd-end...")
    INITRD_START = 0x110000000
    INITRD_END = INITRD_START + cpio_size
    high_end = (INITRD_END >> 32) & 0xFFFFFFFF
    low_end = INITRD_END & 0xFFFFFFFF
    end_prop_str = f"<0x{high_end:02x} 0x{low_end:08x}>"
    print(f"[T709 BUILDER] linux,initrd-start = <0x01 0x10000000>")
    print(f"[T709 BUILDER] linux,initrd-end   = {end_prop_str} (decimal: {INITRD_END})")

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
    print(f"[T709 BUILDER] Rebuilt DTB: {OUT_DTB} ({dtb_size} bytes, sha256: {dtb_sha})")

    # Verify decompiled DTB
    verify_dts = subprocess.check_output([DTC_BIN, "-I", "dtb", "-O", "dts", str(OUT_DTB)]).decode("utf-8")
    assert f"linux,initrd-end = {end_prop_str};" in verify_dts, "Verification of DTB failed!"
    print(f"[T709 BUILDER] DTB round-trip verified successfully!")
    print(f"[T709 BUILDER] Done in {time.time() - t0:.2f}s")

if __name__ == "__main__":
    main()
