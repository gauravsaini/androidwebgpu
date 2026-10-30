#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T654:
1. Pinned SensorManager / DeviceStateManagerService Watchdog timeout mechanism:
   In SystemServer.startOtherServices() at line 1603:
   SystemServiceManager.startService(DeviceStateManagerService.class)
   initializes DeviceStateManagerService.<init> -> DeviceStatePolicyImpl.<init> ->
   DeviceStateProviderImpl.create(context) -> DeviceStateProviderImpl.createFromConfig() ->
   DeviceStateProviderImpl.setStateConditions():
   Line 363 unconditionally calls:
       final SensorManager sensorManager = mContext.getSystemService(SensorManager.class);
   SystemSensorManager.<init> calls nativeCreate() -> SensorManager::getInstanceForPackage() ->
   waitForSensorService(), which queries ServiceManager for 'sensorservice' in a loop.
   In SystemServer.startCoreServices(), StartNativeSensorService was submitted on
   SystemServerInitThreadPool:
   NativeSensorService.<init> -> new SensorService() -> SensorService::onFirstRef() ->
   SensorDevice::connectHalService() -> AidlSensorHalWrapper::connect():
   Because /vendor/etc/vintf/manifest/android.hardware.sensors-multihal.xml declares
   android.hardware.sensors.ISensors/default, AServiceManager_isDeclared() returns true.
   AidlSensorHalWrapper then calls AServiceManager_waitForService("android.hardware.sensors.ISensors/default"),
   which blocks indefinitely waiting for the AIDL HAL service to be registered.
   Because StartNativeSensorService blocks on the missing HAL, SensorService is never
   registered in ServiceManager, causing the main thread in DeviceStateManagerService to
   block in nativeCreate for 93s until Watchdog kills system_server.

2. Pinned mechanism to supply the missing sensors piece:
   In vendor.img:
   - In Track T589 (prepare_t589_stub.py), /vendor/etc/init/android.hardware.sensors-service-multihal.rc
     at offset 0x13b7000 (264 bytes) was replaced with a shell sleep stub:
     service vendor.sensors-hal-multihal /system/bin/sh -c "echo '[SENSORS-STUB] sensors stub active' > /dev/kmsg; while true; do sleep 3600; done"
     At boot (line 6745), Android init rejected this service because /system/bin/sh has no SELinux domain transition.
   - The compiled AIDL HAL binary /vendor/bin/hw/android.hardware.sensors-service.multihal
     (inode 106, 144,008 bytes) is present in vendor.img and implements HalProxyAidl,
     which calls AServiceManager_addService(halProxy->asBinder().get(), "android.hardware.sensors.ISensors/default").
   - In T585, sensors-hal-multihal crashed with signal 6 because /vendor/etc/sensors/hals.conf
     loaded android.hardware.sensors@2.1-impl.ranchu.so, which aborted when QEMU goldfish pipe / vsock sockets failed.
   - When hals.conf is empty (0 subHALs loaded), HalProxy initializes with 0 sensors,
     registers android.hardware.sensors.ISensors/default with ServiceManager without crashing,
     and unblocks SensorService!
3. The Fix:
   - In vendor.img:
     a) Restore offset 0x13b7000 to SENS_ORIG (264 bytes):
        service vendor.sensors-hal-multihal /vendor/bin/hw/android.hardware.sensors-service.multihal
     b) Inode 368 (hals.conf) i_size at offset 0x1af04 set to 0.
     c) Data block at 0x15d9000 zeroed out (44 null bytes).
   - In t636-diag.sh:
     In do_prep:
     Ensure /vendor/etc/sensors/hals.conf is empty (bind mount empty file).
     Explicitly start vendor.sensors-hal-multihal (/system/bin/start vendor.sensors-hal-multihal).
     Emit marker T654_FIX_APPLIED.
4. Stream CPIO archive into exactly ONE fix image:
   guest-image/t654-system-server-startotherservices-devicestatemanager-sensors-hal-watchdog-initramfs.cpio.gz.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track654-system-server-startotherservices-devicestatemanager-sensors-hal-watchdog")
BASE_IMAGE = ROOT / "guest-inputs/t653-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t654-system-server-startotherservices-devicestatemanager-sensors-hal-watchdog-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t654-manifest.json"

EXPECTED_BASE_SHA256 = "c264d438fc2f8de7b349edcf4e82f9ac26d662934d43f34d40337542f5905cab"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

VENDOR_IMG_SIZE = 134217728
SENS_RC_OFFSET = 0x13b7000
SENS_RC_LEN = 264

SENS_ORIG = (
    b"service vendor.sensors-hal-multihal /vendor/bin/hw/android.hardware.sensors-service.multihal\n"
    b"    class hal\n"
    b"    user system\n"
    b"    group system wakelock context_hub input uhid\n"
    b"    task_profiles ServiceCapacityLow\n"
    b"    capabilities BLOCK_SUSPEND\n"
    b"    rlimit rtprio 10 10\n"
)
assert len(SENS_ORIG) == SENS_RC_LEN, f"SENS_ORIG len {len(SENS_ORIG)} != {SENS_RC_LEN}"

HALS_INODE_OFFSET = 0x1af00
HALS_ISIZE_OFFSET = 0x1af04
HALS_DATA_OFFSET = 0x15d9000
HALS_DATA_LEN = 44

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


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    # Anchor for do_prep checks: right after T652 SettingsProvider DE data directory check
    anchor = '    # T651: Verify SdkSandbox is present in /system/system_ext/priv-app/'
    t654_check = """    # T654: Ensure /vendor/etc/sensors/hals.conf is empty and start sensors-hal-multihal
    mark "T654_PREP_SENSORS_MULTIHAL"
    "$TOY" rm -f /data/empty_hals.conf 2>/dev/null || :
    "$TOY" touch /data/empty_hals.conf 2>/dev/null || :
    "$TOY" chmod 0644 /data/empty_hals.conf 2>/dev/null || :
    "$TOY" mount -o bind /data/empty_hals.conf /vendor/etc/sensors/hals.conf 2>/dev/null || :
    /system/bin/start vendor.sensors-hal-multihal 2>/dev/null || :
    "$TOY" sleep 0.2 2>/dev/null || :
    mark "T654_FIX_APPLIED"

"""
    if anchor not in script:
        raise ValueError("Could not find anchor in base_script for do_prep check")
    script = script.replace(anchor, t654_check + anchor, 1)

    # In watch_boot_state, add sensors_hal property monitoring
    boot_anchor = '        sspids=$("$TOY" pidof system_server 2>/dev/null)\n        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)'
    boot_replace = ('        sspids=$("$TOY" pidof system_server 2>/dev/null)\n'
                    '        sensors_hal=$(/system/bin/getprop init.svc.vendor.sensors-hal-multihal 2>/dev/null)\n'
                    '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)')
    if boot_anchor in script:
        script = script.replace(boot_anchor, boot_replace, 1)
        script = script.replace('boot_completed=$completed"', 'sensors_hal=$sensors_hal boot_completed=$completed"', 1)

    return script.encode("utf-8")


def patch_vendor_img(vendor_bytes: bytearray) -> bytearray:
    if len(vendor_bytes) != VENDOR_IMG_SIZE:
        raise ValueError(f"vendor.img size mismatch: {len(vendor_bytes)} != {VENDOR_IMG_SIZE}")

    # 1. Patch android.hardware.sensors-service-multihal.rc at 0x13b7000
    orig_rc = bytes(vendor_bytes[SENS_RC_OFFSET:SENS_RC_OFFSET + SENS_RC_LEN])
    if b"vendor.sensors-hal-multihal" not in orig_rc:
        raise ValueError("Sensors RC service header not found at 0x13b7000")
    print(f"[T654 BUILDER] Replacing sensors RC at 0x13b7000 (264 bytes)...")
    vendor_bytes[SENS_RC_OFFSET:SENS_RC_OFFSET + SENS_RC_LEN] = SENS_ORIG

    # 2. Patch inode 368 i_size to 0
    cur_size = int.from_bytes(vendor_bytes[HALS_ISIZE_OFFSET:HALS_ISIZE_OFFSET + 4], "little")
    print(f"[T654 BUILDER] Current inode 368 i_size: {cur_size}, setting to 0...")
    vendor_bytes[HALS_ISIZE_OFFSET:HALS_ISIZE_OFFSET + 4] = b"\x00\x00\x00\x00"

    # 3. Zero out hals.conf data block at 0x15d9000
    print(f"[T654 BUILDER] Zeroing out hals.conf data block at 0x15d9000 (44 bytes)...")
    vendor_bytes[HALS_DATA_OFFSET:HALS_DATA_OFFSET + HALS_DATA_LEN] = b"\x00" * HALS_DATA_LEN

    return vendor_bytes


def main():
    t0 = time.monotonic()
    print("[T654 BUILDER] Starting T654 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T654 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T654 BUILDER] Verified T653 base image SHA: {base_sha}")

    # Read extracted base diag script
    diag_script_path = ROOT / "scripts/extracted_t652_diag.sh"
    with open(diag_script_path, "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_bytes = build_diag_script(base_diag)
    print(f"[T654 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_bytes)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T654 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if hdr[:6] != b"070701":
                raise ValueError(f"Invalid CPIO header magic: {hdr[:6]}")

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "ramdisk-images/vendor.img":
                print(f"[T654 BUILDER] Found vendor.img ({filesize} bytes), applying sensor HAL & hals.conf patches...")
                if filesize != VENDOR_IMG_SIZE:
                    raise ValueError(f"vendor.img size mismatch: {filesize} != {VENDOR_IMG_SIZE}")

                vendor_data = bytearray(fin.read(filesize))
                patched_vendor = patch_vendor_img(vendor_data)

                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                fout.write(patched_vendor)

                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

            elif name_str == "t636-diag.sh":
                print(f"[T654 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)...")
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
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
    output_inode = OUTPUT_IMAGE.stat().st_ino
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T654",
        "description": "Fix system_server DeviceStateManagerService SensorManager Watchdog timeout: restore sensors-hal-multihal service in vendor.img and empty hals.conf so HalProxyAidl registers ISensors/default with 0 sensors cleanly",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "base_image_bytes": BASE_IMAGE.stat().st_size,
        "kernel_image": str(kernel_path),
        "kernel_image_sha256": kernel_sha,
        "fix_image": str(OUTPUT_IMAGE),
        "fix_image_sha256": output_sha,
        "fix_image_bytes": output_bytes,
        "fix_image_inode": output_inode,
        "elapsed_seconds": round(elapsed, 2),
    }

    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    print(f"[T654 BUILDER] Successfully built {OUTPUT_IMAGE} in {elapsed:.1f}s")
    print(f"[T654 BUILDER] SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")


if __name__ == "__main__":
    main()
