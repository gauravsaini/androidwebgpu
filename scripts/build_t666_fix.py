#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T666:
system-server-art-odrefresh-boot-profile-recompile-zygote-segv (Chain A)

PINNED MECHANISM:
1. Root Cause:
   In T665, modifying services.jar inside system.img changed classes2.dex CRC32 from 0xfa0310b9
   to 0xdf90e1bc, which invalidated the precompiled ART boot profile signatures (boot-image.prof
   and cache-info.xml). At boot, odrefresh detected the profile checksum mismatch, exited with code 79
   (kCompilationRequired), and invoked dex2oat64 to recompile the boot classpath into
   /data/misc/apexdata/com.android.art/dalvik-cache/arm64/. When ZygoteInit started, ART loaded the newly
   compiled boot artifacts, causing ZygoteInit to crash with SIGSEGV (SEGV_MAPERR, fault addr 0x0)
   in art::ArtField::GetDeclaringClass (#04 boot-framework.oat ZygoteInit.main+2164) before system_server
   could launch.

2. Pinned Fix Point (Filesystem Layer ONLY):
   The underlying wall from T665 (UserDataPreparer failing on tmpfs /data because Linux 4.19 lacks
   shmem_user_xattr_handler for user.* xattrs) must be solved at the filesystem layer without touching
   system.img or any boot-classpath JAR.
   On ext4, user.* extended attributes are natively supported (fs/ext4/xattr.c).
   Calling Os.getxattr(path, "user.serial") on uninitialized paths returns ENODATA (errno 61) instead of
   ENOTSUP (errno 95). UserDataPreparer.getSerialNumber natively catches ENODATA and returns -1.
   enforceSerialNumber logs "Serial number missing on ...; assuming current is valid" and calls
   setSerialNumber, which executes Os.setxattr(path, "user.serial", "0", 0).
   On ext4, setxattr succeeds without throwing IOException, prepareUserDataLI succeeds, sets
   sys.user.0.ce_available=true, and user unlock completes (finishUserUnlocking).
   The wipe path is never entered, uncrypt is never called, and no recovery reboot crash occurs.
   system.img remains byte-identical to T664 (SHA 7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08),
   so ART boot profile signatures remain valid and odrefresh exit(79) count is 0.

3. Mount Scope:
   UserDataPreparer.enforceSerialNumber checks exactly four paths:
     1. /data/user_de/0
     2. /data/system_de/0
     3. /data/user/0 (bind-mounted to /data/data)
     4. /data/system_ce/0
   All four paths reside within /data, which is mounted as ext4.
   Therefore, mounting an ext4 image on /data covers all checked paths completely.

4. Implementation:
   - Primary Shape: loop-mounted ext4 image (/ramdisk-images/userdata.img 512M) attached to /dev/loop2
     in early-ramdisk and mounted on /data with -t ext4 -o rw,nosuid,nodev,noatime,barrier=1.
   - Fallback: /dev/ram0 via kernel brd.
   - Preserve all T664 fixes intact: throttled diagnostic loops, no pre-creation of dynamic-UID
     package dirs, CE/DE mirror mounts, 0 .capex.

Base Image (T664): SHA-256 2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t666-system-server-art-odrefresh-boot-profile-recompile-zygote-segv-initramfs.cpio.gz
"""

from __future__ import annotations

import datetime
import gzip
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE_IMAGE = ROOT / "guest-inputs/t664-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t666-system-server-art-odrefresh-boot-profile-recompile-zygote-segv-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t666-manifest.json"

EXPECTED_BASE_SHA256 = "2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_SYSTEM_IMG_SHA256 = "7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08"

MKE2FS = "/opt/homebrew/opt/e2fsprogs/sbin/mke2fs"
TUNE2FS = "/opt/homebrew/opt/e2fsprogs/sbin/tune2fs"
DEBUGFS = "/opt/homebrew/opt/e2fsprogs/sbin/debugfs"
E2FSCK = "/opt/homebrew/opt/e2fsprogs/sbin/e2fsck"

CHUNK = 8 * 1024 * 1024


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def pad4(n: int) -> int:
    return (-n) & 3


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def format_cpio_entry(name: str, content: bytes, mode: int = 0o100755, ino: int = 89999) -> bytes:
    name_bytes = name.encode("ascii") + b"\0"
    namesize = len(name_bytes)
    filesize = len(content)
    hdr = (
        f"070701"
        f"{ino:08x}"
        f"{mode:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{1:08x}"
        f"{0:08x}"
        f"{filesize:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{namesize:08x}"
        f"{0:08x}"
    ).encode("ascii")

    out = bytearray()
    out.extend(hdr)
    out.extend(name_bytes)
    pad_hdr = pad4(len(out))
    out.extend(b"\0" * pad_hdr)
    out.extend(content)
    pad_data = pad4(filesize)
    out.extend(b"\0" * pad_data)
    return bytes(out)


def patch_early_ramdisk(original: str) -> str:
    """Replace tmpfs /data mount with ext4 userdata.img loop-mount."""
    old_mount_block = (
        '"$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0771 tmpfs /data\n'
        'if [ -f /t636-diag.sh ]; then\n'
        '    "$TOYBOX" cp /t636-diag.sh /data/t636-diag.sh\n'
        '    "$TOYBOX" chmod 0755 /data/t636-diag.sh\n'
        '    echo "[T636] copied /t636-diag.sh to /data/t636-diag.sh"\n'
        'fi\n'
        'echo "[T552] tmpfs userdata mounted"'
    )

    new_mount_block = (
        '# T666: Pinned filesystem-layer fix: mount ext4 on /data\n'
        '# Ext4 natively supports user.* xattrs, returning ENODATA on unset paths\n'
        '# and allowing setSerialNumber to succeed without entering the recovery wipe path.\n'
        '"$TOYBOX" mknod -m 0600 /dev/loop2 b 7 2 2>/dev/null || :\n'
        '"$TOYBOX" mknod -m 0600 /dev/block/loop2 b 7 2 2>/dev/null || :\n'
        'mounted_ext4=0\n'
        'if [ -f /ramdisk-images/userdata.img ]; then\n'
        '    "$TOYBOX" losetup /dev/loop2 /ramdisk-images/userdata.img 2>/dev/null || :\n'
        '    if "$TOYBOX" mount -t ext4 -o rw,nosuid,nodev,noatime,barrier=1 /dev/loop2 /data 2>/dev/null; then\n'
        '        mounted_ext4=1\n'
        '        echo "[T666] ext4 userdata mounted on /data via /dev/loop2"\n'
        '    fi\n'
        'fi\n'
        'if [ "$mounted_ext4" -eq 0 ]; then\n'
        '    "$TOYBOX" mknod -m 0600 /dev/ram0 b 1 0 2>/dev/null || :\n'
        '    "$TOYBOX" mknod -m 0600 /dev/block/ram0 b 1 0 2>/dev/null || :\n'
        '    if [ -f /ramdisk-images/userdata.img ]; then\n'
        '        "$TOYBOX" cp /ramdisk-images/userdata.img /dev/ram0 2>/dev/null || :\n'
        '    fi\n'
        '    if "$TOYBOX" mount -t ext4 -o rw,nosuid,nodev,noatime,barrier=1 /dev/ram0 /data 2>/dev/null; then\n'
        '        mounted_ext4=1\n'
        '        echo "[T666] ext4 userdata mounted on /data via /dev/ram0"\n'
        '    fi\n'
        'fi\n'
        'if [ "$mounted_ext4" -eq 0 ]; then\n'
        '    echo "[T666] WARNING: ext4 mount failed, falling back to tmpfs"\n'
        '    "$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0771 tmpfs /data\n'
        'else\n'
        '    "$TOYBOX" chmod 0771 /data 2>/dev/null || :\n'
        '    "$TOYBOX" chown 1000:1000 /data 2>/dev/null || :\n'
        'fi\n'
        'if [ -f /t636-diag.sh ]; then\n'
        '    "$TOYBOX" cp /t636-diag.sh /data/t636-diag.sh\n'
        '    "$TOYBOX" chmod 0755 /data/t636-diag.sh\n'
        '    echo "[T636] copied /t636-diag.sh to /data/t636-diag.sh"\n'
        'fi\n'
        'echo "[T666] userdata filesystem initialization complete"'
    )

    if old_mount_block not in original:
        raise ValueError("Could not find old mount block in early-ramdisk!")

    return original.replace(old_mount_block, new_mount_block, 1)


def patch_diag_script(script_text: str) -> str:
    """Enhance t636-diag.sh with sys.user.0.ce_available and data_fs monitoring, and ensure second-stage nodes."""
    # 1. Add ce_available and data_fs monitoring to T642_BOOT_STATE
    target_pattern = 'completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
    replacement = (
        'completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        ce_available=$(/system/bin/getprop sys.user.0.ce_available 2>/dev/null)\n'
        '        data_fs=$("$TOYBOX" stat -f -c "%T" /data 2>/dev/null || echo "unknown")\n'
    )
    if target_pattern in script_text:
        script_text = script_text.replace(target_pattern, replacement, 1)

    old_boot_marker = 'boot_completed=$completed"'
    new_boot_marker = 'boot_completed=$completed ce_available=$ce_available data_fs=$data_fs"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    # 2. In do_prep: log mount info and ensure second-stage nodes and system_ce/0
    prep_enter_marker = 'do_prep() {\n    mark "T642_DO_PREP_ENTER pid=$$"\n'
    prep_enhancement = (
        'do_prep() {\n'
        '    mark "T642_DO_PREP_ENTER pid=$$"\n'
        '    data_mnt=$("$TOYBOX" grep " /data " /proc/mounts 2>/dev/null || echo "unknown")\n'
        '    mark "T666_DATA_MOUNT_INFO $data_mnt"\n'
        '    [ -b /dev/block/loop2 ] || "$TOYBOX" mknod -m 0600 /dev/block/loop2 b 7 2 2>/dev/null || :\n'
        '    [ -b /dev/loop2 ] || "$TOYBOX" mknod -m 0600 /dev/loop2 b 7 2 2>/dev/null || :\n'
        '    "$TOYBOX" mkdir -p /data/system_ce/0 2>/dev/null || :\n'
        '    "$TOYBOX" chown 1000:1000 /data/system_ce/0 2>/dev/null || :\n'
        '    "$TOYBOX" chmod 0700 /data/system_ce/0 2>/dev/null || :\n'
    )
    if prep_enter_marker in script_text:
        script_text = script_text.replace(prep_enter_marker, prep_enhancement, 1)

    return script_text


def create_ext4_userdata_image(image_path: Path, size_mb: int = 512) -> None:
    """Format a 512MB ext4 image with user_xattr, acl, and pre-created Android directories."""
    print(f"[T666 BUILDER] Formatting {size_mb}MB ext4 userdata image at {image_path}...")
    if image_path.exists():
        image_path.unlink()

    # Format ext4 with Linux 4.19 compatible features
    cmd_mkfs = [
        MKE2FS, "-F", "-t", "ext4", "-b", "4096", "-N", "32768",
        "-O", "^orphan_file,^metadata_csum_seed,^metadata_csum",
        str(image_path), f"{size_mb}M"
    ]
    subprocess.run(cmd_mkfs, check=True, capture_output=True)

    # Set default mount options: user_xattr, acl
    cmd_tune = [TUNE2FS, "-o", "user_xattr,acl", str(image_path)]
    subprocess.run(cmd_tune, check=True, capture_output=True)

    # Configure root directory (inode 2): mode 0771, uid 1000 (system), gid 1000 (system)
    debugfs_commands = [
        "sif <2> mode 040771",
        "sif <2> uid 1000",
        "sif <2> gid 1000",
        # Pre-create standard directories for Android multi-user storage
        "mkdir user_de",
        "sif user_de mode 040771", "sif user_de uid 1000", "sif user_de gid 1000",
        "mkdir user_de/0",
        "sif user_de/0 mode 040771", "sif user_de/0 uid 1000", "sif user_de/0 gid 1000",
        "mkdir data",
        "sif data mode 040771", "sif data uid 1000", "sif data gid 1000",
        "mkdir user",
        "sif user mode 040771", "sif user uid 1000", "sif user gid 1000",
        "mkdir user/0",
        "sif user/0 mode 040771", "sif user/0 uid 1000", "sif user/0 gid 1000",
        "mkdir system_de",
        "sif system_de mode 040700", "sif system_de uid 1000", "sif system_de gid 1000",
        "mkdir system_de/0",
        "sif system_de/0 mode 040700", "sif system_de/0 uid 1000", "sif system_de/0 gid 1000",
        "mkdir system_ce",
        "sif system_ce mode 040700", "sif system_ce uid 1000", "sif system_ce gid 1000",
        "mkdir system_ce/0",
        "sif system_ce/0 mode 040700", "sif system_ce/0 uid 1000", "sif system_ce/0 gid 1000",
        "mkdir system",
        "sif system mode 040775", "sif system uid 1000", "sif system gid 1000",
        "mkdir system/users",
        "sif system/users mode 040775", "sif system/users uid 1000", "sif system/users gid 1000",
        "mkdir system/users/0",
        "sif system/users/0 mode 040700", "sif system/users/0 uid 1000", "sif system/users/0 gid 1000",
        "mkdir misc_de",
        "sif misc_de mode 040771", "sif misc_de uid 1000", "sif misc_de gid 1000",
        "mkdir misc_de/0",
        "sif misc_de/0 mode 040771", "sif misc_de/0 uid 1000", "sif misc_de/0 gid 1000",
        "mkdir misc_ce",
        "sif misc_ce mode 040771", "sif misc_ce uid 1000", "sif misc_ce gid 1000",
        "mkdir misc_ce/0",
        "sif misc_ce/0 mode 040771", "sif misc_ce/0 uid 1000", "sif misc_ce/0 gid 1000",
    ]

    for dcmd in debugfs_commands:
        subprocess.run([DEBUGFS, "-w", "-R", dcmd, str(image_path)], check=True, capture_output=True)

    # Check filesystem with e2fsck
    res = subprocess.run([E2FSCK, "-f", "-y", str(image_path)], capture_output=True, text=True)
    if res.returncode not in (0, 1):  # 0 = clean, 1 = file system errors corrected
        raise RuntimeError(f"e2fsck failed with exit {res.returncode}: {res.stderr}")

    img_size = image_path.stat().st_size
    img_sha = sha256_file(image_path)
    print(f"[T666 BUILDER] Formatted ext4 userdata.img: {img_size} bytes, SHA-256: {img_sha}")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T666 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # 1. Verify base image (T664)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T666 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # 2. Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T666 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # 3. Create ext4 userdata.img
    tmp_userdata = Path("/tmp/t666_userdata.img")
    create_ext4_userdata_image(tmp_userdata, size_mb=512)
    userdata_bytes = tmp_userdata.stat().st_size
    userdata_sha = sha256_file(tmp_userdata)

    # 4. Clean existing fix images
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    for old_img in OUTPUT_DIR.glob("t666-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T666 BUILDER] Removed old fix image: {old_img}")

    patched_ramdisk = False
    patched_diag = False
    added_userdata = False
    verified_system_img = False

    print(f"[T666 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if not hdr.startswith(b"070701"):
                raise ValueError("Malformed CPIO header!")

            ino = int(hdr[6:14], 16)
            mode = int(hdr[14:22], 16)
            uid = int(hdr[22:30], 16)
            gid = int(hdr[30:38], 16)
            nlink = int(hdr[38:46], 16)
            mtime = int(hdr[46:54], 16)
            filesize = int(hdr[54:62], 16)
            namesize = int(hdr[94:102], 16)

            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("ascii", errors="replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "early-ramdisk":
                print(f"[T666 BUILDER] Patching {name_str} ({filesize} bytes)...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                script_text = raw_script.decode("utf-8")
                patched_text = patch_early_ramdisk(script_text)
                patched_bytes = patched_text.encode("utf-8")
                entry = format_cpio_entry("early-ramdisk", patched_bytes, mode=mode, ino=ino)
                fout.write(entry)
                patched_ramdisk = True
                print(f"[T666 BUILDER] Patched {name_str}: {len(raw_script)} -> {len(patched_bytes)} bytes.")

            elif name_str == "t636-diag.sh":
                print(f"[T666 BUILDER] Patching {name_str} ({filesize} bytes)...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                script_text = raw_script.decode("utf-8")
                patched_text = patch_diag_script(script_text)
                patched_bytes = patched_text.encode("utf-8")
                entry = format_cpio_entry("t636-diag.sh", patched_bytes, mode=mode, ino=ino)
                fout.write(entry)
                patched_diag = True
                print(f"[T666 BUILDER] Patched {name_str}: {len(raw_script)} -> {len(patched_bytes)} bytes.")

            elif name_str == "ramdisk-images/system.img":
                # Verify system.img hash while streaming UNTOUCHED
                print(f"[T666 BUILDER] Streaming system.img ({filesize} bytes) and verifying SHA-256...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                h_sys = hashlib.sha256()
                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError("Unexpected EOF reading system.img data!")
                    h_sys.update(c)
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

                actual_sys_sha = h_sys.hexdigest()
                if actual_sys_sha != EXPECTED_SYSTEM_IMG_SHA256:
                    raise ValueError(f"system.img SHA mismatch! {actual_sys_sha} != {EXPECTED_SYSTEM_IMG_SHA256}")
                verified_system_img = True
                print(f"[T666 BUILDER] Verified system.img byte-identical: {actual_sys_sha} OK.")

            elif name_str == "ramdisk-images/vendor.img":
                # Stream vendor.img, then immediately inject ramdisk-images/userdata.img
                print(f"[T666 BUILDER] Streaming vendor.img ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError("Unexpected EOF reading vendor.img data!")
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

                # Inject ramdisk-images/userdata.img
                print(f"[T666 BUILDER] Injecting ramdisk-images/userdata.img ({userdata_bytes} bytes)...")
                ud_name = b"ramdisk-images/userdata.img\0"
                ud_namesize = len(ud_name)
                ud_hdr = (
                    f"070701"
                    f"{90001:08x}"
                    f"{0o100644:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{1:08x}"
                    f"{0:08x}"
                    f"{userdata_bytes:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{ud_namesize:08x}"
                    f"{0:08x}"
                ).encode("ascii")
                fout.write(ud_hdr)
                fout.write(ud_name)
                pad_ud_name = pad4(110 + ud_namesize)
                if pad_ud_name:
                    fout.write(b"\0" * pad_ud_name)

                with open(tmp_userdata, "rb") as f_ud:
                    while True:
                        c = f_ud.read(CHUNK)
                        if not c:
                            break
                        fout.write(c)
                pad_ud_data = pad4(userdata_bytes)
                if pad_ud_data:
                    fout.write(b"\0" * pad_ud_data)
                added_userdata = True
                print(f"[T666 BUILDER] Injected ramdisk-images/userdata.img OK.")

            else:
                # Stream other files unchanged
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError(f"Unexpected EOF reading {name_str} data!")
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

    if not patched_ramdisk:
        raise RuntimeError("early-ramdisk was not patched!")
    if not patched_diag:
        raise RuntimeError("t636-diag.sh was not patched!")
    if not added_userdata:
        raise RuntimeError("ramdisk-images/userdata.img was not added!")
    if not verified_system_img:
        raise RuntimeError("ramdisk-images/system.img was not verified!")

    # Cleanup temp userdata image
    if tmp_userdata.exists():
        tmp_userdata.unlink()

    # Move to final output image atomically
    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_bytes = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0

    print(f"[T666 BUILDER] Fix image created successfully!")
    print(f"[T666 BUILDER] Path: {OUTPUT_IMAGE}")
    print(f"[T666 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T666",
        "description": "system-server-art-odrefresh-boot-profile-recompile-zygote-segv",
        "pinned_mechanism": "Mount ext4 filesystem on /data in early-ramdisk with native user.* xattr support, avoiding UserDataPreparer wipe path and keeping system.img untouched to preserve ART boot profiles",
        "base_image": {
            "path": str(BASE_IMAGE),
            "sha256": base_sha,
            "bytes": base_stat.st_size,
            "st_nlink": base_stat.st_nlink,
        },
        "canonical_kernel": {
            "path": str(kernel_path),
            "sha256": kernel_sha,
        },
        "system_img_sha256": EXPECTED_SYSTEM_IMG_SHA256,
        "userdata_img": {
            "size_bytes": userdata_bytes,
            "sha256": userdata_sha,
            "fs_type": "ext4",
            "features": "ext_attr,dir_index,filetype,extent,flex_bg,sparse_super,large_file,huge_file,dir_nlink,extra_isize",
            "mount_options": "user_xattr,acl"
        },
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": fix_sha,
            "bytes": fix_bytes,
        },
        "built_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    print(f"[T666 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
