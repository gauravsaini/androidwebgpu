#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T658:
system-server-startotherservices-connectivityservice-noresourcepackage (Chain A)

CLASS FIX Directive:
Inventory EVERY /system/apex/*.capex inside system.img and replace ALL of them
with their authentic uncompressed original_apex (.apex), exactly as T657 did
for com.android.wifi, so apexd mounts every payload directly with no decompression
step on tmpfs /data.

Outputs:
  guest-image/t658-system-server-startotherservices-connectivityservice-noresourcepackage-initramfs.cpio.gz
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import zipfile
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track658-system-server-startotherservices-connectivityservice-noresourcepackage")
BASE_IMAGE = ROOT / "guest-inputs/t657-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t658-system-server-startotherservices-connectivityservice-noresourcepackage-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t658-manifest.json"
INVENTORY_FILE = ROOT / "evidence/converted_apex_inventory.json"

EXPECTED_BASE_SHA256 = "e19481445ae86bb334d63502c42070d7f0e8637398458aeb19925852db4e4a70"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

CHUNK = 8 * 1024 * 1024
DEBUGFS = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/debugfs"
E2FSCK = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/e2fsck"
RESIZE2FS = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/resize2fs"


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


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    anchor = '    mark "T657_FIX_APPLIED"\n'
    t658_check = """    # T658: Class fix verification - verify all capex converted to apex and tethering apex
    mark "T658_CLASS_FIX_PREP"
    for pkg in com.android.tethering com.android.permission com.android.adservices com.android.appsearch com.android.media com.android.extservices; do
        "$TOY" mkdir -p "/data/misc/apexdata/$pkg" 2>/dev/null || :
        "$TOY" chown -R 1000:1000 "/data/misc/apexdata/$pkg" 2>/dev/null || :
        "$TOY" chmod 0771 "/data/misc/apexdata/$pkg" 2>/dev/null || :
    done
    if [ -f /system/apex/com.android.tethering.apex ]; then
        mark "T658_SYSTEM_APEX_TETHERING_EXISTS"
    else
        mark "T658_SYSTEM_APEX_TETHERING_MISSING"
    fi
    if [ -d /apex/com.android.tethering/priv-app ]; then
        mark "T658_TETHERING_PRIVAPP_DIR_EXISTS"
    else
        mark "T658_TETHERING_PRIVAPP_DIR_PENDING"
    fi
    mark "T658_FIX_APPLIED"
"""
    if anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T657_FIX_APPLIED\"' in base diag script")
    script = script.replace(anchor, anchor + "\n" + t658_check, 1)

    boot_anchor = (
        '        wifi_apex_status="missing"\n'
        '        if [ -d /apex/com.android.wifi/priv-app ]; then\n'
        '            wifi_apex_status="ok"\n'
        '        fi\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        mark "T642_BOOT_STATE uptime=$uptime system_server_prop=$ss system_server_pids=$sspids zygote=$zy zypid=$zypid sensors_hal=$sensors_hal netd=$netd_svc netstats=$netstats_status wifi_apex=$wifi_apex_status boot_completed=$completed"'
    )
    boot_replace = (
        '        wifi_apex_status="missing"\n'
        '        if [ -d /apex/com.android.wifi/priv-app ]; then\n'
        '            wifi_apex_status="ok"\n'
        '        fi\n'
        '        tethering_apex_status="missing"\n'
        '        if [ -d /apex/com.android.tethering/priv-app ]; then\n'
        '            tethering_apex_status="ok"\n'
        '        fi\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        mark "T642_BOOT_STATE uptime=$uptime system_server_prop=$ss system_server_pids=$sspids zygote=$zy zypid=$zypid sensors_hal=$sensors_hal netd=$netd_svc netstats=$netstats_status wifi_apex=$wifi_apex_status tethering_apex=$tethering_apex_status boot_completed=$completed"'
    )
    if boot_anchor in script:
        script = script.replace(boot_anchor, boot_replace, 1)
    else:
        print("[T658 BUILDER] Warning: boot_anchor not found for exact replacement")

    return script.encode("utf-8")


def extract_system_img_from_base(dest_path: Path) -> int:
    print(f"[T658 BUILDER] Extracting ramdisk-images/system.img from {BASE_IMAGE} to {dest_path}...")
    with gzip.open(BASE_IMAGE, "rb") as fin:
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

            if name_str == "ramdisk-images/system.img":
                with open(dest_path, "wb") as fout:
                    rem = filesize
                    while rem > 0:
                        chunk = fin.read(min(rem, CHUNK))
                        fout.write(chunk)
                        rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                print(f"[T658 BUILDER] Extracted system.img ({filesize} bytes).")
                return filesize
            else:
                rem = filesize
                while rem > 0:
                    fin.read(min(rem, CHUNK))
                    rem -= min(rem, CHUNK)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
    raise RuntimeError("ramdisk-images/system.img not found in base image")


def run_e2fsck(img_path: Path):
    res = subprocess.run([E2FSCK, "-f", "-y", str(img_path)], capture_output=True, text=True)
    if res.returncode not in (0, 1, 2):
        raise RuntimeError(f"e2fsck failed with returncode {res.returncode}:\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}")
    print(f"[T658 BUILDER] e2fsck clean (returncode {res.returncode})")


def prepare_class_fix_system_img(scratch_dir: Path) -> tuple[Path, list[dict], int, int]:
    system_img_path = scratch_dir / "system.img"
    capex_dump_dir = scratch_dir / "capex"
    apex_extract_dir = scratch_dir / "apex"
    capex_dump_dir.mkdir(parents=True, exist_ok=True)
    apex_extract_dir.mkdir(parents=True, exist_ok=True)

    if not system_img_path.exists():
        extract_system_img_from_base(system_img_path)
    old_system_img_size = system_img_path.stat().st_size
    print(f"[T658 BUILDER] Old system.img size: {old_system_img_size} bytes")

    # 1. Inventory all .capex files in system/apex
    res = subprocess.run([DEBUGFS, "-R", "ls -l system/apex", str(system_img_path)], capture_output=True, text=True, check=True)
    capex_list = []
    for line in res.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 8 and parts[-1].endswith(".capex"):
            ino = parts[0]
            size = int(parts[5])
            name = parts[-1]
            capex_list.append((ino, size, name))

    print(f"[T658 BUILDER] Found {len(capex_list)} .capex files in system.img:")
    inventory = []
    total_net_growth = 0

    for ino, capex_size, name in capex_list:
        pkg_base = name[:-len(".capex")]
        apex_name = f"{pkg_base}.apex"
        dumped_capex = capex_dump_dir / name
        extracted_apex = apex_extract_dir / apex_name

        # Dump capex from system.img
        subprocess.run([DEBUGFS, "-R", f"dump system/apex/{name} {dumped_capex}", str(system_img_path)], capture_output=True, check=True)

        # Extract original_apex from capex
        has_original_apex = False
        apex_size = 0
        with zipfile.ZipFile(dumped_capex, "r") as z:
            if "original_apex" in z.namelist():
                has_original_apex = True
                with z.open("original_apex") as src, open(extracted_apex, "wb") as dst:
                    shutil.copyfileobj(src, dst)
                apex_size = extracted_apex.stat().st_size

        if not has_original_apex:
            print(f"  [EXCEPTION] {name}: NO extractable original_apex! Leaving unchanged per directive.")
            inventory.append({
                "name": name,
                "capex_size": capex_size,
                "apex_size": None,
                "converted": False,
                "note": "no extractable original_apex"
            })
            continue

        net_delta = apex_size - capex_size
        total_net_growth += net_delta
        print(f"  [CONVERT] {name} ({capex_size} B) -> {apex_name} ({apex_size} B, delta: +{net_delta} B)")
        inventory.append({
            "name": name,
            "apex_name": apex_name,
            "capex_size": capex_size,
            "apex_size": apex_size,
            "net_delta": net_delta,
            "converted": True,
            "extracted_path": str(extracted_apex),
        })

    # Save inventory
    with open(INVENTORY_FILE, "w") as f:
        json.dump(inventory, f, indent=2)
    print(f"[T658 BUILDER] Saved converted APEX inventory to {INVENTORY_FILE}")
    print(f"[T658 BUILDER] Total net growth needed: {total_net_growth} bytes ({total_net_growth / 1024 / 1024:.2f} MB)")

    # 2. Expand system.img to accommodate net growth
    # Let's add 256 MiB (268,435,456 bytes)
    extra_bytes = 256 * 1024 * 1024
    new_system_img_size = old_system_img_size + extra_bytes
    print(f"[T658 BUILDER] Resizing system.img: {old_system_img_size} -> {new_system_img_size} (+256 MiB)...")
    with open(system_img_path, "ab") as f:
        f.truncate(new_system_img_size)

    run_e2fsck(system_img_path)
    subprocess.run([RESIZE2FS, str(system_img_path)], capture_output=True, check=True)
    run_e2fsck(system_img_path)
    print(f"[T658 BUILDER] Filesystem resized and verified clean.")

    # 3. Replace all converted .capex files with authentic .apex and set security.selinux
    print(f"[T658 BUILDER] Applying replacements into system.img via debugfs...")
    for item in inventory:
        if not item.get("converted"):
            continue
        capex_name = item["name"]
        apex_name = item["apex_name"]
        apex_path = item["extracted_path"]

        # Remove capex
        subprocess.run([DEBUGFS, "-w", "-R", f"rm system/apex/{capex_name}", str(system_img_path)], capture_output=True, check=True)
        # Write authentic uncompressed apex
        subprocess.run([DEBUGFS, "-w", "-R", f"write {apex_path} system/apex/{apex_name}", str(system_img_path)], capture_output=True, check=True)
        # Set security.selinux xattr
        subprocess.run([DEBUGFS, "-w", "-R", f"ea_set system/apex/{apex_name} security.selinux u:object_r:system_file:s0", str(system_img_path)], capture_output=True, check=True)

    # 4. Final verification of system/apex in system.img
    run_e2fsck(system_img_path)
    verify_res = subprocess.run([DEBUGFS, "-R", "ls -l system/apex", str(system_img_path)], capture_output=True, text=True, check=True)
    remaining_capex = [line for line in verify_res.stdout.splitlines() if line.strip().endswith(".capex")]
    if remaining_capex:
        raise RuntimeError(f"Remaining .capex found after conversion: {remaining_capex}")
    print("[T658 BUILDER] Verified: ZERO .capex files remain in system/apex. All converted to .apex!")

    final_size = system_img_path.stat().st_size
    return system_img_path, inventory, old_system_img_size, final_size


def main():
    t0 = time.monotonic()
    print("[T658 BUILDER] Starting T658 class-fix initramfs build...")

    free_gib = check_disk_space()
    print(f"[T658 BUILDER] Disk check passed: {free_gib:.2f} GiB free on /System/Volumes/Data.")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T658 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T658 BUILDER] Verified T657 base image SHA: {base_sha}")

    if BASE_IMAGE.stat().st_nlink <= 1:
        sys.exit(f"Base image {BASE_IMAGE} must be hardlinked (st_nlink > 1), got {BASE_IMAGE.stat().st_nlink}")
    print(f"[T658 BUILDER] Base image hardlink verified: link count = {BASE_IMAGE.stat().st_nlink}")

    scratch_dir = ROOT / "scratch"
    scratch_dir.mkdir(parents=True, exist_ok=True)
    system_img_path, inventory, old_system_size, new_system_size = prepare_class_fix_system_img(scratch_dir)

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T658 BUILDER] Streaming CPIO archive from {BASE_IMAGE} to build ONE fix initramfs...")
    diag_patched = False
    system_patched = False

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

            if name_str == "t636-diag.sh":
                print(f"[T658 BUILDER] Reading and updating t636-diag.sh ({filesize} bytes)...")
                base_diag_bytes = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_diag_bytes = build_diag_script(base_diag_bytes.decode("latin1"))
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
                fout.write(new_entry)
                diag_patched = True
                print(f"[T658 BUILDER] Replaced t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)")

            elif name_str == "ramdisk-images/system.img":
                print(f"[T658 BUILDER] Replacing ramdisk-images/system.img ({filesize} bytes) with converted system.img ({new_system_size} bytes)...")
                # Skip old system.img data in input
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                # Write new system.img from scratch/system.img
                ino = int(hdr[6:14], 16)
                mode = int(hdr[14:22], 16)
                name_b = name_bytes
                sys_hdr = (
                    f"070701"
                    f"{ino:08x}"
                    f"{mode:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{1:08x}"
                    f"{0:08x}"
                    f"{new_system_size:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{namesize:08x}"
                    f"{0:08x}"
                ).encode("ascii")
                fout.write(sys_hdr)
                fout.write(name_b)
                pad_hdr = pad4(len(sys_hdr) + len(name_b))
                if pad_hdr:
                    fout.write(b"\0" * pad_hdr)

                with open(system_img_path, "rb") as fsys:
                    while True:
                        chunk = fsys.read(CHUNK)
                        if not chunk:
                            break
                        fout.write(chunk)

                pad_sys_data = pad4(new_system_size)
                if pad_sys_data:
                    fout.write(b"\0" * pad_sys_data)
                system_patched = True
                print(f"[T658 BUILDER] Replaced ramdisk-images/system.img with {new_system_size} bytes")

            else:
                # Copy verbatim
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

            if name_str == "TRAILER!!!":
                break

    if not diag_patched:
        raise RuntimeError("Failed to patch t636-diag.sh!")
    if not system_patched:
        raise RuntimeError("Failed to patch ramdisk-images/system.img!")

    if OUTPUT_IMAGE.exists():
        OUTPUT_IMAGE.unlink()
    PARTIAL_IMAGE.rename(OUTPUT_IMAGE)

    old_initramfs_size = BASE_IMAGE.stat().st_size
    new_initramfs_size = OUTPUT_IMAGE.stat().st_size
    fix_sha = sha256_file(OUTPUT_IMAGE)
    elapsed = time.monotonic() - t0
    print(f"[T658 BUILDER] ONE fix initramfs created successfully in {elapsed:.1f}s:")
    print(f"  Path: {OUTPUT_IMAGE}")
    print(f"  SHA-256: {fix_sha}")
    print(f"  Old initramfs size: {old_initramfs_size} bytes")
    print(f"  New initramfs size: {new_initramfs_size} bytes")
    print(f"  Old system.img size: {old_system_size} bytes")
    print(f"  New system.img size: {new_system_size} bytes")

    manifest = {
        "track": "T658",
        "created_epoch": int(time.time()),
        "kernel_path": str(kernel_path),
        "kernel_sha256": kernel_sha,
        "base_image_path": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "fix_image_path": str(OUTPUT_IMAGE),
        "fix_image_sha256": fix_sha,
        "old_initramfs_size": old_initramfs_size,
        "new_initramfs_size": new_initramfs_size,
        "old_system_img_size": old_system_size,
        "new_system_img_size": new_system_size,
        "converted_apex_count": len([x for x in inventory if x.get("converted")]),
        "inventory": inventory,
    }
    with open(MANIFEST_FILE, "w") as f:
        json.dump(manifest, f, indent=2)
    print(f"[T658 BUILDER] Saved manifest to {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
