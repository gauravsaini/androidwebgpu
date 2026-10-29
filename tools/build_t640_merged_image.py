#!/usr/bin/env python3
"""Build exactly ONE merged initramfs for T640: combine T636 classpath + T637 mapper + T638 SwiftShader."""

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
T639_INITRD = INPUTS / "t639-merged-initramfs.cpio.gz"
PATCHED_SWIFTSHADER_SO = ROOT / "toolchain" / "libGLES_swiftshader_patched.so"
OUTPUT_INITRD = GUEST_IMAGE / "t640-three-fix-merge-initramfs.cpio.gz"
OUTPUT_PARTIAL = OUTPUT_INITRD.with_suffix(".cpio.gz.partial")

DEBUGFS = Path("/opt/homebrew/opt/e2fsprogs/sbin/debugfs")
E2FSCK = Path("/opt/homebrew/opt/e2fsprogs/sbin/e2fsck")

EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_T639_INITRD = "c80fb11251cdce4649636cb312b1b6d447980ac71dcf49fe11c73fce9d2da5c3"
EXPECTED_SWIFTSHADER_SO = "bd57955669865747618d4c532bf17e99e23b474dcb4a9239f9817cbefd63319a"
EXPECTED_MAPPER_SO = "9b669318015cb41c5309361f57c23a00ba156417c45e40c5bef310523c4f628b"
EXPECTED_SYSTEM_IMG = "8267e3c89b80ae7abd930b2232e5fc763bc5078fedccdea4856a43a43a98e87e"

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


def repack_with_vendor(source_initrd: Path, target_initrd: Path, target_partial: Path, replacement_vendor: Path):
    seen_vendor = False
    with gzip.open(source_initrd, "rb") as source, gzip.open(target_partial, "wb", compresslevel=6) as target:
        with replacement_vendor.open("rb") as vendor_stream:
            while True:
                header, name_bytes, name, name_padding, size = read_entry(source)
                target.write(header)
                target.write(name_bytes)
                target.write(name_padding)
                if name == "ramdisk-images/vendor.img":
                    if seen_vendor or size != replacement_vendor.stat().st_size:
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
                raise RuntimeError("source initramfs has no embedded vendor image")
            if vendor_stream.tell() != replacement_vendor.stat().st_size:
                raise RuntimeError("not all patched vendor bytes were embedded")
    with gzip.open(target_partial, "rb") as stream:
        while stream.read(CHUNK):
            pass
    os.replace(target_partial, target_initrd)


def main():
    print("=== T640 MERGE: CLASSPATH + MAPPER + SWIFTSHADER BUILD ===")
    GUEST_IMAGE.mkdir(parents=True, exist_ok=True)
    FLIGHT.mkdir(parents=True, exist_ok=True)

    # 1. SHA verification of input artifacts
    print("[1] Verifying input artifact SHA-256 hashes...")
    kernel_hash = sha256(KERNEL)
    print(f"Kernel Image:        {kernel_hash}")
    assert kernel_hash == EXPECTED_KERNEL, f"kernel SHA mismatch: {kernel_hash}"

    t639_hash = sha256(T639_INITRD)
    print(f"T639 Initramfs:      {t639_hash}")
    assert t639_hash == EXPECTED_T639_INITRD, f"T639 initramfs SHA mismatch: {t639_hash}"

    swiftshader_hash = sha256(PATCHED_SWIFTSHADER_SO)
    print(f"Patched SwiftShader: {swiftshader_hash}")
    assert swiftshader_hash == EXPECTED_SWIFTSHADER_SO, f"swiftshader SHA mismatch: {swiftshader_hash}"

    # 2. Disk hygiene check
    free_bytes = shutil.disk_usage(ROOT).free
    print(f"[2] Free disk bytes: {free_bytes} ({free_bytes / 1024**3:.2f} GiB)")
    if free_bytes < MIN_BUILD_FREE_BYTES:
        raise SystemExit(f"Disk space below {MIN_BUILD_FREE_BYTES} bytes! Aborting build.")

    # 3. Extract vendor.img from T639 merged image
    with tempfile.TemporaryDirectory(prefix="t640-merge-", dir="/private/tmp") as tmpdir:
        tmp_vendor = Path(tmpdir) / "vendor.img"
        print("[3] Extracting vendor.img from T639 merged image...")
        extract_cpio_member(T639_INITRD, "ramdisk-images/vendor.img", tmp_vendor)
        vendor_hash_pre = sha256(tmp_vendor)
        print(f"Extracted vendor.img SHA-256: {vendor_hash_pre}")

        # 4. Verify mapper.default.so is already inside extracted vendor.img
        print("[4] Verifying mapper.default.so inside vendor.img using debugfs...")
        extracted_mapper_so = Path(tmpdir) / "dumped_mapper.so"
        dump_mapper_cmd = [str(DEBUGFS), "-R", f"dump /lib64/hw/mapper.default.so {extracted_mapper_so}", str(tmp_vendor)]
        res_m = subprocess.run(dump_mapper_cmd, capture_output=True, text=True, check=True)
        dumped_mapper_hash = sha256(extracted_mapper_so)
        print(f"Dumped mapper.default.so SHA-256: {dumped_mapper_hash}")
        assert dumped_mapper_hash == EXPECTED_MAPPER_SO, f"dumped mapper SHA mismatch: {dumped_mapper_hash}"

        ea_mapper_cmd = [str(DEBUGFS), "-R", "ea_get /lib64/hw/mapper.default.so security.selinux", str(tmp_vendor)]
        res_ea_m = subprocess.run(ea_mapper_cmd, capture_output=True, text=True, check=True)
        print(f"SELinux EA mapper check: {res_ea_m.stdout.strip()}")
        assert "u:object_r:same_process_hal_file:s0" in res_ea_m.stdout, "SELinux label mismatch on mapper!"

        # 5. Patch all 4 SwiftShader aliases inside vendor.img using debugfs
        print("[5] Writing patched SwiftShader to vendor.img (/lib64/egl/*)...")
        cmds = []
        aliases = [
            "libGLES_swiftshader.so",
            "libEGL_swiftshader.so",
            "libGLESv1_CM_swiftshader.so",
            "libGLESv2_swiftshader.so",
        ]
        for alias in aliases:
            cmds.extend([
                f"rm /lib64/egl/{alias}",
                f"write {PATCHED_SWIFTSHADER_SO} /lib64/egl/{alias}",
                f"sif /lib64/egl/{alias} mode 0100644",
                f"sif /lib64/egl/{alias} uid 0",
                f"sif /lib64/egl/{alias} gid 0",
                f"ea_set /lib64/egl/{alias} security.selinux u:object_r:same_process_hal_file:s0",
            ])

        debugfs_script = Path(tmpdir) / "patch_swiftshader.debugfs"
        debugfs_script.write_text("\n".join(cmds) + "\n")

        debugfs_res = subprocess.run(
            [str(DEBUGFS), "-w", "-f", str(debugfs_script), str(tmp_vendor)],
            capture_output=True,
            text=True,
            check=True,
        )
        (FLIGHT / "debugfs-write.log").write_text(debugfs_res.stdout + debugfs_res.stderr)

        # 6. Verify filesystem with e2fsck
        print("[6] Verifying vendor filesystem with e2fsck...")
        fsck_res = subprocess.run(
            [str(E2FSCK), "-f", "-n", str(tmp_vendor)],
            capture_output=True,
            text=True,
        )
        (FLIGHT / "vendor-e2fsck.log").write_text(fsck_res.stdout + fsck_res.stderr)
        if fsck_res.returncode != 0:
            raise RuntimeError(f"vendor.img e2fsck failed: {fsck_res.stdout}")
        print("e2fsck check clean.")

        # 7. Verify dumped swiftshader from vendor.img
        print("[7] Verifying dumped SwiftShader from vendor.img...")
        extracted_sw_so = Path(tmpdir) / "dumped_swiftshader.so"
        dump_sw_cmd = [str(DEBUGFS), "-R", f"dump /lib64/egl/libGLES_swiftshader.so {extracted_sw_so}", str(tmp_vendor)]
        subprocess.run(dump_sw_cmd, capture_output=True, text=True, check=True)
        dumped_sw_hash = sha256(extracted_sw_so)
        print(f"Dumped libGLES_swiftshader.so SHA-256: {dumped_sw_hash}")
        assert dumped_sw_hash == EXPECTED_SWIFTSHADER_SO, "dumped swiftshader SHA mismatch!"

        ea_sw_cmd = [str(DEBUGFS), "-R", "ea_get /lib64/egl/libGLES_swiftshader.so security.selinux", str(tmp_vendor)]
        res_ea_sw = subprocess.run(ea_sw_cmd, capture_output=True, text=True, check=True)
        print(f"SELinux EA swiftshader check: {res_ea_sw.stdout.strip()}")
        assert "u:object_r:same_process_hal_file:s0" in res_ea_sw.stdout, "SELinux label mismatch on swiftshader!"

        # Re-verify mapper is still present
        dump_mapper_cmd2 = [str(DEBUGFS), "-R", f"dump /lib64/hw/mapper.default.so {extracted_mapper_so}", str(tmp_vendor)]
        subprocess.run(dump_mapper_cmd2, capture_output=True, text=True, check=True)
        assert sha256(extracted_mapper_so) == EXPECTED_MAPPER_SO, "mapper.default.so altered during swiftshader update!"

        vendor_sha_post = sha256(tmp_vendor)
        print(f"Patched vendor.img SHA-256: {vendor_sha_post}")

        # 8. Repack into single merged initramfs
        print(f"[8] Repacking into {OUTPUT_INITRD}...")
        repack_with_vendor(T639_INITRD, OUTPUT_INITRD, OUTPUT_PARTIAL, tmp_vendor)

    # 9. Verify merged initramfs integrity and lineage
    print("[9] Verifying merged initramfs integrity and lineage...")
    merged_sha = sha256(OUTPUT_INITRD)
    print(f"Merged initramfs SHA-256: {merged_sha}")

    embedded_system_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/system.img")
    print(f"Merged embedded system.img SHA-256: {embedded_system_sha}")
    assert embedded_system_sha == EXPECTED_SYSTEM_IMG, f"embedded system SHA mismatch: {embedded_system_sha}"

    embedded_vendor_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/vendor.img")
    print(f"Merged embedded vendor.img SHA-256: {embedded_vendor_sha}")
    assert embedded_vendor_sha == vendor_sha_post, f"embedded vendor SHA mismatch: {embedded_vendor_sha}"

    # Verify diagnostic script present
    diag_sha = cpio_member_sha256(OUTPUT_INITRD, "t636-diag.sh")
    print(f"Merged t636-diag.sh SHA-256:        {diag_sha}")

    manifest = {
        "track": "T640",
        "description": "Three-fix-merge: T636 classpath fix + T637 Gralloc 5 mapper.default.so + T638 SwiftShader AutoBackendTexture patch",
        "built_utc": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "images": {
            "kernel_Image": {"path": str(KERNEL), "sha256": kernel_hash},
            "t639_base_initramfs": {"path": str(T639_INITRD), "sha256": t639_hash},
            "merged_initramfs": {"path": str(OUTPUT_INITRD), "sha256": merged_sha},
            "embedded_system_img": {"member": "ramdisk-images/system.img", "sha256": embedded_system_sha},
            "embedded_vendor_img": {"member": "ramdisk-images/vendor.img", "sha256": embedded_vendor_sha},
            "embedded_t636_diag": {"member": "t636-diag.sh", "sha256": diag_sha},
        },
        "fixes_integrated": {
            "T636_classpath": {
                "jar": "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar",
                "target": "BOOTCLASSPATH 20 jars (MediaPlayer preload fix)"
            },
            "T637_mapper": {
                "library": "/vendor/lib64/hw/mapper.default.so",
                "sha256": EXPECTED_MAPPER_SO,
                "selinux_context": "u:object_r:same_process_hal_file:s0",
                "target": "Gralloc 5 Stable-C SP-HAL mapper (surfaceflinger-sigabrt-line3732 fix)"
            },
            "T638_swiftshader": {
                "libraries_patched": [f"/vendor/lib64/egl/{a}" for a in aliases],
                "sha256": EXPECTED_SWIFTSHADER_SO,
                "patch_offset_hex": "0x5afc",
                "selinux_context": "u:object_r:same_process_hal_file:s0",
                "target": "AutoBackendTexture anonymous-mmap fallback (surfaceflinger-skia-autobackendtexture-abort fix)"
            }
        }
    }
    manifest_path = FLIGHT / "guest-image-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"[10] Manifest written to {manifest_path}")
    print("=== T640 MERGED IMAGE BUILD COMPLETE ===")


if __name__ == "__main__":
    main()
