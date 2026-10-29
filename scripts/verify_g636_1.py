#!/usr/bin/env python3
"""Oracle check for Gate G636-1: Inputs, single fix image, QEMU 11.1.2, and verbatim Zygote BOOTCLASSPATH."""
from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track636-bootclasspath-fix")
FLIGHT = WORKTREE / "flights/flight-01"
LOG = FLIGHT / "verify-boot.log"
MANIFEST = WORKTREE / "guest-image/t636-manifest.json"

EXPECTED = {
    "kernel": ("05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04", WORKTREE / "guest-inputs/Image"),
    "qemu": ("8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869", Path("/opt/homebrew/bin/qemu-system-aarch64")),
    "base_image": ("6b0b0d715b92e97b9a997c60f7c5e06cff81d7178af92d1ed28fc55a95414e17", WORKTREE / "guest-inputs/t628-iallocator-service-initramfs.cpio.gz"),
    "boot_image": (None, WORKTREE / "guest-image/t636-bootclasspath-fixed-initramfs.cpio.gz"),
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    # 1. Verify inputs
    for name, (expected, path) in EXPECTED.items():
        assert path.is_file(), f"missing input: {path}"
        actual = sha256_file(path)
        if expected is not None:
            assert actual == expected, f"{name} SHA mismatch: {actual} != {expected}"

    # 2. Verify single fix image manifest
    assert MANIFEST.is_file(), f"missing manifest file: {MANIFEST}"
    with open(MANIFEST, "r", encoding="utf-8") as f:
        m = json.load(f)
    assert m.get("track") == "T636", "manifest track must be T636"
    assert m.get("bootclasspath_entries_count") == 20, "manifest must declare 20 bootclasspath entries"
    assert "framework-connectivity.jar" in m.get("bootclasspath_20", ""), "framework-connectivity.jar missing from manifest"

    # 3. Verify flight log
    assert LOG.is_file(), f"missing flight log: {LOG}"
    text = LOG.read_text(encoding="utf-8", errors="replace")

    # 4. Prove verbatim Zygote BOOTCLASSPATH contains framework-connectivity.jar
    has_zygote_proc = "T636_ZYGOTE_PROCESS_FOUND" in text or "T636_ZYGOTE_PROP_FOUND" in text
    assert has_zygote_proc, "missing guest evidence: zygote PID not found in guest flight"

    has_zygote_bcp = "T636_ZYGOTE_BOOTCLASSPATH_VERBATIM" in text
    assert has_zygote_bcp, "missing guest evidence: verbatim zygote environ BOOTCLASSPATH not captured"

    bcp_matches = re.findall(r"T636_ZYGOTE_BOOTCLASSPATH_VERBATIM pid=(\d+) BOOTCLASSPATH=(.*)", text)
    assert bcp_matches, "no verbatim BOOTCLASSPATH matches found"

    matched_connectivity = False
    for pid, bcp in bcp_matches:
        if "framework-connectivity.jar" in bcp:
            matched_connectivity = True
            break
    assert matched_connectivity, "zygote environ BOOTCLASSPATH does not contain framework-connectivity.jar!"

    print("G636_1_PASS")


if __name__ == "__main__":
    main()
