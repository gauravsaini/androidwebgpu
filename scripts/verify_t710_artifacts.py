#!/usr/bin/env python3
import hashlib
import os
import subprocess
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track710-sensorservice-wall-cure")
CPIO = WORKTREE / "guest-inputs/t710-fix.cpio.gz"
DTB = WORKTREE / "guest-inputs/t710-modified.dtb"
STREAMER = WORKTREE / "guest-inputs/t710_logcat_streamer"
SENSOR = WORKTREE / "guest-inputs/dummy_sensorservice"

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(8 * 1024 * 1024):
            h.update(chunk)
    return h.hexdigest()

def main():
    assert CPIO.is_file(), f"Missing CPIO: {CPIO}"
    assert DTB.is_file(), f"Missing DTB: {DTB}"
    assert STREAMER.is_file(), f"Missing STREAMER: {STREAMER}"
    assert SENSOR.is_file(), f"Missing SENSOR: {SENSOR}"

    cpio_size = CPIO.stat().st_size
    dtb_size = DTB.stat().st_size
    cpio_sha = sha256_file(CPIO)
    dtb_sha = sha256_file(DTB)

    print(f"CPIO: size={cpio_size}, sha256={cpio_sha}")
    print(f"DTB:  size={dtb_size}, sha256={dtb_sha}")

    # Verify DTB initrd-end calculation
    expected_end = 0x110000000 + cpio_size
    high_end = (expected_end >> 32) & 0xFFFFFFFF
    low_end = expected_end & 0xFFFFFFFF
    end_str = f"<0x{high_end:02x} 0x{low_end:08x}>"

    dts = subprocess.check_output(["/opt/homebrew/bin/dtc", "-I", "dtb", "-O", "dts", str(DTB)]).decode()
    assert f"linux,initrd-end = {end_str};" in dts, f"DTB initrd-end mismatch: expected {end_str}"
    print(f"DTB alignment verified: linux,initrd-end = {end_str} ({hex(expected_end)})")
    print("ARTIFACTS_VERIFIED_OK")

if __name__ == "__main__":
    main()
