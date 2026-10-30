#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T644: give system_server the linker namespaces its flattened APEXes need."""

from __future__ import annotations

import base64
import gzip
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track644-system-server-apex-namespace")
BASE_IMAGE = ROOT / "guest-inputs/t642-system-server-fd-allowlist-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t644-system-server-apex-namespace-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t644-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "62c259446f8869bf1ed6fc8b46609ff75cfbdab3ae81b94e50960c85e6adc99a"
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

def build_diag_script() -> bytes:
    sys.path.insert(0, str(ROOT))
    from scripts.generate_linker_config import generate_ld_config, generate_apex_info_list

    ld_config_txt = generate_ld_config()
    apex_info_xml = generate_apex_info_list()

    b64_ld_config = base64.b64encode(ld_config_txt.encode("utf-8")).decode("ascii")
    b64_apex_info = base64.b64encode(apex_info_xml.encode("utf-8")).decode("ascii")

    # Read base diag script
    base_diag_path = ROOT / "scripts/diag_script_content.sh"
    with open(base_diag_path, "r") as f:
        base_script = f.read()

    # Create the T644 prep additions
    t644_prep_block = f"""
    # 5. Generate /linkerconfig/ld.config.txt with all APEX namespaces
    mark "T644_GENERATING_LINKER_CONFIG"
    "$TOY" mkdir -p /linkerconfig/bootstrap /linkerconfig/default
    echo -n "{b64_ld_config}" | "$TOY" base64 -d > /linkerconfig/ld.config.txt
    "$TOY" cp /linkerconfig/ld.config.txt /linkerconfig/bootstrap/ld.config.txt
    "$TOY" cp /linkerconfig/ld.config.txt /linkerconfig/default/ld.config.txt
    "$TOY" cp /linkerconfig/ld.config.txt /ld.config.txt 2>/dev/null || :
    "$TOY" chmod 0644 /linkerconfig/ld.config.txt /linkerconfig/bootstrap/ld.config.txt /linkerconfig/default/ld.config.txt /ld.config.txt 2>/dev/null || :

    # 6. Generate /apex/apex-info-list.xml with all active APEXes
    mark "T644_GENERATING_APEX_INFO_LIST"
    echo -n "{b64_apex_info}" | "$TOY" base64 -d > /apex/apex-info-list.xml
    "$TOY" chmod 0644 /apex/apex-info-list.xml

    # 7. Verify linker config contains com_android_tethering
    if "$TOY" grep -q "com_android_tethering" /linkerconfig/ld.config.txt; then
        cfg_size=$("$TOY" wc -c < /linkerconfig/ld.config.txt 2>/dev/null)
        mark "T644_LINKER_CONFIG_VERIFIED_OK size=$cfg_size"
    else
        mark "T644_LINKER_CONFIG_VERIFICATION_FAILED"
    fi
"""

    # Insert before mark "T642_DO_PREP_EXIT pid=$$"
    anchor = '    mark "T642_DO_PREP_EXIT pid=$$"'
    if anchor not in base_script:
        raise ValueError("Could not find anchor in diag_script_content.sh")

    new_script = base_script.replace(anchor, t644_prep_block + "\n" + anchor)
    return new_script.encode("utf-8")

def main():
    t0 = time.monotonic()
    print("[T644 BUILDER] Starting T644 initramfs build...")

    # 1. Verify inputs
    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T644 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T644 BUILDER] Verified T642 base image SHA: {base_sha}")

    # 2. Build updated diagnostic script
    new_diag_content = build_diag_script()
    print(f"[T644 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_content)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    # 3. Stream CPIO archive and replace t636-diag.sh
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
                print(f"[T644 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_content)} bytes)...")
                # Skip original content and padding
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                # Format new entry
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_content)
                fout.write(new_entry)
            else:
                # Copy verbatim
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
        "track": "T644",
        "description": "Give system_server the linker namespaces its flattened APEXes need (ld.config.txt + apex-info-list.xml)",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": output_bytes,
        "build_elapsed_seconds": round(elapsed, 2),
        "namespace_mechanism": "Direct generation of /linkerconfig/ld.config.txt with visible=true APEX namespaces and /apex/apex-info-list.xml metadata during prep phase",
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
            "base_t642_initramfs": {
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

    print(f"[T644 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T644 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T644 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes)")
    print(f"[T644 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")

if __name__ == "__main__":
    main()
