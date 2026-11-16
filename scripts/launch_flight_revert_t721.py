#!/usr/bin/env python3
"""Launch T721 scaffold-revert flight detached (600 s guest, 700 s host cap).
DECISIVE for PASS: new composer (t721-vendor.img) + ORIGINAL surfaceflinger
(t721-system-revert.img, scaffold REMOVED). Fence errors must stay 0 WITHOUT
the scaffold — that is the proof the retire fence is real.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T721_FLIGHT"] = "flight-revert"
env["T721_SYSTEM_IMG"] = str(WORKTREE / "guest-inputs/t721-system-revert.img")
env["T721_SYSTEM_IMG_SHA"] = "0ba6d6d353b406a7c0a3df340be60bde678ee215b5b8c3bee5ec7a7274d3e3d3"
env["T721_GUEST_TARGET"] = "600"
env["T721_HOST_CAP"] = "700"
env["T721_GDB_PORT"] = "1259"

log_path = WORKTREE / "supervisor_flight_revert.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-revert supervisor in background, PID {pid}")
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
