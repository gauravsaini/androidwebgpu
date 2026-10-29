#!/usr/bin/env python3
"""Build one versioned T637 fix initramfs that provides the gralloc mapper for SurfaceFlinger."""

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
TOOLCHAIN = ROOT / "toolchain"
FLIGHT = ROOT / "flights" / "fix-01"
BASE_INITRD = INPUTS / "t628-iallocator-service-initramfs.cpio.gz"
BASE_VENDOR = INPUTS / "t628-vendor-allocator.img"
OUTPUT_INITRD = INPUTS / "t637-gralloc-mapper-fix-initramfs.cpio.gz"
OUTPUT_PARTIAL = OUTPUT_INITRD.with_suffix(".cpio.gz.partial")
MAPPER_SO = TOOLCHAIN / "mapper.default.so"
CAPTURE_SH = TOOLCHAIN / "t637-sf-tombstone-dump.sh"
CAPTURE_RC = TOOLCHAIN / "t637-sf-tombstone-dump.rc"
DEBUGFS = Path("/opt/homebrew/opt/e2fsprogs/sbin/debugfs")
E2FSCK = Path("/opt/homebrew/opt/e2fsprogs/sbin/e2fsck")
EXPECTED_BASE_INITRD = "6b0b0d715b92e97b9a997c60f7c5e06cff81d7178af92d1ed28fc55a95414e17"
EXPECTED_BASE_VENDOR = "e29c56c5efd6f455439c4a99d9e757996af12488df1641bb2e1b7bc581e84206"
EXPECTED_SYSTEM = "1f9d601bfc44640c3680365e9d8b82fd3408f0c1dc6b515f4e47095b9bcb74df"
KERNEL = INPUTS / "Image"
EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
MIN_BUILD_FREE_BYTES = 3 * 1024**3
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


def repack_with_vendor(replacement: Path):
    seen_vendor = False
    with gzip.open(BASE_INITRD, "rb") as source, gzip.open(OUTPUT_PARTIAL, "wb", compresslevel=6) as target:
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
                raise RuntimeError("T628 initramfs has no embedded vendor image")
            if vendor_stream.tell() != replacement.stat().st_size:
                raise RuntimeError("not all patched vendor bytes were embedded")
    with gzip.open(OUTPUT_PARTIAL, "rb") as stream:
        while stream.read(CHUNK):
            pass
    os.replace(OUTPUT_PARTIAL, OUTPUT_INITRD)


def main():
    if OUTPUT_INITRD.exists() or OUTPUT_PARTIAL.exists():
        raise SystemExit(f"refusing to overwrite versioned image: {OUTPUT_INITRD}")
    if sha256(KERNEL) != EXPECTED_KERNEL:
        raise SystemExit("kernel SHA does not match the T628 proving lineage")
    if sha256(BASE_INITRD) != EXPECTED_BASE_INITRD or sha256(BASE_VENDOR) != EXPECTED_BASE_VENDOR:
        raise SystemExit("T628 input SHA mismatch")
    if cpio_member_sha256(BASE_INITRD, "ramdisk-images/system.img") != EXPECTED_SYSTEM:
        raise SystemExit("embedded system image SHA mismatch")
    if cpio_member_sha256(BASE_INITRD, "ramdisk-images/vendor.img") != EXPECTED_BASE_VENDOR:
        raise SystemExit("embedded T628 vendor SHA mismatch")
    if not MAPPER_SO.exists():
        raise SystemExit("mapper.default.so not found; compile toolchain first")

    free_bytes = shutil.disk_usage(INPUTS).free
    FLIGHT.mkdir(parents=True, exist_ok=True)
    (FLIGHT / "build-resources.txt").write_text(
        f"free_bytes_before_build={free_bytes}\nfree_gib_before_build={free_bytes / 1024**3:.3f}\n"
    )
    if free_bytes < MIN_BUILD_FREE_BYTES:
        raise SystemExit(f"disk reserve is below 3,000,000,000 bytes; no image written ({free_bytes})")

    commands_file = FLIGHT / "debugfs-commands.txt"
    commands_file.write_text(
        "\n".join(
            [
                f"write {MAPPER_SO} /lib64/hw/mapper.default.so",
                "sif /lib64/hw/mapper.default.so mode 0100644",
                "sif /lib64/hw/mapper.default.so uid 0",
                "sif /lib64/hw/mapper.default.so gid 0",
                "ea_set /lib64/hw/mapper.default.so security.selinux u:object_r:same_process_hal_file:s0",
                f"write {MAPPER_SO} /lib64/mapper.default.so",
                "sif /lib64/mapper.default.so mode 0100644",
                "sif /lib64/mapper.default.so uid 0",
                "sif /lib64/mapper.default.so gid 0",
                "ea_set /lib64/mapper.default.so security.selinux u:object_r:same_process_hal_file:s0",
                f"write {CAPTURE_SH} /bin/t637-sf-tombstone-dump.sh",
                "sif /bin/t637-sf-tombstone-dump.sh mode 0100755",
                "sif /bin/t637-sf-tombstone-dump.sh uid 0",
                "sif /bin/t637-sf-tombstone-dump.sh gid 0",
                "ea_set /bin/t637-sf-tombstone-dump.sh security.selinux u:object_r:vendor_file:s0",
                f"write {CAPTURE_RC} /etc/init/t637-sf-tombstone-dump.rc",
                "sif /etc/init/t637-sf-tombstone-dump.rc mode 0100644",
                "sif /etc/init/t637-sf-tombstone-dump.rc uid 0",
                "sif /etc/init/t637-sf-tombstone-dump.rc gid 0",
                "ea_set /etc/init/t637-sf-tombstone-dump.rc security.selinux u:object_r:vendor_configs_file:s0",
                "",
            ]
        )
    )

    with tempfile.TemporaryDirectory(prefix="t637-fix-", dir="/private/tmp") as temp_name:
        temp_dir = Path(temp_name)
        vendor = temp_dir / "vendor.img"
        shutil.copyfile(BASE_VENDOR, vendor)

        debugfs = subprocess.run(
            [str(DEBUGFS), "-w", "-f", str(commands_file), str(vendor)],
            capture_output=True,
            text=True,
        )
        (FLIGHT / "debugfs-write.log").write_text(debugfs.stdout + debugfs.stderr)
        if debugfs.returncode:
            raise RuntimeError("debugfs failed; see flights/fix-01/debugfs-write.log")

        fsck = subprocess.run(
            [str(E2FSCK), "-f", "-n", str(vendor)],
            capture_output=True,
            text=True,
        )
        (FLIGHT / "vendor-e2fsck.log").write_text(fsck.stdout + fsck.stderr)
        if fsck.returncode:
            raise RuntimeError("patched vendor failed e2fsck; see flights/fix-01/vendor-e2fsck.log")

        vendor_sha = sha256(vendor)
        repack_with_vendor(vendor)

    system_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/system.img")
    embedded_vendor_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/vendor.img")
    if system_sha != EXPECTED_SYSTEM or embedded_vendor_sha != vendor_sha:
        raise RuntimeError("T637 initramfs embedded image hashes do not match their sources")
    initramfs_sha = sha256(OUTPUT_INITRD)
    manifest = {
        "track": "T637",
        "lineage": "T628 proving image plus T637 mapper.default.so Gralloc 5 Same-Process HAL and tombstone collector",
        "built_utc": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "images": {
            "kernel_Image": {"path": str(KERNEL), "sha256": sha256(KERNEL)},
            "initramfs": {"path": str(OUTPUT_INITRD), "sha256": initramfs_sha},
            "embedded_system_img": {"member": "ramdisk-images/system.img", "sha256": system_sha},
            "embedded_vendor_img": {"member": "ramdisk-images/vendor.img", "sha256": embedded_vendor_sha},
        },
        "mapper_fix": {
            "library_name": "mapper.default.so",
            "installed_paths": [
                "/vendor/lib64/hw/mapper.default.so",
                "/vendor/lib64/mapper.default.so"
            ],
            "mapper_so_sha256": sha256(MAPPER_SO),
            "hal_type": "Gralloc 5 / IMapper 5.0 Stable-C SP-HAL",
            "entrypoint": "AIMapper_loadIMapper",
            "selinux_context": "u:object_r:same_process_hal_file:s0"
        },
        "t628_initramfs_sha256": EXPECTED_BASE_INITRD,
        "t628_vendor_sha256": EXPECTED_BASE_VENDOR,
    }
    (FLIGHT / "guest-image-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
