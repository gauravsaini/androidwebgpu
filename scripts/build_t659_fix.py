#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T659:
system-server-startotherservices-audioservice-audiopolicy (Chain A)

PINNED MECHANISM:
In T646, t636-diag.sh bind-mounted /data/manifest.xml over /vendor/etc/vintf/manifest.xml
to register the Widevine DRM HAL, but that partial manifest omitted android.hardware.audio.effect.
Consequently, vendor.audio-hal (/vendor/bin/hw/android.hardware.audio.service) crashed SIGABRT
on early boot and every subsequent cycle trying to register
android.hardware.audio.effect@7.0::IEffectsFactory/default.
Without vendor.audio-hal, audioserver never published media.audio_policy, causing
SystemServer main thread to block in AudioProductStrategy.native_list_audio_product_strategies()
inside StartAudioService until Watchdog killed the process at 88s.

FIX:
Supply the missing android.hardware.audio.effect@7.0 HIDL declaration in
vendor/etc/vintf/manifest.xml and t636-diag.sh /data/manifest.xml bind-mount, unblocking
vendor.audio-hal registration, audioserver initialization, and media.audio_policy publishing.

Base Image (T658): SHA-256 220fcef8fb0536d48f818a04ff0ccecadabaaf8e03a93cf1d0554dfd36e2223a
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Outputs:
  guest-image/t659-system-server-startotherservices-audioservice-audiopolicy-initramfs.cpio.gz
"""

from __future__ import annotations

import base64
import gzip
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track659-system-server-startotherservices-audioservice-audiopolicy")
BASE_IMAGE = ROOT / "guest-inputs/t658-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t659-system-server-startotherservices-audioservice-audiopolicy-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t659-manifest.json"

EXPECTED_BASE_SHA256 = "220fcef8fb0536d48f818a04ff0ccecadabaaf8e03a93cf1d0554dfd36e2223a"
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
    <hal format="hidl">
        <name>android.hardware.audio.effect</name>
        <transport>hwbinder</transport>
        <version>7.0</version>
        <interface>
            <name>IEffectsFactory</name>
            <instance>default</instance>
        </interface>
        <fqname>@7.0::IEffectsFactory/default</fqname>
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


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def patch_diag_script(base_script: str) -> bytes:
    script = base_script

    # 1. Update VINTF manifest base64 string
    new_b64 = base64.b64encode(VINTF_MANIFEST_XML.encode("utf-8")).decode("ascii")

    old_vintf_anchor_start = "    # T646: Update VINTF manifest with Widevine DRM HAL"
    old_vintf_anchor_end = "    # T646: Disable printk ratelimiting"
    idx1 = script.find(old_vintf_anchor_start)
    idx2 = script.find(old_vintf_anchor_end)
    if idx1 == -1 or idx2 == -1:
        raise ValueError("Could not find T646 VINTF manifest block in t636-diag.sh")

    new_vintf_block = f"""    # T646/T659: Update VINTF manifest with Widevine DRM HAL and Audio Effect HAL
    mark "T646_UPDATING_VINTF_MANIFEST"
    mark "T659_UPDATING_VINTF_MANIFEST"
    "$TOY" mkdir -p /vendor/etc/vintf
    echo -n "{new_b64}" | "$TOY" base64 -d > /data/manifest.xml
    "$TOY" cp /data/manifest.xml /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    "$TOY" mount -o bind /data/manifest.xml /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    "$TOY" chmod 0644 /vendor/etc/vintf/manifest.xml 2>/dev/null || :
    if "$TOY" grep -q "widevine" /vendor/etc/vintf/manifest.xml && "$TOY" grep -q "audio.effect" /vendor/etc/vintf/manifest.xml; then
        mark "T646_VINTF_MANIFEST_VERIFIED_OK"
        mark "T659_VINTF_MANIFEST_VERIFIED_OK"
    else
        mark "T646_VINTF_MANIFEST_VERIFICATION_FAILED"
    fi

"""
    script = script[:idx1] + new_vintf_block + script[idx2:]

    # 2. Add T659 verification mark
    t658_applied_anchor = '    mark "T658_FIX_APPLIED"\n'
    t659_check = """    # T659: Audio HAL & VINTF effect manifest verification
    mark "T659_AUDIO_POLICY_PREP"
    if "$TOY" grep -q "android.hardware.audio.effect" /vendor/etc/vintf/manifest.xml; then
        mark "T659_AUDIO_EFFECT_VINTF_PRESENT"
    else
        mark "T659_AUDIO_EFFECT_VINTF_MISSING"
    fi
    mark "T659_FIX_APPLIED"
"""
    if t658_applied_anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T658_FIX_APPLIED\"' in base diag script")
    script = script.replace(t658_applied_anchor, t658_applied_anchor + "\n" + t659_check, 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T659 BUILDER] Starting build for Track T659...")
    free_gib = check_disk_space()
    print(f"[T659 BUILDER] Disk check: {free_gib:.2f} GiB free.")

    # 1. Verify inputs
    if not BASE_IMAGE.exists():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")

    base_sha = sha256_file(BASE_IMAGE)
    kernel_sha = sha256_file(kernel_path)
    print(f"[T659 BUILDER] Base image SHA-256: {base_sha}")
    print(f"[T659 BUILDER] Kernel SHA-256:     {kernel_sha}")

    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")

    # Check hardlink count
    base_stat = os.stat(BASE_IMAGE)
    print(f"[T659 BUILDER] Base image link count: {base_stat.st_nlink} (inode {base_stat.st_ino})")
    if base_stat.st_nlink < 2:
        print("[T659 BUILDER] Warning: Base image link count is < 2")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    # Extract original t636-diag.sh from BASE_IMAGE
    print("[T659 BUILDER] Extracting t636-diag.sh from BASE_IMAGE...")
    base_diag_bytes = None
    with gzip.open(BASE_IMAGE, "rb") as fin:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if hdr[:6] != b"070701":
                raise ValueError(f"Invalid CPIO header magic: {hdr[:6]}")
            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name = fin.read(namesize)[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)
            if name == "t636-diag.sh":
                base_diag_bytes = fin.read(filesize)
                break
            else:
                pad_data = pad4(filesize)
                fin.seek(filesize + pad_data, 1)

    if not base_diag_bytes:
        raise RuntimeError("t636-diag.sh not found in base image")

    patched_diag_bytes = patch_diag_script(base_diag_bytes.decode("utf-8"))
    print(f"[T659 BUILDER] Patched t636-diag.sh ({len(base_diag_bytes)} -> {len(patched_diag_bytes)} bytes).")

    vintf_manifest_bytes = VINTF_MANIFEST_XML.encode("utf-8")
    print(f"[T659 BUILDER] Prepared updated vendor/etc/vintf/manifest.xml ({len(vintf_manifest_bytes)} bytes).")

    # Stream CPIO archive, replacing t636-diag.sh and vendor/etc/vintf/manifest.xml
    print(f"[T659 BUILDER] Streaming CPIO archive from {BASE_IMAGE} to build ONE fix initramfs...")
    diag_patched = False
    manifest_patched = False

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
            name = name_bytes[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name == "t636-diag.sh":
                print(f"[T659 BUILDER] Writing patched t636-diag.sh ({len(patched_diag_bytes)} bytes)...")
                new_entry = format_cpio_entry("t636-diag.sh", patched_diag_bytes, mode=0o100755)
                fout.write(new_entry)
                diag_patched = True
                # Skip old data
                fin.seek(filesize + pad4(filesize), 1)
            elif name == "vendor/etc/vintf/manifest.xml":
                print(f"[T659 BUILDER] Writing patched vendor/etc/vintf/manifest.xml ({len(vintf_manifest_bytes)} bytes)...")
                new_entry = format_cpio_entry("vendor/etc/vintf/manifest.xml", vintf_manifest_bytes, mode=0o100644)
                fout.write(new_entry)
                manifest_patched = True
                # Skip old data
                fin.seek(filesize + pad4(filesize), 1)
            else:
                # Verbatim passthrough
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
                    fout.write(fin.read(pad_data))

            if name == "TRAILER!!!":
                break

    if not diag_patched:
        raise RuntimeError("Failed to patch t636-diag.sh in CPIO stream!")
    if not manifest_patched:
        raise RuntimeError("Failed to patch vendor/etc/vintf/manifest.xml in CPIO stream!")

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha256 = sha256_file(OUTPUT_IMAGE)
    output_size = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0

    print(f"[T659 BUILDER] Fix initramfs built in {elapsed:.1f}s:")
    print(f"  Path:   {OUTPUT_IMAGE}")
    print(f"  SHA256: {output_sha256}")
    print(f"  Bytes:  {output_size}")

    manifest = {
        "track": "T659",
        "created_epoch": int(time.time()),
        "kernel_path": str(kernel_path),
        "kernel_sha256": kernel_sha,
        "base_image_path": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "fix_image_path": str(OUTPUT_IMAGE),
        "fix_image_sha256": output_sha256,
        "fix_image_size": output_size,
        "build_elapsed_seconds": round(elapsed, 2),
        "vintf_manifest_xml": VINTF_MANIFEST_XML,
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    print(f"[T659 BUILDER] Saved manifest to {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
