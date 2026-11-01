#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T661:
system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile (Chain A)

PINNED MECHANISM:
In init.rc, /data/user_de is bind-mounted onto /data_mirror/data_de/null during post-fs-data,
and init_user0 prepares /data/user_de/0.
During PackageManagerService.systemReady(), UserManagerService.reconcileUsers() -> UserDataPreparer
calls enforceSerialNumber() which invokes Os.getxattr(path, 'user.serial').
On this virtual guest, /data is mounted on tmpfs under Linux 4.19.130 (guest-inputs/Image).
Linux 4.19 tmpfs does not support user.* extended attributes (shmem_user_xattr_handler added in Linux 6.6),
returning ENOTSUP (errno 95). UserDataPreparer catches this and calls vold.destroyUserStorage(volumeUuid, 0, flags).
vold invokes destroy_dir("/data/user_de/0"), calling rmdir("/data/user_de/0").
While /data/user/0 survived because /data/data was bind-mounted onto it (causing rmdir to return EBUSY),
/data/user_de/0 was NOT a mount point, so rmdir succeeded, wiping /data/user_de/0 and /data_mirror/data_de/null/0.
When PermissionPolicyService reached Phase 550 and bound PermissionControllerService, ActivityManager
forked com.android.permissioncontroller and com.android.systemui.
In com_android_internal_os_Zygote.cpp:800 (mountAppData),
mount("/data_mirror/data_de/null/0/<pkg>", "/data/user_de/0/<pkg>", MS_BIND | MS_REC) failed with ENOENT
because the mirror path and target directory were destroyed, triggering JNI FatalError (SIGABRT).

FIX:
Supply and preserve /data/user_de/0 and /data_mirror/data_de/null/0 by:
1. In do_prep:
   - Ensure /data/user_de/0 exists (mode 0771 system:system).
   - Self-bind mount user 0 DE directories (/data/user_de/0, /data/misc_de/0, /data/system_de/0, etc.)
     so that vold.destroyUserStorage's rmdir returns EBUSY (matching /data/user/0's survival mechanism).
   - Ensure /data_mirror/data_de/null/0 exists and bind mount /data/user_de/0 onto it.
   - Pre-create and provision DE directories for critical apps (com.android.permissioncontroller,
     com.android.systemui, com.android.providers.settings).
2. In background daemon:
   - Run a continuous watcher (watch_de_mirrors) that maintains the self-bind mount on /data/user_de/0,
     the mirror mount on /data_mirror/data_de/null/0, and scans /data/data/* to continuously provision
     and maintain DE directories for every app process.

Base Image (T660): SHA-256 b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile-initramfs.cpio.gz
"""

from __future__ import annotations

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

ROOT = Path("/Users/Shared/wt-track661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile")
BASE_IMAGE = ROOT / "guest-inputs/t660-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t661-manifest.json"

EXPECTED_BASE_SHA256 = "b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

CHUNK = 8 * 1024 * 1024


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


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def patch_diag_script(script_text: str) -> bytes:
    """Insert T661 prep and continuous mirror watcher into t636-diag.sh."""
    # 1. Insertion in do_prep(): right after SettingsProvider databases setup
    anchor_prep = '    mark "T654_PREP_SENSORS_MULTIHAL"'
    if anchor_prep not in script_text:
        raise ValueError(f"Anchor '{anchor_prep}' not found in t636-diag.sh!")

    t661_prep_block = """    # T661: Pinned fix for /data_mirror/data_de/null and /data/user_de/0
    mark "T661_PREP_USER_DE_AND_MIRROR_ENTER"

    # 1. Ensure /data/user_de/0 exists with mode 0771 system:system
    "$TOY" mkdir -p /data/user_de/0 2>/dev/null || :
    "$TOY" chown 1000:1000 /data/user_de/0 2>/dev/null || :
    "$TOY" chmod 0771 /data/user_de/0 2>/dev/null || :

    # 2. Self-bind mount user 0 DE directories so vold destroyUserStorage rmdir returns EBUSY
    # This prevents vold from wiping out /data/user_de/0 when UserDataPreparer fails getxattr on tmpfs
    for d in /data/user_de/0 /data/misc_de/0 /data/system_de/0 /data/vendor_de/0 /data/system/users/0 /data/misc/profiles/cur/0; do
        "$TOY" mkdir -p "$d" 2>/dev/null || :
        "$TOY" mount -o bind "$d" "$d" 2>/dev/null || :
    done

    # 3. Ensure /data_mirror directories exist and mirror user 0
    "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :
    "$TOY" mount -o bind /data/user/0 /data_mirror/data_ce/null/0 2>/dev/null || :

    # 4. Pre-create, provision, and self-bind mount critical Phase 550 app DE directories in BOTH locations:
    # (a) com.android.permissioncontroller (UID 10097, GID 10097)
    "$TOY" mkdir -p /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10097:10097 /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.permissioncontroller /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10097:10097 /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.permissioncontroller /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :

    # (b) com.android.systemui (UID 10083, GID 10083)
    "$TOY" mkdir -p /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.systemui /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.systemui /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :

    # (c) com.android.providers.settings (UID 1000, GID 1000)
    "$TOY" mkdir -p /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.providers.settings /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_de/null/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data_mirror/data_de/null/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.providers.settings /data_mirror/data_de/null/0/com.android.providers.settings 2>/dev/null || :

    if [ -d /data_mirror/data_de/null/0/com.android.permissioncontroller ] && [ -d /data_mirror/data_de/null/0/com.android.systemui ]; then
        mark "T661_PREP_USER_DE_AND_MIRROR_VERIFIED_OK"
    else
        mark "T661_PREP_USER_DE_AND_MIRROR_VERIFIED_FAILED"
    fi

"""
    script_text = script_text.replace(anchor_prep, t661_prep_block + anchor_prep, 1)

    # 2. Insertion in background daemon: add watch_de_mirrors &
    anchor_bg = "watch_boot_state &\n"
    if anchor_bg not in script_text:
        raise ValueError(f"Anchor '{anchor_bg}' not found in t636-diag.sh!")

    t661_bg_watcher = """# 7. DE Mirror & App Directory Watcher
watch_de_mirrors() {
    mark "T661_DE_MIRROR_WATCHER_STARTED"
    while :; do
        # 1. Maintain self-bind on /data/user_de/0
        if [ ! -d /data/user_de/0 ]; then
            "$TOY" mkdir -p /data/user_de/0 2>/dev/null || :
            "$TOY" chown 1000:1000 /data/user_de/0 2>/dev/null || :
            "$TOY" chmod 0771 /data/user_de/0 2>/dev/null || :
            "$TOY" mount -o bind /data/user_de/0 /data/user_de/0 2>/dev/null || :
        fi
        # 2. Maintain /data_mirror/data_de/null/0
        if [ ! -d /data_mirror/data_de/null/0 ]; then
            "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
            "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :
        fi

        # 3. High-priority Phase 550 packages: check and self-bind immediately
        for spec in "com.android.permissioncontroller:10097:10097" "com.android.systemui:10083:10083" "com.android.providers.settings:1000:1000"; do
            pkg=${spec%%:*}
            ug=${spec#*:}
            uid=${ug%%:*}
            gid=${ug#*:}
            if [ ! -d "/data/user_de/0/$pkg" ]; then
                "$TOY" mkdir -p "/data/user_de/0/$pkg" 2>/dev/null || :
                "$TOY" chown "$uid:$gid" "/data/user_de/0/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data/user_de/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "/data/user_de/0/$pkg" "/data/user_de/0/$pkg" 2>/dev/null || :
            fi
            if [ ! -d "/data_mirror/data_de/null/0/$pkg" ]; then
                "$TOY" mkdir -p "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" chown "$uid:$gid" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "/data/user_de/0/$pkg" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
            fi
        done

        # 4. Mirror all packages in /data/data into /data/user_de/0 and /data_mirror/data_de/null/0
        for pdir in /data/data/* /data/user/0/*; do
            [ -d "$pdir" ] || continue
            pkg=${pdir##*/}
            [ -n "$pkg" ] || continue
            de_target="/data/user_de/0/$pkg"
            if [ ! -d "$de_target" ]; then
                "$TOY" mkdir -p "$de_target" 2>/dev/null || :
                ug=$("$TOY" stat -c "%u:%g" "$pdir" 2>/dev/null)
                if [ -n "$ug" ]; then
                    "$TOY" chown "$ug" "$de_target" 2>/dev/null || :
                else
                    "$TOY" chown 1000:1000 "$de_target" 2>/dev/null || :
                fi
                "$TOY" chmod 0700 "$de_target" 2>/dev/null || :
                "$TOY" mount -o bind "$de_target" "$de_target" 2>/dev/null || :
                if [ ! -d "/data_mirror/data_de/null/0/$pkg" ]; then
                    "$TOY" mkdir -p "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                    if [ -n "$ug" ]; then
                        "$TOY" chown "$ug" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                    fi
                    "$TOY" chmod 0700 "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                    "$TOY" mount -o bind "$de_target" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                fi
                mark "T661_PROVISIONED_DE_DIR pkg=$pkg ug=$ug"
            fi
        done
        "$TOY" sleep 0.05 2>/dev/null || :
    done
}
watch_de_mirrors &

"""
    script_text = script_text.replace(anchor_bg, anchor_bg + t661_bg_watcher, 1)

    # 3. Add user_de and mirror_de status to watch_boot_state
    old_boot_state = 'completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n        mark "T642_BOOT_STATE'
    new_boot_state = """        user_de_status="missing"
        if [ -d /data/user_de/0/com.android.permissioncontroller ]; then
            user_de_status="ok"
        fi
        mirror_de_status="missing"
        if [ -d /data_mirror/data_de/null/0/com.android.permissioncontroller ]; then
            mirror_de_status="ok"
        fi
        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)
        mark "T642_BOOT_STATE user_de=$user_de_status mirror_de=$mirror_de_status"""
    if old_boot_state not in script_text:
        raise ValueError("old_boot_state anchor not found in t636-diag.sh!")
    script_text = script_text.replace(old_boot_state, new_boot_state, 1)

    return script_text.encode("utf-8")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T661 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # Verify base image
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T661 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T661 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # Clean existing fix images
    for old_img in OUTPUT_DIR.glob("t661-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T661 BUILDER] Removed old fix image: {old_img}")

    patched_diag = False
    print(f"[T661 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

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

            if name_str == "t636-diag.sh":
                print(f"[T661 BUILDER] Found t636-diag.sh ({filesize} bytes), patching for T661...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_script_bytes = patch_diag_script(raw_script.decode("utf-8"))
                new_entry = format_cpio_entry("t636-diag.sh", new_script_bytes, mode=mode, ino=ino)
                fout.write(new_entry)
                patched_diag = True
                print(f"[T661 BUILDER] Patched t636-diag.sh: {filesize} -> {len(new_script_bytes)} bytes.")

            elif name_str == "TRAILER!!!":
                trailer_entry = format_cpio_entry("TRAILER!!!", b"", mode=0, ino=0)
                fout.write(trailer_entry)
                break

            else:
                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))

    if not patched_diag:
        raise RuntimeError("Failed to patch t636-diag.sh: entry not found!")

    # Atomic rename
    PARTIAL_IMAGE.rename(OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_size = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0
    print(f"[T661 BUILDER] Built fix initramfs in {elapsed:.1f}s:")
    print(f"  Path:   {OUTPUT_IMAGE}")
    print(f"  Size:   {fix_size} bytes")
    print(f"  SHA256: {fix_sha}")

    # Write manifest
    manifest = {
        "track": "T661",
        "description": "system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile fix",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": {
            "path": str(BASE_IMAGE),
            "sha256": base_sha,
            "st_nlink": base_stat.st_nlink,
        },
        "kernel": {
            "path": str(kernel_path),
            "sha256": kernel_sha,
        },
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": fix_sha,
            "size_bytes": fix_size,
        },
        "modifications": [
            "t636-diag.sh do_prep: Provisioned /data/user_de/0 (0771 1000:1000) and self-bind-mounted user 0 DE directories to prevent vold.destroyUserStorage rmdir (EBUSY protection)",
            "t636-diag.sh do_prep: Bound /data/user_de/0 onto /data_mirror/data_de/null/0 and /data/user/0 onto /data_mirror/data_ce/null/0",
            "t636-diag.sh do_prep: Pre-created and provisioned DE directories for com.android.permissioncontroller (10097:10097), com.android.systemui (10083:10083), com.android.providers.settings (1000:1000) with self-bind mounts",
            "t636-diag.sh background: Added watch_de_mirrors continuous daemon to maintain mounts and provision DE directories for all apps in /data/data/* and /data/user/0/*",
        ],
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    print(f"[T661 BUILDER] Wrote manifest to {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
