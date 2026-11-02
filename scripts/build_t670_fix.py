#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T670:
idmap-systemproperties-stop-init-control-queue-watchdog-60s (Chain A)

PINNED MECHANISM & CONTROL QUEUE FLOOD ELIMINATION:
1. Root cause:
   In Cycle 1 (PID 3765), system_server completed Phase 600/1000, unlocked user 0
   (LockSettingsService CE unlock line 27220, finishUserUnlocking-0 line 27221), and
   processed sys.boot_completed=1 (line 27825).
   After user 0 unlock, OverlayManagerService closed its connection to idmap2d.
   IdmapDaemon$Connection.lambda$close$0 called IdmapDaemon.stopIdmapService() on
   android.fg, which called SystemService.stop("idmap2d") ->
   SystemProperties.set("ctl.stop", "idmap2d").
   This synchronous property set blocked for 94 seconds in SystemProperties.native_set
   waiting for init on /dev/socket/property_service.
   At line 42732, Watchdog killed system_server (PID 3765) with SIG 9 (line 42754):
   "Blocked in handler on on foreground thread (android.fg) for 94s".

2. Causal flood:
   The init property service / control message queue was saturated with 4,160 dropped
   messages:
   "init: Too many pending control messages, dropped 'interface_start' for
   'android.hardware.media.c2@1.0::IComponentStore/software' from pid: 948"
   (libc error code 0x20, lines 37718-42567).
   hwservicemanager (PID 948) flooded init because media.c2 (mediaswcodec) was declared
   in the framework VINTF manifest (/system/etc/vintf/manifest/manifest_media_c2_software.xml)
   but com.android.media.swcodec was skipped during APEX mounting (due to lacking javalib/)
   and had no interface registration in init's .rc tables.
   Consequently, init could not start it and dropped the messages, keeping init's
   pending_control_messages queue (>100) permanently saturated.

3. Pinned Fix:
   - In early-ramdisk and t636-diag.sh, pre-create /data/misc/apexdata/com.android.media.swcodec
     and /data/data/com.android.media.swcodec (0771 1000:1000).
   - In t636-diag.sh do_prep, bind-mount /system/system_ext/apex/com.android.media.swcodec
     to /apex/com.android.media.swcodec (enabling flattened native APEX access).
   - Direct-spawn /apex/com.android.media.swcodec/bin/mediaswcodec early in do_prep with
     marker T670_MEDIASWCODEC_EARLY_SPAWNED so android.hardware.media.c2@1.0::IComponentStore/software
     registers with hwservicemanager before client requests arrive, stopping the flood at source.
   - Continuously supervise mediaswcodec and its APEX mount in watch_early_services().
   - In root CPIO, inject vendor/etc/init/mediaswcodec.rc and vendor/etc/init/hw/mediaswcodec.rc
     declaring the service and interface mappings for media.c2@1.0, 1.1, and 1.2 software instances.
   - Add swcodec PID tracking to T642_BOOT_STATE.
   - Preserve all T664-T669 fixes: ext4 /data on /dev/loop2, tmpfs /data/media with SB_POSIXACL,
     /data/misc/gatekeeper (0700 1000:1000), /data/vendor_de/0/fpdata (0700 1000:1000),
     direct-spawned gnss, gatekeeperd, and fingerprint-service with continuous supervision.
   - Zero modifications to system.img (preserves ART boot profiles).

Base Image (T669 Fix): SHA-256 c6c431f49f5e24f11cc1a289f8bfade7cdc75b6adc69ee7b42f3de9f21593548
Canonical Kernel:      SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t670-idmap-systemproperties-stop-init-control-queue-watchdog-60s-initramfs.cpio.gz
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
BASE_IMAGE = ROOT / "guest-inputs/t669-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t670-idmap-systemproperties-stop-init-control-queue-watchdog-60s-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t670-manifest.json"

EXPECTED_BASE_SHA256 = "c6c431f49f5e24f11cc1a289f8bfade7cdc75b6adc69ee7b42f3de9f21593548"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_SYSTEM_IMG_SHA256 = "7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08"

CHUNK = 8 * 1024 * 1024

MEDIASWCODEC_RC_CONTENT = b"""service media.swcodec /apex/com.android.media.swcodec/bin/mediaswcodec
    interface android.hardware.media.c2@1.0::IComponentStore software
    interface android.hardware.media.c2@1.1::IComponentStore software
    interface android.hardware.media.c2@1.2::IComponentStore software
    class main hal
    user mediacodec
    group camera drmrpc mediadrm
    ioprio rt 4
    task_profiles ProcessCapacityHigh
"""


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
    """Pre-create /data/misc/apexdata/com.android.media.swcodec and /data/data/com.android.media.swcodec."""
    target_pattern = 'echo "[T669] /data/vendor_de/0/fpdata pre-created (0700 1000:1000)"\n'
    replacement = (
        'echo "[T669] /data/vendor_de/0/fpdata pre-created (0700 1000:1000)"\n'
        '# T670: Pre-create /data/misc/apexdata/com.android.media.swcodec and /data/data/com.android.media.swcodec\n'
        '"$TOYBOX" mkdir -p /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '"$TOYBOX" chmod 0771 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '"$TOYBOX" chown 1000:1000 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '"$TOYBOX" mkdir -p /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        '"$TOYBOX" chmod 0771 /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        '"$TOYBOX" chown 1000:1000 /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        'echo "[T670] /data/.../com.android.media.swcodec pre-created (0771 1000:1000)"\n'
    )

    if target_pattern not in original:
        raise ValueError("Could not find target pattern in early-ramdisk!")

    return original.replace(target_pattern, replacement, 1)


def patch_diag_script(script_text: str) -> str:
    """Enhance t636-diag.sh with mediaswcodec bind mount, direct spawn & supervision."""
    # 1. Update APEX loop to also bind-mount APEXes with bin/ (like com.android.media.swcodec)
    apex_target = (
        '        for d in "$base"/com.android.*; do\n'
        '            [ -d "$d/javalib" ] || continue\n'
        '            pkg=${d##*/}\n'
        '            if [ ! -d "/apex/$pkg/javalib" ]; then\n'
    )
    apex_repl = (
        '        for d in "$base"/com.android.*; do\n'
        '            [ -d "$d/javalib" ] || [ -d "$d/bin" ] || continue\n'
        '            pkg=${d##*/}\n'
        '            if [ ! -d "/apex/$pkg" ] || [ ! -e "/apex/$pkg/javalib" -a ! -e "/apex/$pkg/bin" ]; then\n'
    )
    if apex_target in script_text:
        script_text = script_text.replace(apex_target, apex_repl, 1)
    else:
        raise ValueError("Could not find apex_target in t636-diag.sh!")

    # 2. In do_prep: ensure swcodec APEX bind mount, dirs pre-created, and direct spawn mediaswcodec early
    prep_marker = (
        '    if [ -x /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu ]; then\n'
        '        /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu >/dev/console 2>&1 &\n'
        '        mark "T669_FINGERPRINT_EARLY_SPAWNED"\n'
        '    fi\n'
    )
    prep_addition = prep_marker + (
        '    # T670: Ensure com.android.media.swcodec is bind-mounted into /apex/com.android.media.swcodec\n'
        '    if [ -d /system/system_ext/apex/com.android.media.swcodec ] && [ ! -d /apex/com.android.media.swcodec/bin ]; then\n'
        '        "$TOY" mkdir -p /apex/com.android.media.swcodec 2>/dev/null || :\n'
        '        "$TOY" mount -o bind /system/system_ext/apex/com.android.media.swcodec /apex/com.android.media.swcodec 2>/dev/null || :\n'
        '        mark "T670_SWCODEC_APEX_BIND_MOUNTED"\n'
        '    fi\n'
        '    # T670: Pre-create /data/misc/apexdata/com.android.media.swcodec and /data/data/com.android.media.swcodec\n'
        '    "$TOY" mkdir -p /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '    "$TOY" chmod 0771 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '    "$TOY" chown 1000:1000 /data/misc/apexdata/com.android.media.swcodec 2>/dev/null || :\n'
        '    "$TOY" mkdir -p /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        '    "$TOY" chmod 0771 /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        '    "$TOY" chown 1000:1000 /data/data/com.android.media.swcodec 2>/dev/null || :\n'
        '    mark "T670_SWCODEC_DIRS_PREPARED"\n'
        '    # T670: Direct spawn mediaswcodec early (bypass ctl_start queue drop)\n'
        '    if [ -x /apex/com.android.media.swcodec/bin/mediaswcodec ]; then\n'
        '        /apex/com.android.media.swcodec/bin/mediaswcodec >/dev/console 2>&1 &\n'
        '        mark "T670_MEDIASWCODEC_EARLY_SPAWNED"\n'
        '    elif [ -x /system/system_ext/apex/com.android.media.swcodec/bin/mediaswcodec ]; then\n'
        '        /system/system_ext/apex/com.android.media.swcodec/bin/mediaswcodec >/dev/console 2>&1 &\n'
        '        mark "T670_MEDIASWCODEC_EARLY_SPAWNED"\n'
        '    fi\n'
    )
    if prep_marker in script_text:
        script_text = script_text.replace(prep_marker, prep_addition, 1)
    else:
        raise ValueError("Could not find prep_marker in t636-diag.sh!")

    # 3. Add mediaswcodec supervision in watch_early_services
    watch_marker = (
        '        if ! "$TOY" pidof android.hardware.biometrics.fingerprint-service.ranchu >/dev/null 2>&1 && ! "$TOY" pidof fingerprint-service.ranchu >/dev/null 2>&1; then\n'
        '            if [ -x /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu ]; then\n'
        '                "$TOY" mkdir -p /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '                "$TOY" chmod 0700 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '                "$TOY" chown 1000:1000 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '                /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu >/dev/console 2>&1 &\n'
        '                mark "T669_FINGERPRINT_RESPAWNED"\n'
        '            fi\n'
        '        fi\n'
    )
    watch_addition = watch_marker + (
        '        # T670: Supervise com.android.media.swcodec bind mount and mediaswcodec process\n'
        '        if [ ! -d /apex/com.android.media.swcodec/bin ] && [ -d /system/system_ext/apex/com.android.media.swcodec ]; then\n'
        '            "$TOY" mkdir -p /apex/com.android.media.swcodec 2>/dev/null || :\n'
        '            "$TOY" mount -o bind /system/system_ext/apex/com.android.media.swcodec /apex/com.android.media.swcodec 2>/dev/null || :\n'
        '            mark "T670_SWCODEC_APEX_REMOUNTED"\n'
        '        fi\n'
        '        if ! "$TOY" pidof mediaswcodec >/dev/null 2>&1; then\n'
        '            if [ -x /apex/com.android.media.swcodec/bin/mediaswcodec ]; then\n'
        '                /apex/com.android.media.swcodec/bin/mediaswcodec >/dev/console 2>&1 &\n'
        '                mark "T670_MEDIASWCODEC_RESPAWNED"\n'
        '            elif [ -x /system/system_ext/apex/com.android.media.swcodec/bin/mediaswcodec ]; then\n'
        '                /system/system_ext/apex/com.android.media.swcodec/bin/mediaswcodec >/dev/console 2>&1 &\n'
        '                mark "T670_MEDIASWCODEC_RESPAWNED"\n'
        '            fi\n'
        '        fi\n'
    )
    if watch_marker in script_text:
        script_text = script_text.replace(watch_marker, watch_addition, 1)
    else:
        raise ValueError("Could not find watch_marker in t636-diag.sh!")

    # 4. Add swcodec status to T642_BOOT_STATE
    fp_marker = 'fp_pid=$("$TOY" pidof android.hardware.biometrics.fingerprint-service.ranchu 2>/dev/null || "$TOY" pidof fingerprint-service.ranchu 2>/dev/null || echo "none")\n'
    swcodec_addition = fp_marker + '        swcodec_pid=$("$TOY" pidof mediaswcodec 2>/dev/null || echo "none")\n'
    if fp_marker in script_text:
        script_text = script_text.replace(fp_marker, swcodec_addition, 1)
    else:
        raise ValueError("Could not find fp_marker in t636-diag.sh!")

    old_boot_marker = 'gatekeeperd=$gk_pid gnss=$gnss_pid fingerprint=$fp_pid"'
    new_boot_marker = 'gatekeeperd=$gk_pid gnss=$gnss_pid fingerprint=$fp_pid swcodec=$swcodec_pid"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)
    else:
        raise ValueError("Could not find old_boot_marker in t636-diag.sh!")

    return script_text


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T670 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # 1. Verify base image (T669)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = BASE_IMAGE.stat()
    if base_stat.st_nlink < 2:
        print(f"[T670 BUILDER] WARNING: Base image st_nlink={base_stat.st_nlink} (expected >1).")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA-256 mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T670 BUILDER] Base image verified: {BASE_IMAGE} ({base_sha})")

    # 2. Verify canonical kernel
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.is_file():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA-256 mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T670 BUILDER] Canonical kernel verified: {kernel_path} ({kernel_sha})")

    # 3. Stream base CPIO and build ONE fix initramfs
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    patched_ramdisk = False
    patched_diag = False
    verified_system_img = False
    verified_userdata_img = False

    print(f"[T670 BUILDER] Streaming base CPIO and applying T670 mediaswcodec & init control queue fixes...")
    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if len(hdr) < 110 or not hdr.startswith(b"070701"):
                break

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1", errors="replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "TRAILER!!!":
                # Inject vendor/etc/init/mediaswcodec.rc and vendor/etc/init/hw/mediaswcodec.rc before TRAILER
                print(f"[T670 BUILDER] Injecting vendor/etc/init/mediaswcodec.rc and vendor/etc/init/hw/mediaswcodec.rc...")
                fout.write(format_cpio_entry("vendor/etc/init/mediaswcodec.rc", MEDIASWCODEC_RC_CONTENT, mode=0o100644, ino=91101))
                fout.write(format_cpio_entry("vendor/etc/init/hw/mediaswcodec.rc", MEDIASWCODEC_RC_CONTENT, mode=0o100644, ino=91102))

                # Write TRAILER!!!
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                break

            if name_str == "early-ramdisk":
                content = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                text = content.decode("utf-8")
                patched_text = patch_early_ramdisk(text)
                patched_bytes = patched_text.encode("utf-8")
                ino = int(hdr[6:14], 16)
                mode = int(hdr[14:22], 16)
                fout.write(format_cpio_entry(name_str, patched_bytes, mode=mode, ino=ino))
                patched_ramdisk = True
                print(f"[T670 BUILDER] early-ramdisk patched successfully ({len(content)} -> {len(patched_bytes)} bytes)")
                continue

            if name_str == "t636-diag.sh":
                content = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                text = content.decode("utf-8")
                patched_text = patch_diag_script(text)
                patched_bytes = patched_text.encode("utf-8")
                ino = int(hdr[6:14], 16)
                mode = int(hdr[14:22], 16)
                fout.write(format_cpio_entry(name_str, patched_bytes, mode=mode, ino=ino))
                patched_diag = True
                print(f"[T670 BUILDER] t636-diag.sh patched successfully ({len(content)} -> {len(patched_bytes)} bytes)")
                continue

            if name_str == "ramdisk-images/system.img":
                print(f"[T670 BUILDER] Verifying and streaming system.img ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                h_sys = hashlib.sha256()
                remaining = filesize
                while remaining > 0:
                    read_sz = min(remaining, CHUNK)
                    buf = fin.read(read_sz)
                    if not buf:
                        raise EOFError("Unexpected EOF while reading system.img")
                    h_sys.update(buf)
                    fout.write(buf)
                    remaining -= len(buf)

                sys_sha = h_sys.hexdigest()
                if sys_sha != EXPECTED_SYSTEM_IMG_SHA256:
                    raise ValueError(f"system.img SHA-256 mismatch: {sys_sha} != {EXPECTED_SYSTEM_IMG_SHA256}")
                verified_system_img = True
                print(f"[T670 BUILDER] system.img verified bit-for-bit: {sys_sha}")

                pad_data = pad4(filesize)
                if pad_data:
                    pad_buf = fin.read(pad_data)
                    fout.write(pad_buf)
                continue

            if name_str == "ramdisk-images/userdata.img":
                print(f"[T670 BUILDER] Streaming userdata.img ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                remaining = filesize
                while remaining > 0:
                    read_sz = min(remaining, CHUNK)
                    buf = fin.read(read_sz)
                    if not buf:
                        raise EOFError("Unexpected EOF while reading userdata.img")
                    fout.write(buf)
                    remaining -= len(buf)

                verified_userdata_img = True
                pad_data = pad4(filesize)
                if pad_data:
                    pad_buf = fin.read(pad_data)
                    fout.write(pad_buf)
                continue

            # Stream through unchanged
            fout.write(hdr)
            fout.write(name_bytes)
            if pad_name:
                fout.write(b"\0" * pad_name)

            remaining = filesize
            while remaining > 0:
                read_sz = min(remaining, CHUNK)
                buf = fin.read(read_sz)
                if not buf:
                    raise EOFError(f"Unexpected EOF while reading {name_str}")
                fout.write(buf)
                remaining -= len(buf)

            pad_data = pad4(filesize)
            if pad_data:
                pad_buf = fin.read(pad_data)
                fout.write(pad_buf)

    if not patched_ramdisk:
        raise RuntimeError("Failed to patch early-ramdisk (entry not encountered)!")
    if not patched_diag:
        raise RuntimeError("Failed to patch t636-diag.sh (entry not encountered)!")
    if not verified_system_img:
        raise RuntimeError("Failed to verify system.img!")
    if not verified_userdata_img:
        raise RuntimeError("Failed to verify userdata.img!")

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_size = OUTPUT_IMAGE.stat().st_size
    output_sha = sha256_file(OUTPUT_IMAGE)
    elapsed = time.monotonic() - t0
    print(f"[T670 BUILDER] Build complete in {elapsed:.1f}s.")
    print(f"[T670 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T670 BUILDER] Output SHA-256: {output_sha} ({output_size} bytes)")

    manifest = {
        "track": "T670",
        "mission": "idmap-systemproperties-stop-init-control-queue-watchdog-60s",
        "created_utc": utc_now(),
        "base_image": {
            "path": str(BASE_IMAGE),
            "sha256": base_sha,
            "bytes": base_stat.st_size,
        },
        "canonical_kernel": {
            "path": str(kernel_path),
            "sha256": kernel_sha,
            "bytes": kernel_path.stat().st_size,
        },
        "system_img": {
            "sha256": EXPECTED_SYSTEM_IMG_SHA256,
            "untouched": True,
        },
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": output_sha,
            "bytes": output_size,
        },
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    print(f"[T670 BUILDER] Manifest written: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
