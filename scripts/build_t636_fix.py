#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T636: append framework-connectivity.jar to BOOTCLASSPATH."""
from __future__ import annotations

import gzip
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track636-bootclasspath-fix")
BASE_IMAGE = ROOT / "guest-inputs/t628-iallocator-service-initramfs.cpio.gz"
OUTPUT_IMAGE = ROOT / "guest-image/t636-bootclasspath-fixed-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = ROOT / "guest-image/t636-manifest.json"

EXPECTED_BASE_SHA256 = "6b0b0d715b92e97b9a997c60f7c5e06cff81d7178af92d1ed28fc55a95414e17"
SYSTEM_IMG_SIZE = 2059816960
INIT_INODE_SIZE_OFFSET = 134292740
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096
ORIG_INITRC_SIZE = 58194

BOOTCLASSPATH_20 = (
    "/apex/com.android.art/javalib/core-oj.jar:"
    "/apex/com.android.art/javalib/core-libart.jar:"
    "/apex/com.android.art/javalib/okhttp.jar:"
    "/apex/com.android.art/javalib/bouncycastle.jar:"
    "/apex/com.android.art/javalib/apache-xml.jar:"
    "/system/framework/framework.jar:"
    "/system/framework/framework-graphics.jar:"
    "/system/framework/ext.jar:"
    "/system/framework/telephony-common.jar:"
    "/system/framework/voip-common.jar:"
    "/system/framework/ims-common.jar:"
    "/apex/com.android.i18n/javalib/core-icu4j.jar:"
    "/apex/com.android.btservices/javalib/framework-bluetooth.jar:"
    "/apex/com.android.conscrypt/javalib/conscrypt.jar:"
    "/apex/com.android.devicelock/javalib/framework-devicelock.jar:"
    "/apex/com.android.healthfitness/javalib/framework-healthfitness.jar:"
    "/apex/com.android.os.statsd/javalib/framework-statsd.jar:"
    "/apex/com.android.sdkext/javalib/framework-sdkextensions.jar:"
    "/apex/com.android.virt/javalib/framework-virtualization.jar:"
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar"
)

BCP_OVERRIDE_BLOCK = (
    b"    exec - root root -- /system/bin/sh -c \"/system/bin/toybox sed -i 's|export BOOTCLASSPATH .*|&:/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar|' /data/system/environ/classpath\"\n"
    b"    load_exports /data/system/environ/classpath\n"
    b"    export BOOTCLASSPATH " + BOOTCLASSPATH_20.encode("ascii") + b"\n"
    b"    write /dev/kmsg T636_BOOTCLASSPATH_FIX_APPLIED\n"
    b"    exec_background u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh\n"
)

LOAD_EXPORTS_ANCHOR = b"    load_exports /data/system/environ/classpath\n"

EARLY_RAMDISK_ANCHOR = b'"$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0771 tmpfs /data\n'
EARLY_RAMDISK_INSERT = (
    b'"$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0771 tmpfs /data\n'
    b'if [ -f /t636-diag.sh ]; then\n'
    b'    "$TOYBOX" cp /t636-diag.sh /data/t636-diag.sh\n'
    b'    "$TOYBOX" chmod 0755 /data/t636-diag.sh\n'
    b'    echo "[T636] copied /t636-diag.sh to /data/t636-diag.sh"\n'
    b'fi\n'
)

DIAG_SCRIPT_CONTENT = b"""#!/system/bin/sh
KMSG=/dev/kmsg
TOY=/system/bin/toybox

# Ensure device nodes exist
if [ ! -c /dev/null ]; then
    /system/bin/toybox mknod -m 0666 /dev/null c 1 3 2>/dev/null || true
fi
if [ ! -c /dev/kmsg ]; then
    /system/bin/toybox mknod -m 0666 /dev/kmsg c 1 11 2>/dev/null || true
fi

# Wait for toybox to be available
while [ ! -x "$TOY" ]; do
    sleep 0.5 2>/dev/null || :
done

# Prevent multiple instances
if [ -f /data/t636-diag.pid ]; then
    oldpid=$("$TOY" cat /data/t636-diag.pid 2>/dev/null)
    if [ -n "$oldpid" ] && [ -d "/proc/$oldpid" ]; then
        exit 0
    fi
fi
echo $$ > /data/t636-diag.pid 2>/dev/null || :

mark() {
    if [ -c "$KMSG" ]; then
        echo "$*" > "$KMSG" 2>/dev/null || :
    fi
    if [ -c /dev/console ]; then
        echo "$*" > /dev/console 2>/dev/null || :
    fi
}

mark "T636_DIAGNOSTICS_STARTED version=2 pid=$$"

# 1. Start full logcat streaming to kmsg with restart loop
start_logcat() {
    while [ ! -x /system/bin/logcat ]; do
        "$TOY" sleep 0.5 2>/dev/null || :
    done
    mark T636_LOGCAT_STREAMER_STARTED
    while :; do
        /system/bin/logcat -b all -v threadtime >> "$KMSG" 2>&1
        "$TOY" sleep 1 2>/dev/null || :
    done
}
start_logcat &

# 2. Check flattened tethering jar on disk
if [ -f /system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar ]; then
    mark T636_FLATTENED_TETHERING_JAR_EXISTS
    "$TOY" ls -la /system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar > "$KMSG" 2>&1
else
    mark T636_FLATTENED_TETHERING_JAR_MISSING
fi

# 3. Watch /data/system/environ/classpath and ensure framework-connectivity.jar is present
watch_classpath_source() {
    updated=0
    while [ $updated -eq 0 ]; do
        if [ -f /data/system/environ/classpath ]; then
            mark T636_SOURCE_BOOTCLASSPATH_BEGIN
            "$TOY" grep '^export BOOTCLASSPATH ' /data/system/environ/classpath | "$TOY" sed 's/^export BOOTCLASSPATH //' | "$TOY" tr ':' '\\n' | while IFS= read -r item; do
                [ -n "$item" ] && mark "T636_SOURCE_BCP_ITEM=$item"
            done
            mark T636_SOURCE_BOOTCLASSPATH_END
            if ! "$TOY" grep -q "framework-connectivity.jar" /data/system/environ/classpath; then
                "$TOY" sed -i 's|export BOOTCLASSPATH .*|&:/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar|' /data/system/environ/classpath
                mark T636_DATA_SYSTEM_ENVIRON_CLASSPATH_UPDATED
                mark T636_UPDATED_BOOTCLASSPATH_BEGIN
                "$TOY" grep '^export BOOTCLASSPATH ' /data/system/environ/classpath | "$TOY" sed 's/^export BOOTCLASSPATH //' | "$TOY" tr ':' '\\n' | while IFS= read -r item; do
                    [ -n "$item" ] && mark "T636_UPDATED_BCP_ITEM=$item"
                done
                mark T636_UPDATED_BOOTCLASSPATH_END
            fi
            updated=1
        fi
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}
watch_classpath_source &

# 4. Watcher for Zygote PID and /proc/<pid>/environ BOOTCLASSPATH verbatim
watch_zygote_environ() {
    mark T636_ZYGOTE_WATCHER_STARTED
    seen_pids=""
    while :; do
        for proc in /proc/[0-9]*; do
            [ -d "$proc" ] || continue
            pid=$("$TOY" basename "$proc" 2>/dev/null)
            [ "$pid" = "$$" ] && continue
            case "$seen_pids" in
                *" $pid "*) continue ;;
            esac
            [ -r "$proc/cmdline" ] || continue
            cmd=$("$TOY" tr '\\000' ' ' < "$proc/cmdline" 2>/dev/null)
            case "$cmd" in
                *logcat*|*toybox*|*grep*|*sleep*|*t636*) continue ;;
            esac
            comm=$("$TOY" cat "$proc/comm" 2>/dev/null)
            is_zygote=0
            if [ "$comm" = "zygote64" ] || [ "$comm" = "zygote" ] || [ "$comm" = "main" ] || [ "$comm" = "app_process64" ]; then
                is_zygote=1
            elif echo "$cmd" | "$TOY" grep -q "app_process.*zygote"; then
                is_zygote=1
            fi
            if [ $is_zygote -eq 1 ]; then
                seen_pids="$seen_pids $pid "
                mark "T636_ZYGOTE_PROCESS_FOUND pid=$pid comm=$comm cmd=$cmd"
                if [ -r "$proc/environ" ]; then
                    mark "T636_ZYGOTE_ENVIRON_BEGIN pid=$pid"
                    raw_env=$("$TOY" tr '\\000' ' ' < "$proc/environ" 2>/dev/null)
                    mark "T636_ZYGOTE_ENVIRON_RAW pid=$pid $raw_env"
                    bcp=$("$TOY" tr '\\000' '\\n' < "$proc/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                    if [ -n "$bcp" ]; then
                        mark "T636_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$pid $bcp"
                        printf '%s\\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\\n' | while IFS= read -r item; do
                            [ -n "$item" ] && mark "T636_ZYGOTE_BCP_ITEM pid=$pid item=$item"
                        done
                    else
                        mark "T636_ZYGOTE_BOOTCLASSPATH_MISSING pid=$pid"
                    fi
                    mark "T636_ZYGOTE_ENVIRON_END pid=$pid"
                else
                    mark "T636_ZYGOTE_ENVIRON_UNREADABLE pid=$pid"
                fi
            fi
        done
        for zprop in init.svc_debug_pid.zygote init.svc_debug_pid.zygote64; do
            zypid=$(/system/bin/getprop "$zprop" 2>/dev/null)
            if [ -n "$zypid" ] && [ -d "/proc/$zypid" ]; then
                case "$seen_pids" in
                    *" $zypid "*) ;;
                    *)
                        seen_pids="$seen_pids $zypid "
                        mark "T636_ZYGOTE_PROP_FOUND pid=$zypid prop=$zprop"
                        if [ -r "/proc/$zypid/environ" ]; then
                            mark "T636_ZYGOTE_ENVIRON_BEGIN pid=$zypid"
                            bcp=$("$TOY" tr '\\000' '\\n' < "/proc/$zypid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T636_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=$zypid $bcp"
                            printf '%s\\n' "$bcp" | "$TOY" sed 's/^BOOTCLASSPATH=//' | "$TOY" tr ':' '\\n' | while IFS= read -r item; do
                                [ -n "$item" ] && mark "T636_ZYGOTE_BCP_ITEM pid=$zypid item=$item"
                            done
                            mark "T636_ZYGOTE_ENVIRON_END pid=$zypid"
                        fi
                        ;;
                esac
            fi
        done
        "$TOY" sleep 1 2>/dev/null || :
    done
}
watch_zygote_environ &

# 5. Watcher for system_server
watch_system_server() {
    mark T636_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss=0
    while :; do
        ss_prop=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        if [ -n "$sspids" ] || [ "$ss_prop" = "running" ]; then
            if [ $seen_ss -eq 0 ]; then
                seen_ss=1
                mark "T636_SYSTEM_SERVER_STARTED pid=$sspids prop=$ss_prop"
                if [ -n "$sspids" ]; then
                    for sspid in $sspids; do
                        if [ -r "/proc/$sspid/environ" ]; then
                            bcp=$("$TOY" tr '\\000' '\\n' < "/proc/$sspid/environ" 2>/dev/null | "$TOY" grep '^BOOTCLASSPATH=')
                            mark "T636_SYSTEM_SERVER_BOOTCLASSPATH pid=$sspid $bcp"
                        fi
                    done
                fi
            fi
        fi
        "$TOY" sleep 1 2>/dev/null || :
    done
}
watch_system_server &

# 6. Boot state watcher
watch_boot_state() {
    mark T636_BOOT_STATE_WATCHER_STARTED
    while :; do
        uptime=$("$TOY" cut -d ' ' -f 1 /proc/uptime 2>/dev/null)
        ss=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        zy=$(/system/bin/getprop init.svc.zygote 2>/dev/null)
        [ -z "$zy" ] && zy=$(/system/bin/getprop init.svc.zygote64 2>/dev/null)
        zypid=$(/system/bin/getprop init.svc_debug_pid.zygote 2>/dev/null)
        [ -z "$zypid" ] && zypid=$(/system/bin/getprop init.svc_debug_pid.zygote64 2>/dev/null)
        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)
        mark "T636_BOOT_STATE uptime=$uptime system_server=$ss zygote=$zy zypid=$zypid boot_completed=$completed"
        "$TOY" sleep 10 2>/dev/null || :
    done
}
watch_boot_state &

while :; do "$TOY" sleep 60 2>/dev/null || :; done
"""


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def pad4(size: int) -> int:
    return (-size) % 4


def make_cpio_entry(name: str, content: bytes, mode: int = 0o100755, ino: int = 7636) -> bytes:
    name_bytes = name.encode("ascii") + b"\0"
    namesize = len(name_bytes)
    filesize = len(content)
    pad_name = pad4(110 + namesize)
    pad_file = pad4(filesize)
    hdr = (
        f"070701{ino:08x}{mode:08x}00000000000000000000000100000000"
        f"{filesize:08x}00000000000000000000000000000000{namesize:08x}00000000"
    ).encode("ascii")
    assert len(hdr) == 110
    return hdr + name_bytes + (b"\0" * pad_name) + content + (b"\0" * pad_file)


def patch_cpio_initrc(original: bytes) -> bytes:
    old_bcp_line = (
        b"export BOOTCLASSPATH /apex/com.android.art/javalib/core-oj.jar:"
        b"/apex/com.android.art/javalib/core-libart.jar:"
        b"/apex/com.android.art/javalib/okhttp.jar:"
        b"/apex/com.android.art/javalib/bouncycastle.jar:"
        b"/apex/com.android.art/javalib/apache-xml.jar:"
        b"/system/framework/framework.jar:"
        b"/system/framework/framework-graphics.jar:"
        b"/system/framework/ext.jar:"
        b"/system/framework/telephony-common.jar:"
        b"/system/framework/voip-common.jar:"
        b"/system/framework/ims-common.jar:"
        b"/apex/com.android.i18n/javalib/core-icu4j.jar"
    )
    new_bcp_line = b"export BOOTCLASSPATH " + BOOTCLASSPATH_20.encode("ascii")
    if old_bcp_line not in original:
        raise ValueError("old BOOTCLASSPATH line not found in CPIO init.rc")
    return original.replace(old_bcp_line, new_bcp_line, 1)


def build():
    print(f"[T636 BUILDER] Verifying base image {BASE_IMAGE}...")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T636 BUILDER] Base image SHA-256 OK: {base_sha}")

    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    OUTPUT_IMAGE.parent.mkdir(parents=True, exist_ok=True)

    print(f"[T636 BUILDER] Streaming build into {OUTPUT_IMAGE}...")
    t0 = time.monotonic()

    orig_system_sha = hashlib.sha256()
    new_system_sha = hashlib.sha256()

    new_inode_size = ORIG_INITRC_SIZE + len(BCP_OVERRIDE_BLOCK) - len(LOAD_EXPORTS_ANCHOR)
    if new_inode_size > INIT_ALLOCATED_SIZE:
        raise ValueError(f"New init.rc size {new_inode_size} exceeds allocated extent {INIT_ALLOCATED_SIZE}")

    diag_cpio_entry = make_cpio_entry("t636-diag.sh", DIAG_SCRIPT_CONTENT, mode=0o100755, ino=7636)

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if not (hdr.startswith(b"070701") or hdr.startswith(b"070702")):
                fout.write(hdr)
                break
            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1", errors="replace")
            pad_name = pad4(110 + namesize)
            fin.read(pad_name)

            if name_str == "early-ramdisk":
                orig_content = fin.read(filesize)
                fin.read(pad4(filesize))
                if EARLY_RAMDISK_ANCHOR not in orig_content:
                    raise ValueError("EARLY_RAMDISK_ANCHOR not found in early-ramdisk")
                new_content = orig_content.replace(EARLY_RAMDISK_ANCHOR, EARLY_RAMDISK_INSERT, 1)
                new_filesize = len(new_content)
                new_hdr = hdr[:54] + f"{new_filesize:08x}".encode("ascii") + hdr[62:]
                fout.write(new_hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                fout.write(new_content)
                fout.write(b"\0" * pad4(new_filesize))
                print(f"[T636 BUILDER] Patched {name_str}: {filesize} -> {new_filesize} bytes")

            elif name_str in ("system/etc/init/hw/init.rc", "system/etc/init/init.rc"):
                orig_content = fin.read(filesize)
                fin.read(pad4(filesize))
                new_content = patch_cpio_initrc(orig_content)
                new_filesize = len(new_content)
                new_hdr = hdr[:54] + f"{new_filesize:08x}".encode("ascii") + hdr[62:]
                fout.write(new_hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                fout.write(new_content)
                fout.write(b"\0" * pad4(new_filesize))
                print(f"[T636 BUILDER] Patched {name_str}: {filesize} -> {new_filesize} bytes")

            elif name_str == "ramdisk-images/system.img":
                if filesize != SYSTEM_IMG_SIZE:
                    raise ValueError(f"system.img size mismatch: {filesize} != {SYSTEM_IMG_SIZE}")
                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)

                print(f"[T636 BUILDER] Patching {name_str} ({filesize} bytes)...")
                # 1. Stream from 0 up to INIT_INODE_SIZE_OFFSET (134292740)
                rem = INIT_INODE_SIZE_OFFSET
                while rem > 0:
                    chunk = fin.read(min(rem, 8 * 1024 * 1024))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 2. Inode size field (4 bytes)
                orig_inode_size_bytes = fin.read(4)
                orig_system_sha.update(orig_inode_size_bytes)
                orig_inode_size = struct.unpack("<I", orig_inode_size_bytes)[0]
                if orig_inode_size != ORIG_INITRC_SIZE:
                    raise ValueError(f"Unexpected inode 694 size: {orig_inode_size} != {ORIG_INITRC_SIZE}")

                new_inode_size_bytes = struct.pack("<I", new_inode_size)
                fout.write(new_inode_size_bytes)
                new_system_sha.update(new_inode_size_bytes)
                print(f"[T636 BUILDER] Patched inode size: {orig_inode_size} -> {new_inode_size}")

                # 3. Stream from INIT_INODE_SIZE_OFFSET + 4 up to INIT_DATA_OFFSET (398934016)
                rem = INIT_DATA_OFFSET - (INIT_INODE_SIZE_OFFSET + 4)
                while rem > 0:
                    chunk = fin.read(min(rem, 8 * 1024 * 1024))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 4. Read the 15-block ext4 data extent (INIT_ALLOCATED_SIZE = 61440)
                orig_extent = fin.read(INIT_ALLOCATED_SIZE)
                orig_system_sha.update(orig_extent)
                orig_initrc = orig_extent[:orig_inode_size]
                if LOAD_EXPORTS_ANCHOR not in orig_initrc:
                    raise ValueError("load_exports anchor not found in system.img init.rc")
                new_initrc = orig_initrc.replace(LOAD_EXPORTS_ANCHOR, BCP_OVERRIDE_BLOCK, 1)
                if len(new_initrc) != new_inode_size:
                    raise AssertionError(f"new_initrc length {len(new_initrc)} != calculated {new_inode_size}")

                new_extent = new_initrc + orig_extent[new_inode_size:]
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError("Extent size changed!")
                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T636 BUILDER] Patched init.rc extent ({len(orig_initrc)} -> {len(new_initrc)} bytes)")

                # 5. Stream remainder of system.img
                rem = filesize - (INIT_DATA_OFFSET + INIT_ALLOCATED_SIZE)
                while rem > 0:
                    chunk = fin.read(min(rem, 8 * 1024 * 1024))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))

            elif name_str == "TRAILER!!!":
                # Inject t636-diag.sh immediately before TRAILER!!!
                fout.write(diag_cpio_entry)
                print(f"[T636 BUILDER] Injected /t636-diag.sh entry into CPIO archive ({len(diag_cpio_entry)} bytes)")

                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, 8 * 1024 * 1024))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))
                break

            else:
                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, 8 * 1024 * 1024))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T636",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": OUTPUT_IMAGE.stat().st_size,
        "orig_system_sha256": orig_system_sha.hexdigest(),
        "new_system_sha256": new_system_sha.hexdigest(),
        "build_elapsed_seconds": round(elapsed, 2),
        "bootclasspath_entries_count": 20,
        "bootclasspath_20": BOOTCLASSPATH_20,
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"[T636 BUILDER] Build complete in {elapsed:.1f}s.")
    print(f"[T636 BUILDER] Original embedded system.img SHA-256: {orig_system_sha.hexdigest()}")
    print(f"[T636 BUILDER] Patched embedded system.img SHA-256:  {new_system_sha.hexdigest()}")
    print(f"[T636 BUILDER] New fix initramfs SHA-256:             {output_sha}")
    print(f"[T636 BUILDER] Output size:                           {OUTPUT_IMAGE.stat().st_size} bytes")


if __name__ == "__main__":
    build()
