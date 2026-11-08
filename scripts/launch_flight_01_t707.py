#!/usr/bin/env python3
"""Launch Flight-01 on Rebuilt Route B Image for Track T707 detached."""

import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T707_FLIGHT"] = "flight-01"
env["T707_KERNEL"] = str(WORKTREE / "guest-inputs/t705-Image")
env["T707_KERNEL_SHA"] = "53d9563e1c1bae4be4cab2f62ab1bf6331445bf0e629c66b3fa43114988e2859"
env["T707_HOST_CAP"] = "2500"
env["T707_GUEST_TARGET"] = "2400"
env["T707_GDB_PORT"] = "1252"

log_path = WORKTREE / "supervisor_flight01.log"

pid = os.fork()
if pid > 0:
    print(f"Launched flight-01 supervisor in background, PID {pid}")
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
