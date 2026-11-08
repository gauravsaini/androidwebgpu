#!/usr/bin/env python3
import os
import subprocess
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")
env = dict(os.environ)
env["T705_FLIGHT"] = "smoke"
env["T705_GUEST_TARGET"] = "120"
env["T705_HOST_CAP"] = "180"
env["T705_GDB_PORT"] = "1249"

cmd = [sys.executable, str(WORKTREE / "scripts/supervisor_t705.py")]
proc = subprocess.Popen(
    cmd,
    cwd=str(WORKTREE),
    env=env,
    stdin=subprocess.DEVNULL,
    stdout=open(WORKTREE / "flights/smoke-supervisor.log", "w", encoding="utf-8"),
    stderr=subprocess.STDOUT,
    start_new_session=True,
)
print(f"Smoke supervisor launched in background PID {proc.pid}")
