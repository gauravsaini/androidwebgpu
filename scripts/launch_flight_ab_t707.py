#!/usr/bin/env python3
"""Launch Flight-ab (canonical kernel control) for Track T707 detached."""

import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T707_FLIGHT"] = "flight-ab"
env["T707_KERNEL"] = str(WORKTREE / "guest-inputs/canonical-Image")
env["T707_KERNEL_SHA"] = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
env["T707_HOST_CAP"] = "300"
env["T707_GUEST_TARGET"] = "240"
env["T707_GDB_PORT"] = "1253"

log_path = WORKTREE / "supervisor_flightab.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-ab supervisor in background, PID {pid}")
    sys.exit(0)

os.setsid()

# Second fork to completely detach from terminal
pid2 = os.fork()
if pid2 > 0:
    sys.exit(0)

fd_in = os.open("/dev/null", os.O_RDONLY)
os.dup2(fd_in, 0)
fd_out = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
os.dup2(fd_out, 1)
os.dup2(fd_out, 2)

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t707.py"], env)
