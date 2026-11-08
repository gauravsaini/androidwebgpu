#!/usr/bin/env python3
import hashlib
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")
IMAGE = WORKTREE / "guest-inputs/t705-Image"
CONFIG = WORKTREE / "guest-inputs/t705-kernel.config"

assert IMAGE.exists(), "Rebuilt Image missing"
assert CONFIG.exists(), "Kernel config missing"

h = hashlib.sha256(IMAGE.read_bytes()).hexdigest()
assert h == "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859", f"Image SHA mismatch: {h}"
assert IMAGE.stat().st_size == 20126208, f"Image size mismatch: {IMAGE.stat().st_size}"

cfg = CONFIG.read_text()
assert "CONFIG_VIRTIO_BLK=y" in cfg, "CONFIG_VIRTIO_BLK=y missing from config"

print(f"KERNEL_REBUILD_AND_GATES_PASS: Image SHA={h}, size={IMAGE.stat().st_size}")
