#!/usr/bin/env python3
"""Launch T718 smoke flight detached (240 s guest target, 300 s host cap).

Decisive for the allocator wall: the first T628 REJECT in T716 landed at
guest ~104 s, inside this window. The composer /dev/mem mapped line is
NOT expected here (first open attempt at guest ~522 s in T715) — mapped
is adjudicated from flight-01, not from this smoke.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T718_FLIGHT"] = "smoke"
env["T718_GUEST_TARGET"] = "240"
env["T718_HOST_CAP"] = "300"
env["T718_GDB_PORT"] = "1257"

log_path = WORKTREE / "supervisor_smoke.log"

pid = os.fork()
if pid > 0:
    print(f"Launched smoke supervisor in background, PID {pid}")
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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t718.py"], env)
