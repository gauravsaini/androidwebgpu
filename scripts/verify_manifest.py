#!/usr/bin/env python3
"""Validate T499's pinned native-QEMU/Rust input manifest."""

import hashlib
import json
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
manifest = json.loads((ROOT / "MANIFEST.json").read_text())
required = {
    "cpu",
    "ram_mb",
    "bootargs",
    "kernel_sha256",
    "dtb_sha256",
    "initramfs_sha256",
    "mmio_map",
    "qemu_version",
    "rust_binary_rev",
}
missing = required - manifest.keys()
if missing:
    raise SystemExit(f"manifest fields missing: {sorted(missing)}")

expected_mmio = {
    "framebuffer": "0x5F000000",
    "gpu_port": "0x0A000000",
    "pl011": "0x09000000",
}
if manifest["mmio_map"] != expected_mmio:
    raise SystemExit(f"MMIO map differs: {manifest['mmio_map']!r}")

for role in ("kernel", "dtb", "initramfs"):
    path = Path(manifest[f"{role}_path"])
    actual = hashlib.sha256(path.read_bytes()).hexdigest()
    expected = manifest[f"{role}_sha256"]
    if actual != expected:
        raise SystemExit(f"{role} SHA-256 mismatch: {actual} != {expected}")

dtb = manifest["dtb_path"]
actual_bootargs = subprocess.check_output(
    ["fdtget", dtb, "/chosen", "bootargs"], text=True
).strip()
actual_cpu = subprocess.check_output(
    ["fdtget", "-t", "s", dtb, "/cpus/cpu@0", "compatible"], text=True
).strip()
if actual_bootargs != manifest["bootargs"]:
    raise SystemExit(f"DTB bootargs mismatch: {actual_bootargs!r}")
if actual_cpu != "arm,cortex-a53" or manifest["cpu"] != "cortex-a53":
    raise SystemExit(f"CPU identity mismatch: DTB={actual_cpu!r}")

version = subprocess.check_output(
    [manifest["qemu_binary"], "--version"], text=True
).splitlines()[0]
if version != manifest["qemu_version"]:
    raise SystemExit(f"QEMU version mismatch: {version!r}")

if manifest["initramfs_size_bytes"] != Path(manifest["initramfs_path"]).stat().st_size:
    raise SystemExit("initramfs byte count differs from manifest")

print("G499_1_EVIDENCE")
