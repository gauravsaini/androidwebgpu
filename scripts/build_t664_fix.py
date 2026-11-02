#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T664:
system-server-startbootstrapservices-installer-invalidatemounts-watchdog-96s (Chain A)

PINNED MECHANISM:
Dominant Cause: Diagnostic background loop CPU starvation and process/audit storm under QEMU emulation.
In T663, t636-diag.sh ran multiple unthrottled background loops that continuously spawned subprocesses
(toybox basename, tr, cat, pidof, mountpoint, mount, getprop):
1. watch_zygote_environ: scanned /proc/[0-9]* every 1s spawning toybox basename and tr for every process,
   triggering SELinux audit denials (audit_lost=51) and flooding the UART serial console via logcat.
2. watch_system_server: polled every 200ms spawning getprop and toybox pidof.
3. watch_mirrors: executed mountpoint -q every 500ms on tmpfs directories, which always fails in toybox
   because st_dev of a tmpfs child matches its parent, causing repeated mount -o bind executions.
Under QEMU TCG emulation, this continuous fork/exec storm and audit logging starved installd (PID 2427)
from reading /proc/mounts and returning the Binder transaction when system_server called
Installer.connect() -> invalidateMounts(). After 96s, Watchdog killed system_server (PID 7917).

PINNED FIX:
1. watch_zygote_environ: Read Zygote PID directly from init property init.svc_debug_pid.zygote / zygote64.
   Once Zygote BOOTCLASSPATH is captured and marked (T642_ZYGOTE_ENVIRON_END), exit immediately (return 0).
   Eliminates all /proc scanning and audit spam during system_server startup.
2. watch_system_server: When active_ss_pid is alive, check /proc/$active_ss_pid directly without invoking pidof;
   increase polling sleep from 0.2s to 2.0s.
3. watch_mirrors: Root mirror bind-mounts are already established in do_prep(); do not attempt repeated
   bind-mounts in the watcher loop. Increase sleep from 0.5s to 1.0s.
4. Keep all T663 fixes intact: no pre-creation of dynamic UID packages (>10000), preserve CE/DE mirror mounts,
   0 .capex across lineage.

Base Image (T663): SHA-256 6b38578c7966f722b795cbe10bc5fa60f3e9b6443cf3104ed367f0d2be5c0ff1
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t664-system-server-startbootstrapservices-installer-invalidatemounts-watchdog-96s-initramfs.cpio.gz
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

ROOT = Path("/Users/Shared/wt-track664-system-server-startbootstrapservices-installer-invalidatemounts-watchdog-96s")
BASE_IMAGE = ROOT / "guest-inputs/t663-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t664-system-server-startbootstrapservices-installer-invalidatemounts-watchdog-96s-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t664-manifest.json"

EXPECTED_BASE_SHA256 = "6b38578c7966f722b795cbe10bc5fa60f3e9b6443cf3104ed367f0d2be5c0ff1"
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
    """Insert T664 pinned fix into t636-diag.sh."""
    # 1. Replace watch_zygote_environ with single-shot, zero-contention watcher
    old_zy_marker = '# 4. Watcher for Zygote PID and /proc/<pid>/environ BOOTCLASSPATH verbatim'
    end_zy_marker = 'watch_zygote_environ &\n'

    if old_zy_marker not in script_text or end_zy_marker not in script_text:
        raise ValueError("Zygote watcher markers not found in t636-diag.sh!")

    idx_zy_start = script_text.find(old_zy_marker)
    idx_zy_end = script_text.find(end_zy_marker) + len(end_zy_marker)

    t664_zy_watcher = """# 4. Watcher for Zygote PID and /proc/<pid>/environ BOOTCLASSPATH verbatim (Single-shot, Zero Contention)
watch_zygote_environ() {
    mark T642_ZYGOTE_WATCHER_STARTED
    while :; do
        for zprop in init.svc_debug_pid.zygote init.svc_debug_pid.zygote64; do
            zypid=$(/system/bin/getprop "$zprop" 2>/dev/null)
            if [ -n "$zypid" ] && [ -d "/proc/$zypid" ]; then
                mark "T642_ZYGOTE_PROP_FOUND pid=$zypid prop=$zprop"
                if [ -r "/proc/$zypid/environ" ]; then
                    mark "T642_ZYGOTE_ENVIRON_BEGIN pid=$zypid"
                    raw_env=$("$TOY" tr '\\000' ' ' < "/proc/$zypid/environ" 2>/dev/null)
                    mark "T642_ZYGOTE_ENVIRON_RAW pid=$zypid $raw_env"
                    bcp=$("$TOY" tr '\\000' '\\n' < "/proc/$zypid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                    if [ -n "$bcp" ]; then
                        mark "T642_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$zypid $bcp"
                        printf '%s\\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\\n' | while IFS= read -r item; do
                            [ -n "$item" ] && mark "T642_ZYGOTE_BCP_ITEM pid=$zypid item=$item"
                        done
                    else
                        mark "T642_ZYGOTE_BOOTCLASSPATH_MISSING pid=$zypid"
                    fi
                    mark "T642_ZYGOTE_ENVIRON_END pid=$zypid"
                    mark "T664_ZYGOTE_WATCHER_COMPLETE"
                    return 0
                fi
            fi
        done
        "$TOY" sleep 1 2>/dev/null || :
    done
}
watch_zygote_environ &
"""
    script_text = script_text[:idx_zy_start] + t664_zy_watcher + script_text[idx_zy_end:]

    # 2. Replace watch_system_server with throttled, efficient watcher
    old_ss_marker = '# 5. Watcher for system_server'
    end_ss_marker = 'watch_system_server &\n'

    if old_ss_marker not in script_text or end_ss_marker not in script_text:
        raise ValueError("System server watcher markers not found in t636-diag.sh!")

    idx_ss_start = script_text.find(old_ss_marker)
    idx_ss_end = script_text.find(end_ss_marker) + len(end_ss_marker)

    t664_ss_watcher = """# 5. Watcher for system_server (Throttled, Zero Contention)
watch_system_server() {
    cdev=/dev/console
    [ -c "$cdev" ] || cdev=/dev/ttyAMA0
    mark T642_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss_pids=""
    active_ss_pid=""
    while :; do
        if [ -n "$active_ss_pid" ] && [ -d "/proc/$active_ss_pid" ]; then
            "$TOY" sleep 2 2>/dev/null || :
            continue
        fi

        ss_prop=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        if [ -n "$sspids" ]; then
            for sspid in $sspids; do
                case "$seen_ss_pids" in
                    *" $sspid "*) ;;
                    *)
                        seen_ss_pids="$seen_ss_pids $sspid "
                        active_ss_pid="$sspid"
                        cmd=$("$TOY" tr '\\000' ' ' < "/proc/$sspid/cmdline" 2>/dev/null)
                        comm=$("$TOY" cat "/proc/$sspid/comm" 2>/dev/null)
                        mark "T642_SYSTEM_SERVER_STARTED pid=$sspid comm=$comm cmd=$cmd prop=$ss_prop"
                        if [ -r "/proc/$sspid/environ" ]; then
                            bcp=$("$TOY" tr '\\000' '\\n' < "/proc/$sspid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T642_SYSTEM_SERVER_BOOTCLASSPATH pid=$sspid $bcp"
                        fi
                        if [ -r "/proc/$sspid/status" ]; then
                            st=$("$TOY" head -n 5 "/proc/$sspid/status" 2>/dev/null | "$TOY" tr '\\n' ';')
                            mark "T642_SYSTEM_SERVER_STATUS pid=$sspid $st"
                        fi
                        ;;
                esac
            done
        elif [ -n "$active_ss_pid" ]; then
            mark "T646_SYSTEM_SERVER_EXITED pid=$active_ss_pid"
            active_ss_pid=""
            mark "T646_DUMP_CRASH_LOGCAT_BEGIN"
            /system/bin/logcat -b crash -d -v threadtime >> "$cdev" 2>/dev/null || :
            mark "T646_DUMP_CRASH_LOGCAT_END"
            mark "T646_DUMP_SYSTEM_LOGCAT_BEGIN"
            /system/bin/logcat -b system -d -v threadtime -t 100 >> "$cdev" 2>/dev/null || :
            mark "T646_DUMP_SYSTEM_LOGCAT_END"
            if [ -d /data/system/dropbox ]; then
                for dbf in /data/system/dropbox/*; do
                    [ -f "$dbf" ] || continue
                    mark "T646_DROPBOX_FILE=$dbf"
                    "$TOY" head -n 50 "$dbf" >> "$cdev" 2>/dev/null || :
                done
            fi
        elif [ -n "$ss_prop" ]; then
            mark "T642_SYSTEM_SERVER_PROP_ONLY prop=$ss_prop"
        fi
        "$TOY" sleep 2 2>/dev/null || :
    done
}
watch_system_server &
"""
    script_text = script_text[:idx_ss_start] + t664_ss_watcher + script_text[idx_ss_end:]

    # 3. Replace watch_mirrors with throttled, non-re-mounting watcher
    old_bg_marker = '# 7. Complete CE & DE Mirror and App Directory Watcher'
    end_bg_marker = 'watch_mirrors &\n'

    if old_bg_marker not in script_text or end_bg_marker not in script_text:
        raise ValueError("Background watcher markers not found in t636-diag.sh!")

    idx_bg_start = script_text.find(old_bg_marker)
    idx_bg_end = script_text.find(end_bg_marker) + len(end_bg_marker)

    t664_bg_watcher = """# 7. Complete CE & DE Mirror and App Directory Watcher (Idempotent, Zero Contention)
watch_mirrors() {
    mark "T663_MIRROR_WATCHER_STARTED"
    seen_packages=" "
    while :; do
        # 1. Ensure basic mirror targets exist if somehow missing
        [ -d /data/user_de/0 ] || "$TOY" mkdir -p /data/user_de/0 2>/dev/null || :
        [ -d /data_mirror/data_de/null/0 ] || "$TOY" mkdir -p /data_mirror/data_de/null/0 2>/dev/null || :
        [ -d /data/user/0 ] || "$TOY" mkdir -p /data/user/0 2>/dev/null || :
        [ -d /data_mirror/data_ce/null/0 ] || "$TOY" mkdir -p /data_mirror/data_ce/null/0 2>/dev/null || :

        # 2. Mirror newly created packages in /data/data dynamically into CE and DE mirrors
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
        "$TOY" sleep 1.0 2>/dev/null || :
    done
}
watch_mirrors &
"""
    script_text = script_text[:idx_bg_start] + t664_bg_watcher + script_text[idx_bg_end:]

    return script_text.encode("utf-8")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T664 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # Verify base image (T663)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T664 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T664 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # Clean existing fix images
    for old_img in OUTPUT_DIR.glob("t664-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T664 BUILDER] Removed old fix image: {old_img}")

    patched_diag = False
    print(f"[T664 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

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
                print(f"[T664 BUILDER] Found t636-diag.sh ({filesize} bytes), patching for T664...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                script_text = raw_script.decode("utf-8")
                patched_bytes = patch_diag_script(script_text)
                new_entry = format_cpio_entry("t636-diag.sh", patched_bytes, mode=mode, ino=ino)
                fout.write(new_entry)
                patched_diag = True
                print(f"[T664 BUILDER] Patched t636-diag.sh: {len(raw_script)} -> {len(patched_bytes)} bytes.")
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
    print(f"[T664 BUILDER] Success! Output fix image: {OUTPUT_IMAGE}")
    print(f"[T664 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T664",
        "description": "system-server-startbootstrapservices-installer-invalidatemounts-watchdog-96s",
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
    print(f"[T664 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
