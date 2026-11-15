#!/usr/bin/env python3
"""Launch T719 flight-01 detached (2400 s guest target, 2500 s host cap)."""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T719_FLIGHT"] = "flight-01"
env["T719_GUEST_TARGET"] = "2400"
env["T719_HOST_CAP"] = "2500"
env["T719_GDB_PORT"] = "1258"

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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t719.py"], env)
