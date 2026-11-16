#!/usr/bin/env python3
"""Launch T722 flight-01 detached (2400 s guest target, 2500 s host cap).
Fixed EGL stack (t722 CPIO/DTB/vendor) + REVERT system image (original
surfaceflinger, scaffold REVERTED). PASS requires the dup-fail count DOWN
versus the T721 baseline on a normalized presenting-time basis, T721 fence
log present, Invalid present fence 0 WITHOUT the scaffold, allocation
holds intact, sys.boot_completed present, Launcher3 dexopt NOT SKIPPED,
EPERM 0, Watchdog kills 0, no new crash, no fd leak.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T722_FLIGHT"] = "flight-01"
env["T722_GUEST_TARGET"] = "2400"
env["T722_HOST_CAP"] = "2500"
env["T722_GDB_PORT"] = "1258"

log_path = WORKTREE / "supervisor_flight01.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-01 supervisor in background, PID {pid}")
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
