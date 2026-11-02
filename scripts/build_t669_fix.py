#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T669:
biometrics-fingerprint-aidl-lazy-hal-and-idmap-propertyset-watchdog-60s (Chain A)

PINNED MECHANISM & DUAL 60s BLOCK CHARACTERIZATION:
1. MAIN-thread half (CAUSAL):
   In Cycle 1 (PID 4064), SystemServer synchronously blocked for 106+ seconds
   in FingerprintProvider.getHalInstance() -> ServiceManager.waitForDeclaredService(
   "android.hardware.biometrics.fingerprint.IFingerprint/default") starting at
   17:34:02Z (raw line 25911) during user 0 start.
   vendor.biometrics.fingerprint-service.ranchu is defined with 'class hal' and 'oneshot'
   in vendor.img. It exited with status 0 at 17:33:14Z (line 19488) after 10s idle.
   When ServiceManager (PID 896) attempted to start it via 'ctl.interface_start
   aidl/android.hardware.biometrics.fingerprint.IFingerprint/default', init dropped
   the control message because init's control queue was flooded (>247 messages) by
   hwservicemanager (PID 899) repeatedly requesting missing
   android.hardware.media.c2@1.0::IComponentStore/software.
   Because the service was never started, waitForDeclaredService never returned,
   holding SystemServer's main thread until Watchdog terminated PID 4064 at line 40174.

2. ANDROID.FG half (CO-PRESENT SYMPTOM):
   At ~17:34:48Z, OverlayManagerService completed user 0 overlay checks and closed
   its connection to idmap2d. IdmapDaemon$Connection.lambda$close$0 called
   IdmapDaemon.stopIdmapService() on android.fg, which invoked SystemService.stop("idmap2d").
   This called SystemProperties.set("ctl.stop", "idmap2d"), connecting to
   /dev/socket/property_service. Because init's control message queue was saturated
   and init processes only 1 control message per main loop iteration, android.fg
   blocked in __system_property_set waiting on init's socket.
   Evidence from Cycle 2 proves this is purely a symptom: once fingerprint HAL is
   available, servicemanager stops spamming ctl.interface_start, the queue does
   not saturate, and stopIdmapService on android.fg completes in 136ms (Cycle 2 line 56992).

3. Pinned Fix:
   - Pre-create /data/vendor_de/0/fpdata (0700 1000:1000) in early-ramdisk and t636-diag.sh.
   - Directly spawn /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu
     early in t636-diag.sh do_prep, ensuring IFingerprint/default is registered before
     SystemServer user 0 start and bypassing dropped ctl.interface_start messages.
   - Supervise vendor.biometrics.fingerprint-service.ranchu continuously in
     watch_early_services() in t636-diag.sh, respawning it whenever idle timeout occurs.
   - Preserve all T664-T668 fixes: ext4 /data on /dev/loop2, tmpfs /data/media with
     SB_POSIXACL, /data/misc/gatekeeper (0700 1000:1000), direct-spawned gnss & gatekeeperd.
   - Zero modifications to system.img (preserves ART boot profiles).

Base Image (T668 Run 2): SHA-256 7b93fdb23af4edef3300af94fc49743e41256d771b5982db1f70427c45849155
Canonical Kernel:        SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t669-biometrics-fingerprint-aidl-lazy-hal-and-idmap-propertyset-watchdog-60s-initramfs.cpio.gz
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
BASE_IMAGE = ROOT / "guest-inputs/t668-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t669-biometrics-fingerprint-aidl-lazy-hal-and-idmap-propertyset-watchdog-60s-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t669-manifest.json"

EXPECTED_BASE_SHA256 = "7b93fdb23af4edef3300af94fc49743e41256d771b5982db1f70427c45849155"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_SYSTEM_IMG_SHA256 = "7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08"

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
    """Pre-create /data/vendor_de/0/fpdata (0700 1000:1000) for fingerprint HAL."""
    target_pattern = 'echo "[T668] /data/misc/gatekeeper pre-created (0700 1000:1000)"\n'
    replacement = (
        'echo "[T668] /data/misc/gatekeeper pre-created (0700 1000:1000)"\n'
        '# T669: Pre-create /data/vendor_de/0/fpdata for fingerprint-service\n'
        '"$TOYBOX" mkdir -p /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '"$TOYBOX" chmod 0700 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '"$TOYBOX" chown 1000:1000 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        'echo "[T669] /data/vendor_de/0/fpdata pre-created (0700 1000:1000)"\n'
    )

    if target_pattern not in original:
        raise ValueError("Could not find target pattern in early-ramdisk!")

    return original.replace(target_pattern, replacement, 1)


def patch_diag_script(script_text: str) -> str:
    """Enhance t636-diag.sh with fingerprint HAL direct spawn & supervision."""
    # 1. In do_prep: pre-create fpdata and directly spawn vendor.biometrics.fingerprint-service.ranchu
    prep_marker = '        mark "T668_GATEKEEPERD_EARLY_SPAWNED"\n    fi\n'
    prep_addition = (
        '        mark "T668_GATEKEEPERD_EARLY_SPAWNED"\n    fi\n'
        '    # T669: Pre-create /data/vendor_de/0/fpdata\n'
        '    "$TOYBOX" mkdir -p /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '    "$TOYBOX" chmod 0700 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '    "$TOYBOX" chown 1000:1000 /data/vendor_de/0/fpdata 2>/dev/null || :\n'
        '    mark "T669_FINGERPRINT_DIR_PREPARED"\n'
        '    # T669: Direct spawn vendor.biometrics.fingerprint-service.ranchu early (bypass ctl_start queue drop)\n'
        '    if [ -x /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu ]; then\n'
        '        /vendor/bin/hw/android.hardware.biometrics.fingerprint-service.ranchu >/dev/console 2>&1 &\n'
        '        mark "T669_FINGERPRINT_EARLY_SPAWNED"\n'
        '    fi\n'
    )
    if prep_marker in script_text:
        script_text = script_text.replace(prep_marker, prep_addition, 1)
    else:
        raise ValueError("Could not find prep_marker in t636-diag.sh!")

    # 2. Add fingerprint supervision in watch_early_services
    watch_marker = '                mark "T668_GNSS_RESPAWNED"\n            fi\n        fi\n'
    watch_addition = (
        '                mark "T668_GNSS_RESPAWNED"\n            fi\n        fi\n'
        '        # T669: Supervise vendor.biometrics.fingerprint-service.ranchu\n'
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
    if watch_marker in script_text:
        script_text = script_text.replace(watch_marker, watch_addition, 1)
    else:
        raise ValueError("Could not find watch_marker in t636-diag.sh!")

    # 3. Add fingerprint status to T642_BOOT_STATE
    media_fs_marker = 'gnss_pid=$("$TOY" pidof android.hardware.gnss-service.ranchu 2>/dev/null || echo "none")\n'
    svc_status_addition = (
        'gnss_pid=$("$TOY" pidof android.hardware.gnss-service.ranchu 2>/dev/null || echo "none")\n'
        '        fp_pid=$("$TOY" pidof android.hardware.biometrics.fingerprint-service.ranchu 2>/dev/null || "$TOY" pidof fingerprint-service.ranchu 2>/dev/null || echo "none")\n'
    )
    if media_fs_marker in script_text:
        script_text = script_text.replace(media_fs_marker, svc_status_addition, 1)

    old_boot_marker = 'gatekeeperd=$gk_pid gnss=$gnss_pid"'
    new_boot_marker = 'gatekeeperd=$gk_pid gnss=$gnss_pid fingerprint=$fp_pid"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    return script_text


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T669 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # 1. Verify base image (T668)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = BASE_IMAGE.stat()
    if base_stat.st_nlink < 2:
        print(f"[T669 BUILDER] WARNING: Base image st_nlink={base_stat.st_nlink} (expected >1).")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA-256 mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T669 BUILDER] Base image verified: {BASE_IMAGE} ({base_sha})")

    # 2. Verify canonical kernel
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.is_file():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA-256 mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T669 BUILDER] Canonical kernel verified: {kernel_path} ({kernel_sha})")

    # 3. Stream base CPIO and build ONE fix initramfs
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    patched_ramdisk = False
    patched_diag = False
    verified_system_img = False
    verified_userdata_img = False

    print(f"[T669 BUILDER] Streaming base CPIO and applying T669 Biometrics Fingerprint fixes...")
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
                print(f"[T669 BUILDER] early-ramdisk patched successfully ({len(content)} -> {len(patched_bytes)} bytes)")
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
                print(f"[T669 BUILDER] t636-diag.sh patched successfully ({len(content)} -> {len(patched_bytes)} bytes)")
                continue

            if name_str == "ramdisk-images/system.img":
                print(f"[T669 BUILDER] Verifying and streaming system.img ({filesize} bytes)...")
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
                print(f"[T669 BUILDER] system.img verified bit-for-bit: {sys_sha}")

                pad_data = pad4(filesize)
                if pad_data:
                    pad_buf = fin.read(pad_data)
                    fout.write(pad_buf)
                continue

            if name_str == "ramdisk-images/userdata.img":
                print(f"[T669 BUILDER] Streaming userdata.img ({filesize} bytes)...")
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
    print(f"[T669 BUILDER] Build complete in {elapsed:.1f}s.")
    print(f"[T669 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T669 BUILDER] Output SHA-256: {output_sha} ({output_size} bytes)")

    manifest = {
        "track": "T669",
        "mission": "biometrics-fingerprint-aidl-lazy-hal-and-idmap-propertyset-watchdog-60s",
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
    print(f"[T669 BUILDER] Manifest written: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
