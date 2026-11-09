#!/usr/bin/env python3
import hashlib
import os
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track709-logcat-systemserver-cure")
T708_DIR = Path("/Users/Shared/wt-track708-primecache-disable")

GUEST_INPUTS = WORKTREE / "guest-inputs"
GUEST_INPUTS.mkdir(parents=True, exist_ok=True)

# Files to hardlink from T708 worktree
LINKS = [
    ("guest-inputs/t708-systemserver-fb0-fix.cpio.gz", "guest-inputs/t708-systemserver-fb0-fix.cpio.gz"),
    ("guest-inputs/t708-modified.dtb", "guest-inputs/t708-modified.dtb"),
    ("guest-inputs/t708-vendor.img", "guest-inputs/t708-vendor.img"),
    ("guest-inputs/t708-android.hardware.graphics.composer@2.1-service", "guest-inputs/t708-android.hardware.graphics.composer@2.1-service"),
    ("guest-inputs/system.img", "guest-inputs/system.img"),
    ("guest-inputs/t705-Image", "guest-inputs/t705-Image"),
    ("guest-inputs/extracted_kernel.config", "guest-inputs/extracted_kernel.config"),
    ("guest-inputs/canonical-Image", "guest-inputs/canonical-Image"),
]

for src_rel, dst_rel in LINKS:
    src = T708_DIR / src_rel
    dst = WORKTREE / dst_rel
    if not dst.exists():
        if src.exists():
            os.link(src, dst)
            print(f"Hardlinked {src} -> {dst}")
        else:
            print(f"ERROR: source does not exist: {src}")
            sys.exit(1)

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

INPUTS = {
    "findings-t708.md": (
        WORKTREE / "findings-t708.md",
        "e67d4d55e31afb7bc951fa9f35615ce8bb0b7e8c2d52fd3967697a2a95eb232d",
        20165,
        None,
    ),
    "T708 flight-01 UART": (
        T708_DIR / "flights/flight-01/uart.log",
        "689f4f84df291cbccf0d1d78c1a3ebe1ba53b2a0c717d95ea31e455cbe1ab2d2",
        544137,
        5562,
    ),
    "T708 flight-ab UART": (
        T708_DIR / "flights/flight-ab/uart.log",
        "ac66738502486d386c4a6c13e9a90fa3b56fbe3a0d19f1f0e31fce6f3297ffd3",
        46940,
        560,
    ),
    "T708 smoke re-run UART": (
        T708_DIR / "flights/smoke/uart.log",
        "e3ab4d16262fc3bc440d1c9b35ae33ba53dfe54a6fdc1ff9a360d29f9b2e2c45",
        286267,
        3657,
    ),
    "T708 smoke-attempt-1 UART": (
        T708_DIR / "flights/smoke-attempt-1/uart.log",
        "2b8ac9be575c70cde55bd304cfb28f6c2db4ebc92a1726ec91cf8561a602a33f",
        308109,
        3921,
    ),
    "T708 fix initramfs": (
        GUEST_INPUTS / "t708-systemserver-fb0-fix.cpio.gz",
        "49d395d3e195cdbe2686c61a3b53b32a48ed16e81b592a67e71f1b8236fe1ad9",
        53172530,
        None,
    ),
    "T708 modified.dtb": (
        GUEST_INPUTS / "t708-modified.dtb",
        "9a85c074df4a6c0a84fec65f793cb1b63a73c94ba135d247ea5bf936539fb85c",
        2049,
        None,
    ),
    "T708 vendor.img": (
        GUEST_INPUTS / "t708-vendor.img",
        "8d5506ab441ee37e31cf2071fce30b466c8c29f60a9472f654e27f7c9ba33092",
        134217728,
        None,
    ),
    "T708 rebuilt Composer ELF": (
        GUEST_INPUTS / "t708-android.hardware.graphics.composer@2.1-service",
        "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401",
        19942712,
        None,
    ),
    "T706 patched system.img": (
        GUEST_INPUTS / "system.img",
        "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601",
        2059816960,
        None,
    ),
    "T705 rebuilt Image": (
        GUEST_INPUTS / "t705-Image",
        "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859",
        20126208,
        None,
    ),
    "extracted_kernel.config": (
        GUEST_INPUTS / "extracted_kernel.config",
        "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8",
        133733,
        None,
    ),
    "canonical-Image": (
        GUEST_INPUTS / "canonical-Image",
        "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
        23073280,
        None,
    ),
}

for name, (path, exp_sha, exp_sz, exp_lines) in INPUTS.items():
    if not path.exists():
        print(f"MISSING: {name} at {path}")
        sys.exit(1)
    sz = path.stat().st_size
    if sz != exp_sz:
        print(f"SIZE MISMATCH for {name}: expected {exp_sz}, got {sz}")
        sys.exit(1)
    act_sha = sha256_file(path)
    if act_sha != exp_sha:
        print(f"SHA MISMATCH for {name}: expected {exp_sha}, got {act_sha}")
        sys.exit(1)
    if exp_lines is not None:
        lines = len(path.read_bytes().splitlines())
        if lines != exp_lines:
            print(f"LINES MISMATCH for {name}: expected {exp_lines}, got {lines}")
            sys.exit(1)
    print(f"VERIFIED: {name} ({sz} bytes, sha256: {act_sha[:16]}...)")

print("INPUTS_VERIFIED_OK")
