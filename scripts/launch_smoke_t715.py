#!/usr/bin/env python3
"""Launch T715 smoke flight detached (240 s guest target, 300 s host cap).

Variance vs brief heading (120 s): the brief's own hard smoke criterion
(fixAppsDataOnBoot COMPLETION line, first observable at guest ~158 s in
T714; system_server fork at ~150 s) is unobservable in a 120 s window.
240 s matches the actual T714 smoke window (274.8 s guest) and is the
minimum window that can observe the criterion. Recorded in
progress-t715.txt and findings-t715.md.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T715_FLIGHT"] = "smoke"
env["T715_GUEST_TARGET"] = "240"
env["T715_HOST_CAP"] = "300"
env["T715_GDB_PORT"] = "1257"

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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t715.py"], env)
