#!/usr/bin/env python3
import hashlib
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")

EXPECTED = {
    "findings-t704.md": ("c572f0df", 19065, WORKTREE / "findings-t704.md"),
    "findings-t703.md": ("6e6582ab", 22137, WORKTREE / "findings-t703.md"),
    "findings-t696.md": ("7184046210a2b439bf57c49305aa3558a119e4f1bdec1f8bb8beda58785c5102", None, WORKTREE / "findings-t696.md"),
    "system.img": ("afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7", 2059816960, WORKTREE / "guest-inputs/system.img"),
    "routeB-Image": ("3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3", 20126208, WORKTREE / "guest-inputs/routeB-Image"),
    "t699-systemserver-fb0-fix.cpio.gz": ("c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b", 958780579, WORKTREE / "guest-inputs/t699-systemserver-fb0-fix.cpio.gz"),
    "modified.dtb": ("e83a6437", 1920, WORKTREE / "guest-inputs/modified.dtb"),
    "composer": ("ddc1e4dd", 19941040, WORKTREE / "guest-inputs/android.hardware.graphics.composer@2.1-service"),
    "extracted_kernel.config": ("d840310a", 133733, WORKTREE / "guest-inputs/extracted_kernel.config"),
}

def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

all_pass = True
for name, (exp_prefix, exp_size, path) in EXPECTED.items():
    if not path.exists():
        print(f"FAIL: {name} not found at {path}")
        all_pass = False
        continue
    sz = path.stat().st_size
    actual_sha = sha256_file(path)
    if not actual_sha.startswith(exp_prefix):
        print(f"FAIL: {name} SHA mismatch. Expected prefix {exp_prefix}, got {actual_sha}")
        all_pass = False
    elif exp_size is not None and sz != exp_size:
        print(f"FAIL: {name} size mismatch. Expected {exp_size}, got {sz}")
        all_pass = False
    else:
        print(f"PASS: {name} (size {sz}, sha256 {actual_sha})")

if not all_pass:
    sys.exit(1)
print("ALL INPUTS VERIFIED SUCCESSFULLY")
