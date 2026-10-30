#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T646:
1. VINTF manifest declaration for Widevine DRM Factory HAL.
2. Logcat direct console streaming and printk ratelimit bypass to prevent log dropping.
3. SystemServer lifecycle & crash watcher to capture exact failure lines.
"""

from __future__ import annotations

import base64
import gzip
import hashlib
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track646-system-server-zygote-sigkill")
BASE_IMAGE = ROOT / "guest-inputs/t644-system-server-apex-namespace-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t646-system-server-zygote-sigkill-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t646-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "f466570b7c31da76aa9fcd5574f569cdd3bfc064e204e892d1b6b1e7b99e7c98"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

CHUNK = 8 * 1024 * 1024

VINTF_MANIFEST_XML = """<manifest version="1.0" type="device">
    <hal format="hidl">
        <name>android.hardware.graphics.composer</name>
        <transport>hwbinder</transport>
        <version>2.1</version>
        <interface>
            <name>IComposer</name>
            <instance>default</instance>
        </interface>
    </hal>
    <hal format="hidl">
        <name>android.hardware.graphics.allocator</name>
        <transport>hwbinder</transport>
        <version>2.0</version>
        <interface>
            <name>IAllocator</name>
            <instance>default</instance>
        </interface>
    </hal>
    <hal format="hidl">
        <name>android.hardware.graphics.mapper</name>
        <transport arch="64">passthrough</transport>
        <version>2.0</version>
        <interface>
            <name>IMapper</name>
            <instance>default</instance>
        </interface>
    </hal>
    <hal format="aidl">
        <name>android.hardware.drm</name>
        <version>1</version>
        <interface>
            <name>IDrmFactory</name>
            <instance>widevine</instance>
        </interface>
    </hal>
    <hal format="hidl">
        <name>android.hardware.drm</name>
        <transport>hwbinder</transport>
        <version>1.3</version>
        <interface>
            <name>IDrmFactory</name>
            <instance>widevine</instance>
        </interface>
        <interface>
            <name>ICryptoFactory</name>
            <instance>widevine</instance>
        </interface>
        <fqname>@1.3::IDrmFactory/widevine</fqname>
        <fqname>@1.3::ICryptoFactory/widevine</fqname>
    </hal>
</manifest>
"""

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

def build_diag_script(base_script: str) -> bytes:
    b64_manifest = base64.b64encode(VINTF_MANIFEST_XML.encode("utf-8")).decode("ascii")

    # Add VINTF manifest and printk un-ratelimiting to do_prep
    t646_prep_block = f"""
    # T646: Update VINTF manifest with Widevine DRM HAL
    mark "T646_UPDATING_VINTF_MANIFEST"
    "$TOY" mkdir -p /vendor/etc/vintf
    echo -n "{b64_manifest}" | "$TOY" base64 -d > /data/manifest.xml
    "$TOY" cp /data/manifest.xml /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    "$TOY" mount -o bind /data/manifest.xml /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    "$TOY" chmod 0644 /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    if "$TOY" grep -q "widevine" /vendor/etc/vintf/manifest.xml; then
        mark "T646_VINTF_MANIFEST_VERIFIED_OK"
    else
        mark "T646_VINTF_MANIFEST_VERIFICATION_FAILED"
    fi

    # T646: Disable printk ratelimiting
    if [ -f /proc/sys/kernel/printk_ratelimit ]; then
        echo 0 > /proc/sys/kernel/printk_ratelimit 2>/dev/null || :
        echo 0 > /proc/sys/kernel/printk_ratelimit_burst 2>/dev/null || :
        mark "T646_PRINTK_RATELIMIT_DISABLED"
    fi
"""

    anchor = '    mark "T642_DO_PREP_EXIT pid=$$"'
    if anchor not in base_script:
        raise ValueError("Could not find anchor in base_script")
    script = base_script.replace(anchor, t646_prep_block + "\n" + anchor)

    # Replace start_logcat to stream to /dev/console (un-ratelimited)
    old_start_logcat = """start_logcat() {
    while [ ! -x /system/bin/logcat ]; do
        "$TOY" sleep 0.5 2>/dev/null || :
    done
    mark T642_LOGCAT_STREAMER_STARTED
    while :; do
        /system/bin/logcat -b all -v threadtime >> "$KMSG" 2>&1
        "$TOY" sleep 1 2>/dev/null || :
    done
}"""

    new_start_logcat = """start_logcat() {
    cdev=/dev/console
    [ -c "$cdev" ] || cdev=/dev/ttyAMA0
    while [ ! -x /system/bin/logcat ]; do
        "$TOY" sleep 0.5 2>/dev/null || :
    done
    mark T642_LOGCAT_STREAMER_STARTED
    # T646: Stream directly to console to prevent kernel devkmsg ratelimiting
    while :; do
        /system/bin/logcat -b all -v threadtime >> "$cdev" 2>&1
        "$TOY" sleep 1 2>/dev/null || :
    done
}"""

    if old_start_logcat in script:
        script = script.replace(old_start_logcat, new_start_logcat)
    else:
        print("[WARN] Could not find exact old_start_logcat block")

    # Replace watch_system_server with exit-detection and crash dumping
    old_watch_ss = """watch_system_server() {
    mark T642_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss_pids=""
    while :; do
        ss_prop=$(/system/bin/getprop init.svc.system_server 2>/dev/null)
        sspids=$("$TOY" pidof system_server 2>/dev/null)
        if [ -n "$sspids" ]; then
            for sspid in $sspids; do
                case "$seen_ss_pids" in
                    *" $sspid "*) ;;
                    *)
                        seen_ss_pids="$seen_ss_pids $sspid "
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
        elif [ -n "$ss_prop" ]; then
            mark "T642_SYSTEM_SERVER_PROP_ONLY prop=$ss_prop"
        fi
        "$TOY" sleep 0.5 2>/dev/null || :
    done
}"""

    new_watch_ss = """watch_system_server() {
    cdev=/dev/console
    [ -c "$cdev" ] || cdev=/dev/ttyAMA0
    mark T642_SYSTEM_SERVER_WATCHER_STARTED
    seen_ss_pids=""
    active_ss_pid=""
    while :; do
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
        "$TOY" sleep 0.2 2>/dev/null || :
    done
}"""

    if old_watch_ss in script:
        script = script.replace(old_watch_ss, new_watch_ss)
    else:
        print("[WARN] Could not find exact old_watch_ss block")

    return script.encode("utf-8")

def main():
    t0 = time.monotonic()
    print("[T646 BUILDER] Starting T646 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T646 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T646 BUILDER] Verified T644 base image SHA: {base_sha}")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    # Read base t636-diag.sh from extracted copy
    with open(ROOT / "extracted-t636-diag.sh", "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_content = build_diag_script(base_diag)
    print(f"[T646 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_content)} bytes)")

    vintf_manifest_bytes = VINTF_MANIFEST_XML.encode("utf-8")

    # Stream CPIO archive, replacing t636-diag.sh and vendor/etc/vintf/manifest.xml
    entries_processed = 0
    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr:
                break
            if len(hdr) != 110 or not hdr.startswith(b"070701"):
                raise ValueError(f"Corrupt CPIO header: {hdr[:16]}")

            namesize = int(hdr[94:102].decode("ascii"), 16)
            filesize = int(hdr[54:62].decode("ascii"), 16)

            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("ascii", errors="replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            entries_processed += 1

            if name_str == "t636-diag.sh":
                print(f"[T646 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_content)} bytes)...")
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_content)
                fout.write(new_entry)
            elif name_str == "vendor/etc/vintf/manifest.xml":
                print(f"[T646 BUILDER] Replacing vendor/etc/vintf/manifest.xml ({filesize} -> {len(vintf_manifest_bytes)} bytes)...")
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                new_entry = format_cpio_entry("vendor/etc/vintf/manifest.xml", vintf_manifest_bytes, mode=0o100644)
                fout.write(new_entry)
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

            if name_str == "TRAILER!!!":
                break

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    output_bytes = os.path.getsize(OUTPUT_IMAGE)
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T646",
        "description": "Fix Widevine VINTF manifest, bypass printk ratelimit, stream logcat to console, and detect SystemServer exit",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": output_bytes,
        "build_elapsed_seconds": round(elapsed, 2),
        "images": {
            "kernel": {
                "path": str(kernel_path),
                "sha256": kernel_sha,
            },
            "initramfs": {
                "path": str(OUTPUT_IMAGE),
                "sha256": output_sha,
                "bytes": output_bytes,
            },
            "base_t644_initramfs": {
                "path": str(BASE_IMAGE),
                "sha256": base_sha,
            },
        },
    }

    with open(MANIFEST_FILE, "w") as f:
        json.dump(manifest, f, indent=2)

    FLIGHT_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    with open(FLIGHT_MANIFEST, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"[T646 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T646 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T646 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes)")
    print(f"[T646 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")

if __name__ == "__main__":
    main()
