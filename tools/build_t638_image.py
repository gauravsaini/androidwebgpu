#!/usr/bin/env python3
"""Build one versioned T638 fix initramfs for SurfaceFlinger Skia texture creation."""

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
FLIGHT = ROOT / "flights" / "fix-01"

BASE_INITRD = INPUTS / "t637-gralloc-mapper-fix-initramfs.cpio.gz"
OUTPUT_INITRD = INPUTS / "t638-skia-texture-initramfs.cpio.gz"
OUTPUT_PARTIAL = OUTPUT_INITRD.with_suffix(".cpio.gz.partial")

KERNEL = INPUTS / "Image"
EXPECTED_KERNEL = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_BASE_INITRD = "dbb22d63d47ddc05c73b0d91c05be01801f93f87a778cc200600402554933c9c"
EXPECTED_SYSTEM = "1f9d601bfc44640c3680365e9d8b82fd3408f0c1dc6b515f4e47095b9bcb74df"

DEBUGFS = Path("/opt/homebrew/opt/e2fsprogs/sbin/debugfs")
E2FSCK = Path("/opt/homebrew/opt/e2fsprogs/sbin/e2fsck")

MIN_BUILD_FREE_BYTES = 3 * 1024**3
CHUNK = 8 * 1024 * 1024

# 200-byte patch for soft_egl_create_image at offset 0x5afc
SWIFTSHADER_PATCH = bytes.fromhex(
    "81 05 00 b4 "  # 5afc: cbz x1, 5bac
    "28 30 40 f9 "  # 5b00: ldr x8, [x1, #0x60]
    "2a 38 40 b9 "  # 5b04: ldr w10, [x1, #0x38]
    "2b 3c 40 b9 "  # 5b08: ldr w11, [x1, #0x3c]
    "2c 40 40 b9 "  # 5b0c: ldr w12, [x1, #0x40]
    "2d 44 40 b9 "  # 5b10: ldr w13, [x1, #0x44]
    "5f 05 00 71 "  # 5b14: cmp w10, #1
    "4a 00 00 54 "  # 5b18: b.ge 5b20
    "0a 10 80 52 "  # 5b1c: mov w10, #128
    "7f 05 00 71 "  # 5b20: cmp w11, #1
    "4a 00 00 54 "  # 5b24: b.ge 5b2c
    "0b 10 80 52 "  # 5b28: mov w11, #128
    "9f 01 0a 6b "  # 5b2c: cmp w12, w10
    "4a 00 00 54 "  # 5b30: b.ge 5b38
    "ec 03 0a 2a "  # 5b34: mov w12, w10
    "4d 00 00 35 "  # 5b38: cbnz w13, 5b40
    "2d 00 80 52 "  # 5b3c: mov w13, #1
    "81 7d ab 9b "  # 5b40: umull x1, w12, w11
    "21 f4 7e d3 "  # 5b44: lsl x1, x1, #2
    "e8 01 00 b4 "  # 5b48: cbz x8, 5b84
    "0e 05 40 b9 "  # 5b4c: ldr w14, [x8, #4]
    "df 05 00 71 "  # 5b50: cmp w14, #1
    "8b 01 00 54 "  # 5b54: b.lt 5b84
    "04 0d 40 b9 "  # 5b58: ldr w4, [x8, #0xc]
    "44 01 f8 37 "  # 5b5c: tbnz w4, #31, 5b84
    "00 00 80 d2 "  # 5b60: mov x0, #0
    "c8 1b 80 52 "  # 5b64: mov w8, #222
    "62 00 80 52 "  # 5b68: mov w2, #3
    "23 00 80 52 "  # 5b6c: mov w3, #1
    "05 00 80 d2 "  # 5b70: mov x5, #0
    "01 00 00 d4 "  # 5b74: svc #0
    "08 fc 3f 91 "  # 5b78: add x8, x0, #0xfff
    "1f 05 40 f1 "  # 5b7c: cmp x8, #1, lsl #12
    "82 01 00 54 "  # 5b80: b.hs 5bb0
    "00 00 80 d2 "  # 5b84: mov x0, #0
    "c8 1b 80 52 "  # 5b88: mov w8, #222
    "62 00 80 52 "  # 5b8c: mov w2, #3
    "43 04 80 52 "  # 5b90: mov w3, #0x22
    "e4 03 3f 2a "  # 5b94: mvn w4, wzr
    "05 00 80 d2 "  # 5b98: mov x5, #0
    "01 00 00 d4 "  # 5b9c: svc #0
    "08 fc 3f 91 "  # 5ba0: add x8, x0, #0xfff
    "1f 05 40 f1 "  # 5ba4: cmp x8, #1, lsl #12
    "42 00 00 54 "  # 5ba8: b.hs 5bb0
    "06 00 00 14 "  # 5bac: b 5bc4
    "07 00 00 14 "  # 5bb0: b 5bcc
    "1f 20 03 d5 "  # 5bb4: nop
    "1f 20 03 d5 "  # 5bb8: nop
    "1f 20 03 d5 "  # 5bbc: nop
    "1f 20 03 d5 "  # 5bc0: nop
)


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


def extract_member(source_cpio: Path, member_name: str, out_file: Path):
    with gzip.open(source_cpio, "rb") as stream:
        while True:
            _, _, name, _, size = read_entry(stream)
            if name == member_name:
                with out_file.open("wb") as out:
                    copy_exact(stream, out, size)
                return
            else:
                copy_exact(stream, None, size)
            padding = stream.read(pad4(size))
            if name == "TRAILER!!!":
                break
    raise RuntimeError(f"member {member_name} not found in {source_cpio}")


def repack_with_vendor(replacement_vendor: Path):
    seen_vendor = False
    with gzip.open(BASE_INITRD, "rb") as source, gzip.open(OUTPUT_PARTIAL, "wb", compresslevel=6) as target:
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
                raise RuntimeError("base initramfs has no embedded vendor image")
            if vendor_stream.tell() != replacement_vendor.stat().st_size:
                raise RuntimeError("not all patched vendor bytes were embedded")
    with gzip.open(OUTPUT_PARTIAL, "rb") as stream:
        while stream.read(CHUNK):
            pass
    os.replace(OUTPUT_PARTIAL, OUTPUT_INITRD)


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


def main():
    if OUTPUT_INITRD.exists() or OUTPUT_PARTIAL.exists():
        raise SystemExit(f"refusing to overwrite versioned image: {OUTPUT_INITRD}")
    if sha256(KERNEL) != EXPECTED_KERNEL:
        raise SystemExit("kernel SHA does not match canonical input")
    if sha256(BASE_INITRD) != EXPECTED_BASE_INITRD:
        raise SystemExit("T637 base initramfs SHA mismatch")

    free_bytes = shutil.disk_usage(INPUTS).free
    FLIGHT.mkdir(parents=True, exist_ok=True)
    (FLIGHT / "build-resources.txt").write_text(
        f"free_bytes_before_build={free_bytes}\nfree_gib_before_build={free_bytes / 1024**3:.3f}\n"
    )
    if free_bytes < MIN_BUILD_FREE_BYTES:
        raise SystemExit(f"disk reserve is below {MIN_BUILD_FREE_BYTES} bytes; aborting")

    with tempfile.TemporaryDirectory(prefix="t638-build-", dir="/private/tmp") as tmp_dir_name:
        tmp_dir = Path(tmp_dir_name)
        extracted_vendor = tmp_dir / "vendor.img"
        print("Extracting vendor.img from T637 base initramfs...")
        extract_member(BASE_INITRD, "ramdisk-images/vendor.img", extracted_vendor)

        orig_so_path = tmp_dir / "libGLES_swiftshader_orig.so"
        res = subprocess.run(
            [str(DEBUGFS), "-R", "dump /lib64/egl/libGLES_swiftshader.so " + str(orig_so_path), str(extracted_vendor)],
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            raise RuntimeError(f"failed to dump swiftshader: {res.stderr}")

        orig_so_data = bytearray(orig_so_path.read_bytes())
        assert orig_so_data[0x5afc:0x5b00] == bytes.fromhex("41 06 00 b4"), "unrecognized swiftshader binary"
        orig_so_data[0x5afc:0x5afc + len(SWIFTSHADER_PATCH)] = SWIFTSHADER_PATCH

        patched_so_path = tmp_dir / "libGLES_swiftshader_patched.so"
        patched_so_path.write_bytes(orig_so_data)
        patched_so_sha = hashlib.sha256(orig_so_data).hexdigest()

        # Build debugfs script to update all 4 swiftshader aliases in /lib64/egl/
        cmds = []
        for alias in [
            "libGLES_swiftshader.so",
            "libEGL_swiftshader.so",
            "libGLESv1_CM_swiftshader.so",
            "libGLESv2_swiftshader.so",
        ]:
            cmds.extend([
                f"rm /lib64/egl/{alias}",
                f"write {patched_so_path} /lib64/egl/{alias}",
                f"sif /lib64/egl/{alias} mode 0100644",
                f"sif /lib64/egl/{alias} uid 0",
                f"sif /lib64/egl/{alias} gid 0",
                f"ea_set /lib64/egl/{alias} security.selinux u:object_r:same_process_hal_file:s0",
            ])

        debugfs_script = tmp_dir / "patch_swiftshader.debugfs"
        debugfs_script.write_text("\n".join(cmds) + "\n")

        print("Writing patched swiftshader to vendor.img via debugfs...")
        debugfs_res = subprocess.run(
            [str(DEBUGFS), "-w", "-f", str(debugfs_script), str(extracted_vendor)],
            capture_output=True,
            text=True,
        )
        (FLIGHT / "debugfs-write.log").write_text(debugfs_res.stdout + debugfs_res.stderr)
        if debugfs_res.returncode != 0:
            raise RuntimeError(f"debugfs write failed: {debugfs_res.stderr}")

        print("Verifying filesystem with e2fsck...")
        fsck_res = subprocess.run(
            [str(E2FSCK), "-f", "-n", str(extracted_vendor)],
            capture_output=True,
            text=True,
        )
        (FLIGHT / "vendor-e2fsck.log").write_text(fsck_res.stdout + fsck_res.stderr)
        if fsck_res.returncode != 0:
            raise RuntimeError(f"vendor.img e2fsck failed: {fsck_res.stdout}")

        vendor_sha = sha256(extracted_vendor)
        print(f"Patched vendor.img SHA-256: {vendor_sha}")

        print("Repacking T638 fix initramfs...")
        repack_with_vendor(extracted_vendor)

    system_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/system.img")
    embedded_vendor_sha = cpio_member_sha256(OUTPUT_INITRD, "ramdisk-images/vendor.img")
    if system_sha != EXPECTED_SYSTEM or embedded_vendor_sha != vendor_sha:
        raise RuntimeError("embedded image SHA mismatch")

    initramfs_sha = sha256(OUTPUT_INITRD)
    print(f"T638 initramfs created: {OUTPUT_INITRD}")
    print(f"SHA-256: {initramfs_sha}")

    manifest = {
        "track": "T638",
        "lineage": "T637 gralloc-mapper base plus T638 swiftshader AutoBackendTexture anonymous-mmap fallback",
        "built_utc": dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds"),
        "images": {
            "kernel_Image": {"path": str(KERNEL), "sha256": sha256(KERNEL)},
            "initramfs": {"path": str(OUTPUT_INITRD), "sha256": initramfs_sha},
            "embedded_system_img": {"member": "ramdisk-images/system.img", "sha256": system_sha},
            "embedded_vendor_img": {"member": "ramdisk-images/vendor.img", "sha256": embedded_vendor_sha},
        },
        "swiftshader_texture_fix": {
            "libraries_patched": [
                "/vendor/lib64/egl/libGLES_swiftshader.so",
                "/vendor/lib64/egl/libEGL_swiftshader.so",
                "/vendor/lib64/egl/libGLESv1_CM_swiftshader.so",
                "/vendor/lib64/egl/libGLESv2_swiftshader.so",
            ],
            "patched_so_sha256": patched_so_sha,
            "fix_target": "AutoBackendTexture / soft_egl_create_image anonymous mmap fallback",
            "patch_offset_hex": "0x5afc",
            "patch_size_bytes": len(SWIFTSHADER_PATCH),
        },
        "t637_initramfs_sha256": EXPECTED_BASE_INITRD,
    }
    (FLIGHT / "guest-image-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("Manifest generated at:", FLIGHT / "guest-image-manifest.json")


if __name__ == "__main__":
    main()
