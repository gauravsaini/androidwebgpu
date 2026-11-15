#!/usr/bin/env python3
"""Input verification for Track T715 (Mac-local; box vault copies were
verified separately from the sandbox and are recorded in progress-t715.txt)."""
import hashlib
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track715-installd-trigger")
T714_WORKTREE = Path("/Users/Shared/wt-track714-binary-patch")

INPUTS = {
    "findings-t714.md (T714 worktree)": (
        T714_WORKTREE / "findings-t714.md",
        "f0b2d83d1bb13b9896fec648fcdce593b0182349945c18d1ff62eacbeda61fdb",
        14102,
    ),
    "DEATH_NOTE-T714.md (T714 worktree)": (
        T714_WORKTREE / "DEATH_NOTE-T714.md",
        "ea7cc7bae5f11cff81ce5f8b0b826e90447adc638bd84041a420d1258ef6b6db",
        11438,
    ),
    "T714 flight-01 uart.log": (
        T714_WORKTREE / "flights/flight-01/uart.log",
        "129942be454964593fad4eba72d33f504a2f35defabd36b8bc48cc23c979ef78",
        6950481,
    ),
    "T714 flight-ab uart.log": (
        T714_WORKTREE / "flights/flight-ab/uart.log",
        "6a2a008cd4a4044f6ef61285055a65812771b186af274e9f813d0c742ff487ff",
        47928,
    ),
    "T714 smoke uart.log": (
        T714_WORKTREE / "flights/smoke/uart.log",
        "7b83a44a966023f97b2df6a9cd9939fc9caa81b0f446dc93a763a4a1a958d7d0",
        1334831,
    ),
    "T714 patched system.img (base)": (
        WORKTREE / "guest-inputs/system.img",
        "ea5af412552668682f185a59dd4af39d600325c4ad1aaa62e0413602fae143ef",
        2059816960,
    ),
    "T715 patched system.img (new)": (
        WORKTREE / "guest-inputs/t715-system.img",
        "1283cbf279038f692ae42adc7bab571e89ef4ffd97efb0d982ee46e8050bfe3c",
        2059816960,
    ),
    "T714 patched .so": (
        WORKTREE / "guest-inputs/libandroid_servers_patched.so",
        "ea4ce478f0e72d30018bd6bdac3cabbb506fdc8d616d3b8b7e6af58036482d57",
        850584,
    ),
    "T714 unpatched .so": (
        WORKTREE / "guest-inputs/libandroid_servers.so",
        "2ce96adb15e4a8934d52cfaa8825c1b500b3eefa6a989e071a33963f1580539c",
        850584,
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
    "T705 rebuilt Image": (
        WORKTREE / "guest-inputs/t705-Image",
        "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859",
        20126208,
    ),
    "canonical-Image": (
        WORKTREE / "guest-inputs/canonical-Image",
        "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04",
        23073280,
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
    print("Verifying 16 staged inputs on Mac...")
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
    print("\nALL 16 INPUTS VERIFIED SUCCESSFULLY.")


if __name__ == "__main__":
    main()
