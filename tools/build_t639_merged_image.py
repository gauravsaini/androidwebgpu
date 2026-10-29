#!/usr/bin/env python3
"""Build exactly ONE merged initramfs for T639: combine T636 classpath fix + T637 gralloc mapper."""

from __future__ import annotations

import datetime as dt
import gzip
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INPUTS = ROOT / "guest-inputs"
GUEST_IMAGE = ROOT / "guest-image"
FLIGHT = ROOT / "flights" / "flight-01"

KERNEL = INPUTS / "Image"
T636_INITRD = INPUTS / "t636-bootclasspath-fixed-initramfs.cpio.gz"
T637_INITRD = INPUTS / "t637-gralloc-mapper-fix-initramfs.cpio.gz"
MAPPER_SO = ROOT / "toolchain" / "mapper.default.so"

OUTPUT_INITRD = GUEST_IMAGE / "t639-merged-initramfs.cpio.gz"
OUTPUT_PARTIAL = OUTPUT_INITRD.with_suffix(".cpio.gz.partial")

DEBUGFS = Path("/opt/homebrew/opt/e2fsprogs/sbin/debugfs")
E2FSCK = Path("/opt/homebrew/opt/e2fsprogs/sbin/e2fsck")

EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_T636_INITRD = "2a279db4a3085df1b523afba28cf6994ab3a51d0921c6879db5a8858217ccec0"
EXPECTED_T637_INITRD = "dbb22d63d47ddc05c73b0d91c05be01801f93f87a778cc200600402554933c9c"
EXPECTED_MAPPER_SO = "9b669318015cb41c5309361f57c23a00ba156417c45e40c5bef310523c4f628b"
EXPECTED_T636_SYSTEM = "8267e3c89b80ae7abd930b2232e5fc763bc5078fedccdea4856a43a43a98e87e"
EXPECTED_T637_VENDOR = "77b5d70336d3137105035eaec3260402ef97d16259e31074922e399611ec9326"

MIN_BUILD_FREE_BYTES = 2 * 1024**3
CHUNK = 8 * 1024 * 1024


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(CHUNK):
            digest.update(block)
    return digest.hexdigest()


def pad4(size: int) -> int:
    return (-size) & 3


def read_entry(stream):
    header = stream.read(110)
    if len(header) != 110 or header[:6] != b"070701":
        raise ValueError("invalid or truncated newc CPIO header")
    namesize = int(header[94:102], 16)
    filesize = int(header[54:62], 16)
    name = stream.read(namesize)
    if len(name) != namesize or not name.endswith(b"\0"):
        raise EOFError("truncated or malformed CPIO name")
    padding = stream.read(pad4(110 + namesize))
    if len(padding) != pad4(110 + namesize):
        raise EOFError("truncated CPIO name padding")
    return header, name, name[:-1].decode("latin1"), padding, filesize


def copy_exact(source, target, size: int, digest=None):
    remaining = size
    while remaining:
        block = source.read(min(CHUNK, remaining))
        if not block:
            raise EOFError(f"truncated CPIO data (wanted {size} bytes)")
        if target is not None:
            target.write(block)
        if digest is not None:
            digest.update(block)
        remaining -= len(block)


def cpio_member_sha256(image: Path, wanted: str) -> str:
    digest = hashlib.sha256()
    found = False
    with gzip.open(image, "rb") as stream:
        while True:
            _, _, name, _, size = read_entry(stream)
            if name == wanted:
                if found:
                    raise RuntimeError(f"duplicate CPIO member: {wanted}")
                copy_exact(stream, None, size, digest)
                found = True
            else:
                copy_exact(stream, None, size)
            padding = stream.read(pad4(size))
            if len(padding) != pad4(size):
                raise EOFError(f"truncated CPIO data padding for {name}")
            if name == "TRAILER!!!":
                break
    if not found:
        raise RuntimeError(f"missing CPIO member: {wanted}")
    return digest.hexdigest()


def extract_cpio_member(image: Path, wanted: str, out_path: Path):
    found = False
    with gzip.open(image, "rb") as stream, out_path.open("wb") as target:
        while True:
            _, _, name, _, size = read_entry(stream)
            if name == wanted:
                if found:
                    raise RuntimeError(f"duplicate CPIO member: {wanted}")
                copy_exact(stream, target, size)
                found = True
            else:
                copy_exact(stream, None, size)
            padding = stream.read(pad4(size))
            if len(padding) != pad4(size):
                raise EOFError(f"truncated CPIO data padding for {name}")
            if name == "TRAILER!!!":
                break
    if not found:
        raise RuntimeError(f"missing CPIO member: {wanted}")


def repack_t636_with_vendor(replacement: Path):
    seen_vendor = False
    with gzip.open(T636_INITRD, "rb") as source, gzip.open(OUTPUT_PARTIAL, "wb", compresslevel=6) as target:
        with replacement.open("rb") as vendor_stream:
            while True:
                header, name_bytes, name, name_padding, size = read_entry(source)
                target.write(header)
                target.write(name_bytes)
                target.write(name_padding)
                if name == "ramdisk-images/vendor.img":
                    if seen_vendor or size != replacement.stat().st_size:
                        raise RuntimeError(f"unexpected vendor member shape: duplicate={seen_vendor}, size={size}")
                    copy_exact(source, None, size)
                    vendor_stream.seek(0)
                    copy_exact(vendor_stream, target, size)
                    padding = source.read(pad4(size))
                    if len(padding) != pad4(size):
                        raise EOFError("truncated vendor CPIO padding")
                    target.write(padding)
                    seen_vendor = True
                else:
                    copy_exact(source, target, size)
                    padding = source.read(pad4(size))
                    if len(padding) != pad4(size):
                        raise EOFError(f"truncated CPIO padding for {name}")
                    target.write(padding)
                if name == "TRAILER!!!":
                    while block := source.read(CHUNK):
                        target.write(block)
                    break
            if not seen_vendor:
                raise RuntimeError("T636 initramfs has no embedded vendor image")
            if vendor_stream.tell() != replacement.stat().st_size:
                raise RuntimeError("not all patched vendor bytes were embedded")
    with gzip.open(OUTPUT_PARTIAL, "rb") as stream:
        while stream.read(CHUNK):
            pass
    os.replace(OUTPUT_PARTIAL, OUTPUT_INITRD)


def main():
    print("=== T639 MERGE: CLASSPATH + MAPPER BUILD ===")
    GUEST_IMAGE.mkdir(parents=True, exist_ok=True)
    FLIGHT.mkdir(parents=True, exist_ok=True)

    # 1. SHA verification of input artifacts
    print("[1] Verifying input artifact SHA-256 hashes...")
    kernel_hash = sha256(KERNEL)
    print(f"Kernel Image:   {kernel_hash}")
    assert kernel_hash == EXPECTED_KERNEL, f"kernel SHA mismatch: {kernel_hash}"

    t636_hash = sha256(T636_INITRD)
    print(f"T636 Initramfs: {t636_hash}")
    assert t636_hash == EXPECTED_T636_INITRD, f"T636 initramfs SHA mismatch: {t636_hash}"

    t637_hash = sha256(T637_INITRD)
    print(f"T637 Initramfs: {t637_hash}")
    assert t637_hash == EXPECTED_T637_INITRD, f"T637 initramfs SHA mismatch: {t637_hash}"

    mapper_hash = sha256(MAPPER_SO)
    print(f"mapper.default: {mapper_hash}")
    assert mapper_hash == EXPECTED_MAPPER_SO, f"mapper.default.so SHA mismatch: {mapper_hash}"

    # 2. Disk hygiene check
    free_bytes = shutil.disk_usage(ROOT).free
    print(f"[2] Free disk bytes: {free_bytes} ({free_bytes / 1024**3:.2f} GiB)")
    if free_bytes < MIN_BUILD_FREE_BYTES:
        raise SystemExit(f"Disk space below {MIN_BUILD_FREE_BYTES} bytes! Aborting build.")

    # 3. Extract vendor.img from T637 fix image
    with tempfile.TemporaryDirectory(prefix="t639-merge-", dir="/private/tmp") as tmpdir:
        tmp_vendor = Path(tmpdir) / "vendor.img"
        print("[3] Extracting patched vendor.img from T637...")
        extract_cpio_member(T637_INITRD, "ramdisk-images/vendor.img", tmp_vendor)
        vendor_hash = sha256(tmp_vendor)
        print(f"Extracted vendor.img SHA-256: {vendor_hash}")
        assert vendor_hash == EXPECTED_T637_VENDOR, f"extracted vendor SHA mismatch: {vendor_hash}"

        # 4. Verify mapper.default.so inside vendor.img
        print("[4] Verifying mapper.default.so inside extracted vendor.img using debugfs...")
        extracted_so = Path(tmpdir) / "dumped_mapper.so"
        dump_cmd = [str(DEBUGFS), "-R", f"dump /lib64/hw/mapper.default.so {extracted_so}", str(tmp_vendor)]
        res = subprocess.run(dump_cmd, capture_output=True, text=True, check=True)
        dumped_so_hash = sha256(extracted_so)
        print(f"Dumped mapper.default.so SHA-256: {dumped_so_hash}")
        assert dumped_so_hash == EXPECTED_MAPPER_SO, f"dumped mapper SHA mismatch: {dumped_so_hash}"

        # 5. Verify SELinux label on mapper.default.so
        ea_cmd = [str(DEBUGFS), "-R", "ea_get /lib64/hw/mapper.default.so security.selinux", str(tmp_vendor)]
        res_ea = subprocess.run(ea_cmd, capture_output=True, text=True, check=True)
        print(f"SELinux EA check: {res_ea.stdout.strip()}")
        assert "u:object_r:same_process_hal_file:s0" in res_ea.stdout, "SELinux label mismatch!"

        # 6. Repack T636 with this patched vendor.img
        print(f"[6] Repacking T636 with patched vendor into {OUTPUT_INITRD}...")
        repack_t636_with_vendor(tmp_vendor)

    # 7. Verify merged initramfs
    print("[7] Verifying merged initramfs integrity and lineage...")
    merged_sha = sha256(OUTPUT_INITRD)
    print(f"Merged initramfs SHA-256: {merged_sha}")

    embedded_system_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/system.img")
    print(f"Merged embedded system.img SHA-256: {embedded_system_sha}")
    assert embedded_system_sha == EXPECTED_T636_SYSTEM, f"embedded system SHA mismatch: {embedded_system_sha}"

    embedded_vendor_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/vendor.img")
    print(f"Merged embedded vendor.img SHA-256: {embedded_vendor_sha}")
    assert embedded_vendor_sha == EXPECTED_T637_VENDOR, f"embedded vendor SHA mismatch: {embedded_vendor_sha}"

    # Verify diagnostic script present
    diag_sha = cpio_member_sha256(OUTPUT_INITRD, "t636-diag.sh")
    print(f"Merged t636-diag.sh SHA-256:        {diag_sha}")

    manifest = {
        "track": "T639",
        "description": "Merged T636 classpath fix + T637 Gralloc 5 mapper.default.so fix initramfs",
        "built_utc": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "images": {
            "kernel_Image": {"path": str(KERNEL), "sha256": kernel_hash},
            "t636_base_initramfs": {"path": str(T636_INITRD), "sha256": t636_hash},
            "t637_source_initramfs": {"path": str(T637_INITRD), "sha256": t637_hash},
            "merged_initramfs": {"path": str(OUTPUT_INITRD), "sha256": merged_sha},
            "embedded_system_img": {"member": "ramdisk-images/system.img", "sha256": embedded_system_sha},
            "embedded_vendor_img": {"member": "ramdisk-images/vendor.img", "sha256": embedded_vendor_sha},
            "embedded_t636_diag": {"member": "t636-diag.sh", "sha256": diag_sha},
        },
        "mapper": {
            "path_in_vendor": "/vendor/lib64/hw/mapper.default.so",
            "sha256": mapper_hash,
            "selinux_context": "u:object_r:same_process_hal_file:s0"
        },
        "classpath": {
            "jar": "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar",
            "target": "BOOTCLASSPATH 20 jars"
        }
    }
    manifest_path = FLIGHT / "guest-image-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"[8] Manifest written to {manifest_path}:")
    print(json.dumps(manifest, indent=2))
    print("=== T639 MERGED IMAGE BUILD COMPLETE ===")


if __name__ == "__main__":
    main()
