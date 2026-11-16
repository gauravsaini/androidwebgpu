#!/usr/bin/env python3
"""Launch T722 smoke flight detached (240 s guest target, 300 s host cap).

Smoke criterion (brief Step 3a): REJECT 0, GBA failures 0, >=1 nonzero
allocation, no new crash/regression, T721 fence evidence path intact
(probe DONE PASS). The EGL dup-fail lines are NOT expected in a 240 s
window (first dup-fail needs presenting activity well past boot); the
smoke adjudicates only the allocator / holds / regression prongs.
Scaffold REVERTED (revert system image) for every T722 flight.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T722_FLIGHT"] = "smoke"
env["T722_GUEST_TARGET"] = "240"
env["T722_HOST_CAP"] = "300"
env["T722_GDB_PORT"] = "1257"

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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t722.py"], env)
