#!/usr/bin/env python3
"""Launch T716 smoke flight detached (240 s guest target, 300 s host cap).

Expected outcome is INCONCLUSIVE by design: the composer first attempts
open(/dev/mem) at guest ~522 s in T715 (after boot_completed ~505 s), past
this window. The brief then requires ONE extended 600 s smoke
(launch_smoke_ext_t716.py) for adjudication.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T716_FLIGHT"] = "smoke"
env["T716_GUEST_TARGET"] = "240"
env["T716_HOST_CAP"] = "300"
env["T716_GDB_PORT"] = "1257"

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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t716.py"], env)
