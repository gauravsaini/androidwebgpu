#!/usr/bin/env python3
"""Launch T721 smoke flight detached (240 s guest target, 300 s host cap).

Smoke criterion (brief Step 3a): REJECT 0, GBA failures 0, >=1 nonzero
allocation, no regression, composer fence-creation evidence appears
(the one-time `T721 fd=` log line from the patched composer). NOTE on
timing: present() runs when SF first presents (boot animation); if the
240 s window ends before the first present, the fence evidence is
deferred to flight-01 and the smoke adjudicates only the allocator /
holds / regression prongs (T719 section 3.5 observable-proxy rule).
The composer /dev/mem mapped line is adjudicated from flight-01, not
from this smoke (first open attempt at guest ~522 s in T715).
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
os.chdir(WORKTREE)

env = os.environ.copy()
env["T721_FLIGHT"] = "smoke"
env["T721_GUEST_TARGET"] = "240"
env["T721_HOST_CAP"] = "300"
env["T721_GDB_PORT"] = "1257"

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

os.execve(sys.executable, [sys.executable, "scripts/supervisor_t721.py"], env)
