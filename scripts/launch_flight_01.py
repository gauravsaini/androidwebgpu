#!/usr/bin/env python3
"""Launch Flight-01 on Route B Image for Track T704 detached."""

import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T704_FLIGHT"] = "flight-01"
env["T704_KERNEL"] = str(WORKTREE / "guest-inputs/routeB-Image")
env["T704_KERNEL_SHA"] = "3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3"
env["T704_HOST_CAP"] = "2500"
env["T704_GUEST_TARGET"] = "2400"
env["T704_GDB_PORT"] = "1246"

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
fd_out = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
os.dup2(fd_out, 1)
os.dup2(fd_out, 2)

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t704.py"], env)
