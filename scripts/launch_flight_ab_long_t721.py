#!/usr/bin/env python3
"""Launch T721 A/B control (LONG) detached (700 s guest, 800 s host cap).
OLD composer (T720 CPIO embedding t718-vendor.img, original
setPresentFence(-1)) + REVERT system image (original surfaceflinger,
scaffold REMOVED) on the T720 kernel. The old composer is selected via
the CPIO/DTB overrides (the guest consumes the CPIO-embedded vendor
image, not the supervisor's vendor file); the vendor-file override keeps
the verified file matching the embedded copy.

700 s (not 240 s): the first present lands ~565 s guest, so the control
must cover the presenting window (~135 s) for fence errors to RETURN
(T718-like counts) — a 240 s control ends before any present and is
vacuous. Expected: NO `T721 fd=` log line AND fence errors > 0,
attributing the revert flight's 0 errors to the NEW composer.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T721_FLIGHT"] = "flight-ab"
env["T721_BOOT_IMAGE"] = str(WORKTREE / "guest-inputs/t720-fix.cpio.gz")
env["T721_BOOT_IMAGE_SHA"] = "848a470fa1241ba05e412c70158407f1b0dbdc593b29d1ce0dc16596d5c9657a"
env["T721_DTB"] = str(WORKTREE / "guest-inputs/t720-modified.dtb")
env["T721_DTB_SHA"] = "3ff2bb8ec704d6df555c333ef13ce157d02e7d5b7e872768a1f21776ad90a9b2"
env["T721_VENDOR_IMG"] = str(WORKTREE / "guest-inputs/t718-vendor.img")
env["T721_VENDOR_IMG_SHA"] = "bac3cdc112dc6c3257c016fb4f600c46f43b9372f3d2f2ebb9b7d79fd05daeca"
env["T721_SYSTEM_IMG"] = str(WORKTREE / "guest-inputs/t721-system-revert.img")
env["T721_SYSTEM_IMG_SHA"] = "0ba6d6d353b406a7c0a3df340be60bde678ee215b5b8c3bee5ec7a7274d3e3d3"
env["T721_GUEST_TARGET"] = "700"
env["T721_HOST_CAP"] = "800"
env["T721_GDB_PORT"] = "1260"

log_path = WORKTREE / "supervisor_flight_ab.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-ab (long) supervisor in background, PID {pid}")
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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t721.py"], env)
