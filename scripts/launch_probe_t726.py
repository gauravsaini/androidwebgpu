#!/usr/bin/env python3
"""Launch T726 probe flight detached (600 s guest target, 700 s host cap).

The flight re-flies the early-ramdisk probe with the proven trigger-file
shape: the service runs the probe binary on sys.boot_completed=1, and the
dual-trigger marker re-proves registration in the same flight. The probe's
own lines plus its DONE verdict are the EGL dispatch answer. Window: the
on boot action fires ~95 s guest; sys.boot_completed=1 lands ~498 s guest
(and the boot may run slow -- the preceding lane never reached it in 554 s);
the 600 s guest target covers both triggers plus the fence-path hold window.

Env overrides: T726_FLIGHT (default probe-flight; re-runs MUST use
a new name, never overwrite a completed flight's folder),
T726_GUEST_TARGET, T726_HOST_CAP, T726_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

def main() -> None:
    os.chdir(WORKTREE)

    env = os.environ.copy()
    env.setdefault("T726_FLIGHT", "probe-flight")
    env.setdefault("T726_GUEST_TARGET", "600")
    env.setdefault("T726_HOST_CAP", "700")
    env.setdefault("T726_GDB_PORT", "1258")

    flight = env["T726_FLIGHT"]
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

    os.execve(sys.executable, [sys.executable, "scripts/supervisor_t726.py"], env)

if __name__ == "__main__":
    main()
