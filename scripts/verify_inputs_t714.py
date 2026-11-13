#!/usr/bin/env python3
"""Comprehensive input verification for Track T714."""

import hashlib
import os
import subprocess
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track714-binary-patch")
T713_WORKTREE = Path("/Users/Shared/wt-track713-sensorstart-bypass")

INPUTS = {
    "findings-t713.md (T713 worktree)": (
        T713_WORKTREE / "findings-t713.md",
        "1d4b053db58c3f880ae30e15c5bbd0a765bde0341693e47f06009d66a3a334d2",
        16621,
    ),
    "DEATH_NOTE-T713.md (T713 worktree)": (
        T713_WORKTREE / "DEATH_NOTE-T713.md",
        "fb2c96f7d8e6875b1b55c9f8f1822ea444ffbaf8c523360bdd018d1ea4221f18",
        5099,
    ),
    "T713 flight-01 uart.log": (
        T713_WORKTREE / "flights/flight-01/uart.log",
        "dbd2108d4cda477a6d1c67ad80d4fd58f1fecd4e23a6c6be34204fc4cdac49cf",
        None,
    ),
    "T713 flight-ab uart.log": (
        T713_WORKTREE / "flights/flight-ab/uart.log",
        "243f07249f1b1c730137d558befb20296336453d34b9e1922ba30bfd9cbf323e",
        None,
    ),
    "T713 smoke uart.log": (
        T713_WORKTREE / "flights/smoke/uart.log",
        "297435f5ef665a6e8eb73aba8d7f74a57fb119e5552d5e33336674692707de35",
        None,
    ),
    "T713 fix initramfs": (
        WORKTREE / "guest-inputs/t713-fix.cpio.gz",
        "edba515eac53a2544ba33aa17cb3749c37a2f336207a3efcdfffb94e56ac3177",
        53502851,
    ),
    "T713 modified.dtb": (
        WORKTREE / "guest-inputs/t713-modified.dtb",
        "d60bc98a67c8b8c014b65412d9af869a432a744f7f1eab24b78a290fdf396161",
        2049,
    ),
    "T712 vendor.img": (
        WORKTREE / "guest-inputs/t712-vendor.img",
        "095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf",
        134217728,
    ),
    "T708 rebuilt Composer ELF": (
        WORKTREE / "guest-inputs/t708-android.hardware.graphics.composer@2.1-service",
        "8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401",
        19942712,
    ),
    "T706 patched system.img": (
        WORKTREE / "guest-inputs/system.img",
        "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601",
        2059816960,
    ),
    "T705 rebuilt Image": (
        WORKTREE / "guest-inputs/t705-Image",
        "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859",
        20126208,
    ),
    "extracted_kernel.config": (
        WORKTREE / "extracted_kernel.config",
        "d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8",
        133733,
    ),
}

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    print("Verifying 12 staged inputs on Mac...")
    all_ok = True
    for name, (path, expected_sha, expected_size) in INPUTS.items():
        if not path.exists():
            print(f"[FAIL] {name}: file missing at {path}")
            all_ok = False
            continue
        size = path.stat().st_size
        if expected_size is not None and size != expected_size:
            print(f"[FAIL] {name}: size mismatch (expected {expected_size}, got {size})")
            all_ok = False
            continue
        actual_sha = sha256_file(path)
        if actual_sha != expected_sha:
            print(f"[FAIL] {name}: sha mismatch (expected {expected_sha}, got {actual_sha})")
            all_ok = False
            continue
        print(f"[PASS] {name}: {size} B, SHA-256 {actual_sha}")

    if not all_ok:
        sys.exit(1)

    print("\nVerifying authoritative copies on Linux box vault...")
    box_cmd = (
        "sha256sum "
        "/mnt/sdb1/kernel-critical/lane-images/t713-fix.cpio.gz "
        "/mnt/sdb1/kernel-critical/lane-images/t713-modified.dtb "
        "/mnt/sdb1/kernel-critical/lane-images/t712-vendor.img "
        "/mnt/sdb1/kernel-critical/lane-images/t706-system.img"
    )
    res = subprocess.run(["/Users/ektasaini/workspace/bin/boxssh.sh", box_cmd], capture_output=True, text=True)
    if res.returncode != 0:
        print(f"[FAIL] boxssh command failed: {res.stderr}")
        sys.exit(1)
    print(res.stdout)
    box_expected = {
        "t713-fix.cpio.gz": "edba515eac53a2544ba33aa17cb3749c37a2f336207a3efcdfffb94e56ac3177",
        "t713-modified.dtb": "d60bc98a67c8b8c014b65412d9af869a432a744f7f1eab24b78a290fdf396161",
        "t712-vendor.img": "095386ab640836f01859edc1ff955dab8644dce88e1c7597969c51c8d55578bf",
        "t706-system.img": "2de0533e93a90298e043ee71ca19a2c4545df601ceafc9e14fb507ef56506601",
    }
    for line in res.stdout.strip().splitlines():
        parts = line.split()
        if len(parts) >= 2:
            h, p = parts[0], parts[1]
            fname = os.path.basename(p)
            if fname in box_expected:
                assert h == box_expected[fname], f"Box vault SHA mismatch for {fname}: got {h}"
                print(f"[PASS] Box vault {fname}: SHA-256 {h}")

    print("\nALL 12 INPUTS VERIFIED SUCCESSFULLY.")

if __name__ == "__main__":
    main()
