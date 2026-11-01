#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T662:
system-server-phase550-permissionpolicyservice-permissioncontroller-ce-mirror-inode-lookup (Chain A)

PINNED MECHANISM:
In AOSP App Data Directory Isolation (com_android_internal_os_Zygote.cpp:isolateAppDataPerPackage),
when forking an app process for user 0, Zygote checks getAppDataDirName(mirrorCePath, package_name, ce_data_inode, fail_fn).
mirrorCePath is /data_mirror/data_ce/null/0.
Zygote first calls stat('/data_mirror/data_ce/null/0/<pkg>'). If present, it returns the package name directly.
If absent (ENOENT), it falls back to scanning /data_mirror/data_ce/null/0 via opendir/readdir seeking d_ino == ce_data_inode.
ce_data_inode is recorded by installd (InstalldNativeService::createAppDataLocked) via get_path_inode() on
/data/data/<pkg> (via create_data_user_ce_package_path) during PackageManagerService package scan, and stored in
PackageSetting (PackageUserState.ceDataInode).

In T661 flight-01, /data/data/com.android.permissioncontroller had tmpfs inode 49935 (verified at uart.log lines 12499 and 13514).
However, during PackageManagerService.systemReady(), UserManagerService.reconcileUsers() -> UserDataPreparer
invoked Os.getxattr(path, 'user.serial') on Linux 4.19 tmpfs, returning ENOTSUP (errno 95).
UserDataPreparer treated this as corruption and invoked destroyUserDataLI(..., FLAG_STORAGE_CE) ->
installd.destroyUserData -> delete_dir_contents_and_dir('/data/data', true) (line 16470).
While T661 protected /data/user_de/0 with self-bind mounts (returning EBUSY on unlink/rmdir),
no such protection existed for /data/data or /data_mirror/data_ce/null/0.
When Zygote later forked com.android.systemui and com.android.permissioncontroller at line 17650,
neither package directory existed in /data_mirror/data_ce/null/0, and readdir found no matching inode,
triggering JNI FatalError SIGABRT at com_android_internal_os_Zygote.cpp:1126.

FIX:
1. In do_prep():
   - Ensure /data/data exists (mode 0771 system:system).
   - Pre-create and self-bind mount critical packages in /data/data:
     * com.android.permissioncontroller (UID 10108:10108, mode 0700)
     * com.android.systemui (UID 10083:10083, mode 0700)
     * com.android.providers.settings (UID 1000:1000, mode 0700)
     Self-bind mounts ensure that delete_dir_contents_and_dir() fails with EBUSY during reconcileUsers,
     preserving the directories and their exact tmpfs inodes recorded by PMS.
   - Pre-create /data/user/0 and /data_mirror/data_ce/null/0 and bind-mount /data/data onto both.
   - Bind-mount package directories directly into /data_mirror/data_ce/null/0/<pkg>.
   - Preserve all T661 DE self-bind mounts and mirror setup (with corrected UID 10108:10108 for permissioncontroller).
2. In continuous background daemon (watch_mirrors):
   - Maintain /data/user/0 and /data_mirror/data_ce/null/0 bind mounts to /data/data.
   - Maintain /data/user_de/0 and /data_mirror/data_de/null/0.
   - Maintain self-bind and mirror mounts for high-priority Phase 550 packages in both CE and DE.
   - Continuously scan /data/data/*: for every package created by PMS, apply a self-bind mount
     (preventing deletion) and mirror into /data_mirror/data_ce/null/0/$pkg and /data_mirror/data_de/null/0/$pkg.
   - Sleep 0.05s.

Base Image (T661): SHA-256 2f201b5552d6391958a513f5e9921e9a282a35465ae7387ec793d65572c4dcdc
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t662-system-server-phase550-permissionpolicyservice-permissioncontroller-ce-mirror-inode-lookup-initramfs.cpio.gz
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

ROOT = Path("/Users/Shared/wt-track662-system-server-phase550-permissionpolicyservice-permissioncontroller-ce-mirror-inode-lookup")
BASE_IMAGE = ROOT / "guest-inputs/t661-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t662-system-server-phase550-permissionpolicyservice-permissioncontroller-ce-mirror-inode-lookup-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t662-manifest.json"

EXPECTED_BASE_SHA256 = "2f201b5552d6391958a513f5e9921e9a282a35465ae7387ec793d65572c4dcdc"
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
    """Insert T662 CE & DE prep and continuous mirror watcher into t636-diag.sh."""
    # 1. Replace T661 prep block in do_prep()
    old_prep_marker = '    # T661: Pinned fix for /data_mirror/data_de/null and /data/user_de/0'
    end_prep_marker = '    mark "T654_PREP_SENSORS_MULTIHAL"'

    if old_prep_marker not in script_text or end_prep_marker not in script_text:
        raise ValueError("Prep markers not found in t636-diag.sh!")

    idx_start = script_text.find(old_prep_marker)
    idx_end = script_text.find(end_prep_marker)

    t662_prep_block = """    # T662: Pinned fix for CE & DE mirror inode lookup and app data isolation
    mark "T662_PREP_MIRRORS_ENTER"

    # 1. Ensure /data/user_de/0 and /data/data exist with proper modes
    "$TOY" mkdir -p /data/user_de/0 2>/dev/null || :
    "$TOY" chown 1000:1000 /data/user_de/0 2>/dev/null || :
    "$TOY" chmod 0771 /data/user_de/0 2>/dev/null || :

    "$TOY" mkdir -p /data/data 2>/dev/null || :
    "$TOY" chown 1000:1000 /data/data 2>/dev/null || :
    "$TOY" chmod 0771 /data/data 2>/dev/null || :

    # 2. Self-bind mount user 0 DE directories so vold destroyUserStorage rmdir returns EBUSY
    for d in /data/user_de/0 /data/misc_de/0 /data/system_de/0 /data/vendor_de/0 /data/system/users/0 /data/misc/profiles/cur/0; do
        "$TOY" mkdir -p "$d" 2>/dev/null || :
        "$TOY" mount -o bind "$d" "$d" 2>/dev/null || :
    done

    # 3. Ensure /data/user/0 and /data_mirror directories exist and mirror user 0
    "$TOY" mkdir -p /data/user/0 2>/dev/null || :
    "$TOY" mount -o bind /data/data /data/user/0 2>/dev/null || :

    "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :

    "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :
    "$TOY" mount -o bind /data/data /data_mirror/data_ce/null/0 2>/dev/null || :

    # 4. Pre-create, provision, and self-bind mount critical Phase 550 app directories in BOTH CE and DE:
    # (a) com.android.permissioncontroller (UID 10108, GID 10108)
    # DE:
    "$TOY" mkdir -p /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10108:10108 /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.permissioncontroller /data/user_de/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10108:10108 /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.permissioncontroller /data_mirror/data_de/null/0/com.android.permissioncontroller 2>/dev/null || :
    # CE:
    "$TOY" mkdir -p /data/data/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10108:10108 /data/data/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.permissioncontroller /data/data/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mkdir -p /data/user/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.permissioncontroller /data/user/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_ce/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chown -R 10108:10108 /data_mirror/data_ce/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_ce/null/0/com.android.permissioncontroller 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.permissioncontroller /data_mirror/data_ce/null/0/com.android.permissioncontroller 2>/dev/null || :

    # (b) com.android.systemui (UID 10083, GID 10083)
    # DE:
    "$TOY" mkdir -p /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.systemui /data/user_de/0/com.android.systemui 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/user_de/0/com.android.systemui /data_mirror/data_de/null/0/com.android.systemui 2>/dev/null || :
    # CE:
    "$TOY" mkdir -p /data/data/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data/data/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.systemui /data/data/com.android.systemui 2>/dev/null || :
    "$TOY" mkdir -p /data/user/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.systemui /data/user/0/com.android.systemui 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_ce/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chown -R 10083:10083 /data_mirror/data_ce/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_ce/null/0/com.android.systemui 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.systemui /data_mirror/data_ce/null/0/com.android.systemui 2>/dev/null || :

    # (c) com.android.providers.settings (UID 1000, GID 1000)
    # DE:
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
    # CE:
    "$TOY" mkdir -p /data/data/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/data/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.providers.settings /data/data/com.android.providers.settings 2>/dev/null || :
    "$TOY" mkdir -p /data/user/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.providers.settings /data/user/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" mkdir -p /data_mirror/data_ce/null/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data_mirror/data_ce/null/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_ce/null/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data_mirror/data_ce/null/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" mount -o bind /data/data/com.android.providers.settings /data_mirror/data_ce/null/0/com.android.providers.settings 2>/dev/null || :

    if [ -d /data_mirror/data_de/null/0/com.android.permissioncontroller ] && [ -d /data_mirror/data_ce/null/0/com.android.permissioncontroller ]; then
        mark "T662_PREP_MIRRORS_VERIFIED_OK"
    else
        mark "T662_PREP_MIRRORS_VERIFIED_FAILED"
    fi

"""
    script_text = script_text[:idx_start] + t662_prep_block + script_text[idx_end:]

    # 2. Replace watch_de_mirrors with enhanced watch_mirrors daemon
    old_bg_marker = '# 7. DE Mirror & App Directory Watcher'
    end_bg_marker = 'watch_de_mirrors &\n'

    if old_bg_marker not in script_text or end_bg_marker not in script_text:
        raise ValueError("Background watcher markers not found in t636-diag.sh!")

    idx_bg_start = script_text.find(old_bg_marker)
    idx_bg_end = script_text.find(end_bg_marker) + len(end_bg_marker)

    t662_bg_watcher = """# 7. Complete CE & DE Mirror and App Directory Watcher
watch_mirrors() {
    mark "T662_MIRROR_WATCHER_STARTED"
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

        # 3. Maintain /data/user/0 and /data_mirror/data_ce/null/0
        if [ ! -d /data/user/0 ]; then
            "$TOY" mkdir -p /data/user/0 2>/dev/null || :
            "$TOY" mount -o bind /data/data /data/user/0 2>/dev/null || :
        fi
        if [ ! -d /data_mirror/data_ce/null/0 ]; then
            "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :
            "$TOY" mount -o bind /data/data /data_mirror/data_ce/null/0 2>/dev/null || :
        fi

        # 4. Critical Phase 550 packages: check, self-bind, and mirror in CE and DE immediately
        for spec in "com.android.permissioncontroller:10108:10108" "com.android.systemui:10083:10083" "com.android.providers.settings:1000:1000"; do
            pkg=${spec%%:*}
            ug=${spec#*:}
            uid=${ug%%:*}
            gid=${ug#*:}

            # CE (data/data and mirror_ce)
            if [ ! -d "/data/data/$pkg" ]; then
                "$TOY" mkdir -p "/data/data/$pkg" 2>/dev/null || :
                "$TOY" chown "$uid:$gid" "/data/data/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data/data/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "/data/data/$pkg" "/data/data/$pkg" 2>/dev/null || :
            fi
            if [ ! -d "/data/user/0/$pkg" ]; then
                "$TOY" mkdir -p "/data/user/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "/data/data/$pkg" "/data/user/0/$pkg" 2>/dev/null || :
            fi
            if [ ! -d "/data_mirror/data_ce/null/0/$pkg" ]; then
                "$TOY" mkdir -p "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" chown "$uid:$gid" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "/data/data/$pkg" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
            fi

            # DE (user_de/0 and mirror_de)
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

        # 5. Mirror all packages in /data/data dynamically into CE and DE mirrors
        for pdir in /data/data/*; do
            [ -d "$pdir" ] || continue
            pkg=${pdir##*/}
            [ -n "$pkg" ] || continue
            ug=$("$TOY" stat -c "%u:%g" "$pdir" 2>/dev/null)

            # Self-bind /data/data/$pkg to protect against installd destroyUserData rmdir/unlink
            "$TOY" mount -o bind "$pdir" "$pdir" 2>/dev/null || :

            # Mirror to CE
            if [ ! -d "/data_mirror/data_ce/null/0/$pkg" ]; then
                "$TOY" mkdir -p "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                if [ -n "$ug" ]; then
                    "$TOY" chown "$ug" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                fi
                "$TOY" chmod 0700 "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "$pdir" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                mark "T662_PROVISIONED_CE_DIR pkg=$pkg ug=$ug"
            fi
            if [ ! -d "/data/user/0/$pkg" ]; then
                "$TOY" mkdir -p "/data/user/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "$pdir" "/data/user/0/$pkg" 2>/dev/null || :
            fi

            # Mirror to DE
            de_target="/data/user_de/0/$pkg"
            if [ ! -d "$de_target" ]; then
                "$TOY" mkdir -p "$de_target" 2>/dev/null || :
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
watch_mirrors &
"""
    script_text = script_text[:idx_bg_start] + t662_bg_watcher + script_text[idx_bg_end:]

    # 3. Update watch_boot_state markers
    old_boot_marker = 'mark "T642_BOOT_STATE user_de=$user_de_status mirror_de=$mirror_de_status'
    new_boot_marker = """        mirror_ce_status="missing"
        if [ -d /data_mirror/data_ce/null/0/com.android.permissioncontroller ]; then
            mirror_ce_status="ok"
        fi
        mark "T642_BOOT_STATE user_de=$user_de_status mirror_de=$mirror_de_status mirror_ce=$mirror_ce_status"""
    if old_boot_marker not in script_text:
        raise ValueError("old_boot_marker anchor not found in t636-diag.sh!")
    script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    return script_text.encode("utf-8")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T662 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # Verify base image (T661)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T662 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T662 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # Clean existing fix images
    for old_img in OUTPUT_DIR.glob("t662-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T662 BUILDER] Removed old fix image: {old_img}")

    patched_diag = False
    print(f"[T662 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

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
                print(f"[T662 BUILDER] Found t636-diag.sh ({filesize} bytes), patching for T662...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                script_text = raw_script.decode("utf-8")
                patched_bytes = patch_diag_script(script_text)
                new_entry = format_cpio_entry("t636-diag.sh", patched_bytes, mode=mode, ino=ino)
                fout.write(new_entry)
                patched_diag = True
                print(f"[T662 BUILDER] Patched t636-diag.sh: {len(raw_script)} -> {len(patched_bytes)} bytes.")
            else:
                # Stream unchanged entry
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                # Copy data in chunks
                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError("Unexpected EOF reading CPIO data!")
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

    if not patched_diag:
        raise RuntimeError("t636-diag.sh was not found or patched in CPIO stream!")

    # Atomic move to final image
    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_bytes = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0
    print(f"[T662 BUILDER] Success! Output fix image: {OUTPUT_IMAGE}")
    print(f"[T662 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T662",
        "description": "system-server-phase550-permissionpolicyservice-permissioncontroller-ce-mirror-inode-lookup",
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
    print(f"[T662 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
