#!/usr/bin/env python3
"""Launch T722 A/B control detached (700 s guest, 800 s host cap).
T721 stack UNCHANGED (t721 CPIO embedding the ORIGINAL EGL stub vendor
image, t721 DTB, t721 vendor file, revert system image) on the T720
kernel. The control stack matches the fixed flight's stack except the
EGL change (the guest consumes the CPIO-embedded vendor image).

700 s (not 240 s): the first present lands ~565 s guest and the first
dup-fail needs presenting activity, so the control must cover the
presenting window for dup-fail lines to appear at the T721 rate; a 240 s
control ends before any present and is vacuous (T721 lesson). Expected:
dup-fail lines present at the T721 rate, T721 fence evidence intact.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T722_FLIGHT"] = "flight-ab"
env["T722_BOOT_IMAGE"] = str(WORKTREE / "guest-inputs/t721-fix.cpio.gz")
env["T722_BOOT_IMAGE_SHA"] = "6a302597c19156b99b3b07a8c8a33ca2ef9d823af9aa1008a5da67e822a018f1"
env["T722_DTB"] = str(WORKTREE / "guest-inputs/t721-modified.dtb")
env["T722_DTB_SHA"] = "91b45a10ea3f33edc36b8a945230ea7873b7b9ffe91bbff7f23cb34628ee8bf4"
env["T722_VENDOR_IMG"] = str(WORKTREE / "guest-inputs/t721-vendor.img")
env["T722_VENDOR_IMG_SHA"] = "6bf3e2e0ebbc7dc06031ad64bfaae1acb7fe207e1453e297af6639bbb215148b"
env["T722_SYSTEM_IMG"] = str(WORKTREE / "guest-inputs/t721-system-revert.img")
env["T722_SYSTEM_IMG_SHA"] = "0ba6d6d353b406a7c0a3df340be60bde678ee215b5b8c3bee5ec7a7274d3e3d3"
env["T722_GUEST_TARGET"] = "700"
env["T722_HOST_CAP"] = "800"
env["T722_GDB_PORT"] = "1260"

log_path = WORKTREE / "supervisor_flight_ab.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-ab supervisor in background, PID {pid}")
    sys.exit(0)

os.setsid()

pid2 = os.fork()
if pid2 > 0:
    sys.exit(0)

fd_in = os.open("/dev/null", os.O_RDONLY)
os.dup2(fd_in, 0)
fd_out = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
os.dup2(fd_out, 1)
os.dup2(fd_out, 2)

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t722.py"], env)
