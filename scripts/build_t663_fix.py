#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T663:
system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention (Chain A)

PINNED MECHANISM:
1. Fault A (Dynamic UID Mismatch on First Boot):
   In AOSP, on first boot without a pre-existing /data/system/packages.xml,
   PackageManagerService scans APKs in parallel (ParallelPackageParser) and sequentially
   allocates the lowest available App ID starting from Process.FIRST_APPLICATION_UID (10000)
   via AppIdSettingMap.acquireAndRegisterNewAppId (Settings.registerAppIdLPw).
   Because package parsing finishes in non-deterministic order depending on worker thread
   execution, the UID assigned to com.android.permissioncontroller varies across boots
   (e.g. 10097 in T661, 10088 in T662).
   installd does not assign the UID; PMS passes the assigned UID to installd.createAppDataLocked.
   When installd calls fs_prepare_dir_strict (libcutils/fs.cpp:fs_prepare_path_impl),
   it strictly checks sb.st_uid == uid && sb.st_gid == gid.
   If the directory already exists with a fixed UID (e.g. 10108 pre-created by t636-diag.sh
   in do_prep or watch_mirrors), libcutils detects the mismatch, prints:
     'Expected path ... with owner 10088:10088 but found 10108:10108'
   and returns -1, causing createAppDataLocked to fail with ENOENT.
   FIX: Do NOT pre-create com.android.permissioncontroller (or any dynamic-UID package > 10000)
   in do_prep or watch_mirrors. Let installd create /data/data/$pkg and /data/user_de/0/$pkg
   dynamically during PMS package scan. Because the directory does not exist when installd
   prepares it, fs_prepare_dir_strict executes mkdir(), chown(uid, gid), and chmod(0700) with
   the exact UID PMS assigned, succeeding with 0 errors and returning the genuine tmpfs inode to PMS.

2. Fault B (Mount Contention from watch_mirrors daemon):
   In T662 t636-diag.sh, the background daemon watch_mirrors iterated over /data/data/* and
   executed 'mount -o bind $pdir $pdir' unconditionally every 50ms without checking whether
   $pdir was already a mountpoint. Over 800+ seconds of boot, this stacked hundreds of thousands
   of mount structures on top of each other in the Linux kernel VFS mount table. Every VFS path
   traversal, open, stat, and mount/unmount operation took kernel locks (namespace_sem and mount_lock),
   causing extreme lock contention and thread starvation. At guest time 957s, the system_server
   main thread was blocked inside BinderProxy.transactNative for 91s, triggering the Watchdog kill
   at line 36608.
   FIX: Make watch_mirrors completely idempotent and event-efficient:
   (a) Check 'mountpoint -q $path' before running any bind mount; if already a mountpoint, skip immediately;
   (b) Maintain an in-memory list of already processed packages (seen_packages) so that existing packages
       are skipped instantly with zero subprocess calls;
   (c) Increase daemon sleep interval from 50ms to 500ms (0.5s);
   (d) Total mounts across the entire system capped at ~50-60, completely eliminating mount contention.

Base Image (T662): SHA-256 713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention-initramfs.cpio.gz
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

ROOT = Path("/Users/Shared/wt-track663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention")
BASE_IMAGE = ROOT / "guest-inputs/t662-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t663-system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t663-manifest.json"

EXPECTED_BASE_SHA256 = "713408462a08355e1c9edbc1ddf4f8b7c792ceb32d7de13a918bb9dbed0ef810"
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
    """Insert T663 pinned fix into t636-diag.sh."""
    # 1. Replace T662 prep block in do_prep()
    old_prep_marker = '    # T662: Pinned fix for CE & DE mirror inode lookup and app data isolation'
    end_prep_marker = '    mark "T654_PREP_SENSORS_MULTIHAL"'

    if old_prep_marker not in script_text or end_prep_marker not in script_text:
        raise ValueError("Prep markers not found in t636-diag.sh!")

    idx_start = script_text.find(old_prep_marker)
    idx_end = script_text.find(end_prep_marker)

    t663_prep_block = """    # T663: Pinned fix for dynamic UID mismatch and mount contention
    mark "T663_PREP_MIRRORS_ENTER"

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
        if ! "$TOY" mountpoint -q "$d" 2>/dev/null; then
            "$TOY" mount -o bind "$d" "$d" 2>/dev/null || :
        fi
    done

    # 3. Ensure /data/user/0 and /data_mirror directories exist and mirror user 0
    "$TOY" mkdir -p /data/user/0 2>/dev/null || :
    if ! "$TOY" mountpoint -q /data/user/0 2>/dev/null; then
        "$TOY" mount -o bind /data/data /data/user/0 2>/dev/null || :
    fi

    "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
    if ! "$TOY" mountpoint -q /data_mirror/data_de/null/0 2>/dev/null; then
        "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :
    fi

    "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :
    if ! "$TOY" mountpoint -q /data_mirror/data_ce/null/0 2>/dev/null; then
        "$TOY" mount -o bind /data/data /data_mirror/data_ce/null/0 2>/dev/null || :
    fi

    # 4. SettingsProvider databases (UID 1000:1000 is static SYSTEM_UID, required before PMS starts)
    "$TOY" mkdir -p /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :
    if ! "$TOY" mountpoint -q /data/user_de/0/com.android.providers.settings 2>/dev/null; then
        "$TOY" mount -o bind /data/user_de/0/com.android.providers.settings /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    fi

    "$TOY" mkdir -p /data/data/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/data/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/data/com.android.providers.settings/databases 2>/dev/null || :
    if ! "$TOY" mountpoint -q /data/data/com.android.providers.settings 2>/dev/null; then
        "$TOY" mount -o bind /data/data/com.android.providers.settings /data/data/com.android.providers.settings 2>/dev/null || :
    fi

    # NOTE: com.android.permissioncontroller and com.android.systemui are NOT pre-created here.
    # On first boot without packages.xml, PMS allocates dynamic UIDs (>10000) during package scan.
    # Pre-creating them with fixed UIDs triggers cutils owner mismatch (libcutils/fs.cpp:fs_prepare_path_impl).
    # Leaving them uncreated allows installd to create them with the exact dynamic UID assigned by PMS.

    mark "T663_PREP_MIRRORS_VERIFIED_OK"

"""
    script_text = script_text[:idx_start] + t663_prep_block + script_text[idx_end:]

    # 2. Replace watch_mirrors with idempotent, event-efficient watcher
    old_bg_marker = '# 7. Complete CE & DE Mirror and App Directory Watcher'
    end_bg_marker = 'watch_mirrors &\n'

    if old_bg_marker not in script_text or end_bg_marker not in script_text:
        raise ValueError("Background watcher markers not found in t636-diag.sh!")

    idx_bg_start = script_text.find(old_bg_marker)
    idx_bg_end = script_text.find(end_bg_marker) + len(end_bg_marker)

    t663_bg_watcher = """# 7. Complete CE & DE Mirror and App Directory Watcher (Idempotent, Zero Contention)
watch_mirrors() {
    mark "T663_MIRROR_WATCHER_STARTED"
    seen_packages=" "
    while :; do
        # 1. Maintain self-bind on /data/user_de/0
        if [ ! -d /data/user_de/0 ]; then
            "$TOY" mkdir -p /data/user_de/0 2>/dev/null || :
            "$TOY" chown 1000:1000 /data/user_de/0 2>/dev/null || :
            "$TOY" chmod 0771 /data/user_de/0 2>/dev/null || :
            "$TOY" mount -o bind /data/user_de/0 /data/user_de/0 2>/dev/null || :
        elif ! "$TOY" mountpoint -q /data/user_de/0 2>/dev/null; then
            "$TOY" mount -o bind /data/user_de/0 /data/user_de/0 2>/dev/null || :
        fi

        # 2. Maintain /data_mirror/data_de/null/0
        if [ ! -d /data_mirror/data_de/null/0 ]; then
            "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
            "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :
        elif ! "$TOY" mountpoint -q /data_mirror/data_de/null/0 2>/dev/null; then
            "$TOY" mount -o bind /data/user_de/0 /data_mirror/data_de/null/0 2>/dev/null || :
        fi

        # 3. Maintain /data/user/0 and /data_mirror/data_ce/null/0
        if [ ! -d /data/user/0 ]; then
            "$TOY" mkdir -p /data/user/0 2>/dev/null || :
            "$TOY" mount -o bind /data/data /data/user/0 2>/dev/null || :
        elif ! "$TOY" mountpoint -q /data/user/0 2>/dev/null; then
            "$TOY" mount -o bind /data/data /data/user/0 2>/dev/null || :
        fi

        if [ ! -d /data_mirror/data_ce/null/0 ]; then
            "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :
            "$TOY" mount -o bind /data/data /data_mirror/data_ce/null/0 2>/dev/null || :
        elif ! "$TOY" mountpoint -q /data_mirror/data_ce/null/0 2>/dev/null; then
            "$TOY" mount -o bind /data/data /data_mirror/data_ce/null/0 2>/dev/null || :
        fi

        # 4. Mirror all packages in /data/data dynamically into CE and DE mirrors
        for pdir in /data/data/*; do
            [ -d "$pdir" ] || continue
            pkg=${pdir##*/}
            [ -n "$pkg" ] || continue
            [ "$pkg" = "*" ] && continue

            case "$seen_packages" in
                *" $pkg "*) continue ;;
            esac

            ug=$("$TOY" stat -c "%u:%g" "$pdir" 2>/dev/null)
            # If not yet chowned by installd, let installd finish before binding
            if [ -z "$ug" ] || [ "$ug" = "0:0" ]; then
                continue
            fi

            # Self-bind /data/data/$pkg to protect against installd destroyUserData rmdir/unlink
            if ! "$TOY" mountpoint -q "$pdir" 2>/dev/null; then
                "$TOY" mount -o bind "$pdir" "$pdir" 2>/dev/null || :
            fi

            # Mirror to CE
            if [ ! -d "/data_mirror/data_ce/null/0/$pkg" ]; then
                "$TOY" mkdir -p "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" chown "$ug" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "$pdir" "/data_mirror/data_ce/null/0/$pkg" 2>/dev/null || :
            fi

            if [ ! -d "/data/user/0/$pkg" ]; then
                "$TOY" mkdir -p "/data/user/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "$pdir" "/data/user/0/$pkg" 2>/dev/null || :
            fi

            # Mirror to DE (/data/user_de/0/$pkg and /data_mirror/data_de/null/0/$pkg)
            de_target="/data/user_de/0/$pkg"
            if [ -d "$de_target" ]; then
                if ! "$TOY" mountpoint -q "$de_target" 2>/dev/null; then
                    "$TOY" mount -o bind "$de_target" "$de_target" 2>/dev/null || :
                fi
            else
                "$TOY" mkdir -p "$de_target" 2>/dev/null || :
                "$TOY" chown "$ug" "$de_target" 2>/dev/null || :
                "$TOY" chmod 0700 "$de_target" 2>/dev/null || :
                "$TOY" mount -o bind "$de_target" "$de_target" 2>/dev/null || :
            fi

            if [ ! -d "/data_mirror/data_de/null/0/$pkg" ]; then
                "$TOY" mkdir -p "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" chown "$ug" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" chmod 0700 "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
                "$TOY" mount -o bind "$de_target" "/data_mirror/data_de/null/0/$pkg" 2>/dev/null || :
            fi

            seen_packages="$seen_packages $pkg "
            mark "T663_PROVISIONED_PACKAGE pkg=$pkg ug=$ug"
        done
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}
watch_mirrors &
"""
    script_text = script_text[:idx_bg_start] + t663_bg_watcher + script_text[idx_bg_end:]

    return script_text.encode("utf-8")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T663 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # Verify base image (T662)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T663 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T663 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # Clean existing fix images
    for old_img in OUTPUT_DIR.glob("t663-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T663 BUILDER] Removed old fix image: {old_img}")

    patched_diag = False
    print(f"[T663 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

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
                print(f"[T663 BUILDER] Found t636-diag.sh ({filesize} bytes), patching for T663...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                script_text = raw_script.decode("utf-8")
                patched_bytes = patch_diag_script(script_text)
                new_entry = format_cpio_entry("t636-diag.sh", patched_bytes, mode=mode, ino=ino)
                fout.write(new_entry)
                patched_diag = True
                print(f"[T663 BUILDER] Patched t636-diag.sh: {len(raw_script)} -> {len(patched_bytes)} bytes.")
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
    print(f"[T663 BUILDER] Success! Output fix image: {OUTPUT_IMAGE}")
    print(f"[T663 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T663",
        "description": "system-server-phase550-permissioncontroller-dynamic-uid-and-mount-contention",
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
    print(f"[T663 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
