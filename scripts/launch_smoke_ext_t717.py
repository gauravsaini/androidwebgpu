#!/usr/bin/env python3
"""Launch T717 EXTENDED smoke flight detached (600 s guest target, 660 s host cap).

Optional: only run if the 240 s smoke is ambiguous. Evidence goes to
flights/smoke-ext/ so the 240 s smoke evidence in flights/smoke/ is
preserved, never overwritten.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T717_FLIGHT"] = "smoke-ext"
env["T717_GUEST_TARGET"] = "600"
env["T717_HOST_CAP"] = "660"
env["T717_GDB_PORT"] = "1257"

log_path = WORKTREE / "supervisor_smoke_ext.log"

pid = os.fork()
if pid > 0:
    print(f"Launched extended-smoke supervisor in background, PID {pid}")
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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t717.py"], env)
