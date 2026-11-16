#!/usr/bin/env python3
"""Launch T724 probe flight detached (600 s guest target, 700 s host cap).

The probe flight answers the EGL dispatch question in-guest: the
init .rc service trigger runs /first_stage_ramdisk/t723-probe once sys.boot_completed=1, and the
probe's T723_PROBE lines (SP1 direct, SP2 META-EGL, SP3 control) are the
decisive evidence. Window: boot_completed lands ~90-150 s guest; the
600 s guest target also covers composer fence activity (T721 fence log)
and a RenderEngine dup-fail window comparable to T722's 600 s host run.

Env overrides: T724_FLIGHT (default probe-flight; re-runs MUST use a new
name, never overwrite a completed flight's folder), T724_GUEST_TARGET,
T724_HOST_CAP, T724_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

def main() -> None:
    os.chdir(WORKTREE)

    env = os.environ.copy()
    env.setdefault("T724_FLIGHT", "probe-flight")
    env.setdefault("T724_GUEST_TARGET", "600")
    env.setdefault("T724_HOST_CAP", "700")
    env.setdefault("T724_GDB_PORT", "1258")

    flight = env["T724_FLIGHT"]
    log_path = WORKTREE / f"supervisor_{flight}.log"

    pid = os.fork()
    if pid > 0:
        print(f"Launched {flight} supervisor in background, PID {pid}")
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

    os.execve(sys.executable, [sys.executable, "scripts/supervisor_t724.py"], env)

if __name__ == "__main__":
    main()
