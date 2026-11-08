#!/usr/bin/env python3
import gzip
import hashlib
import subprocess
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")
CPIO = WORKTREE / "guest-inputs/t705-systemserver-fb0-fix.cpio.gz"
DTB = WORKTREE / "guest-inputs/t705-modified.dtb"
SYS_IMG = WORKTREE / "guest-inputs/system.img"
DTC = "/opt/homebrew/bin/dtc"

assert CPIO.exists(), "Rebuilt CPIO missing"
assert DTB.exists(), "Rebuilt DTB missing"
assert SYS_IMG.exists(), "External system.img missing"

# 1. CPIO member check
has_embedded_system = False
early_ramdisk_content = None
member_count = 0
with gzip.open(CPIO, "rb") as f:
    while True:
        hdr = f.read(110)
        if not hdr or len(hdr) < 110 or not hdr.startswith(b"070701"):
            break
        namesize = int(hdr[94:102], 16)
        filesize = int(hdr[54:62], 16)
        name = f.read(namesize).rstrip(b"\0").decode("utf-8", "replace")
        pad_name = (4 - ((110 + namesize) % 4)) % 4
        if pad_name:
            f.read(pad_name)
        pad_file = (4 - (filesize % 4)) % 4
        member_count += 1
        if name == "ramdisk-images/system.img":
            has_embedded_system = True
        elif name == "early-ramdisk":
            early_ramdisk_content = f.read(filesize).decode("utf-8")
            if pad_file:
                f.read(pad_file)
            continue
        f.seek(filesize + pad_file, 1)

assert not has_embedded_system, "CRITICAL: CPIO still contains ramdisk-images/system.img"
assert early_ramdisk_content is not None, "early-ramdisk not found in CPIO"
assert "/dev/vda configured for /system mount" in early_ramdisk_content, "/dev/vda configuration missing in early-ramdisk"
assert "losetup -r /dev/loop0" not in early_ramdisk_content, "loop0 still present in early-ramdisk"

# 2. DTB alignment check
cpio_sz = CPIO.stat().st_size
expected_end = 0x110000000 + cpio_sz
high = (expected_end >> 32) & 0xFFFFFFFF
low = expected_end & 0xFFFFFFFF
exp_prop = f"<0x{high:02x} 0x{low:08x}>"

dts = subprocess.check_output([DTC, "-I", "dtb", "-O", "dts", str(DTB)]).decode("utf-8")
assert f"linux,initrd-end = {exp_prop};" in dts, f"DTB initrd-end mismatch. Expected {exp_prop}"

# 3. External system.img SHA check
sys_sha = hashlib.sha256()
with open(SYS_IMG, "rb") as f:
    while chunk := f.read(8 * 1024 * 1024):
        sys_sha.update(chunk)
assert sys_sha.hexdigest() == "afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7"

print(f"INITRAMFS_AND_DTB_PASS: CPIO size={cpio_sz}, DTB initrd-end={exp_prop}, members={member_count}")
