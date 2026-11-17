#!/usr/bin/env python3
"""Launch T728 service flight detached (600 s guest target, 700 s host cap).

The flight discriminates the prior-lane service-start failure in one flight:
the identical property-action shape with the service command replaced by
a known-good binary (/system/bin/toybox sh -c "echo T728_SERVICE_OK >
/dev/kmsg"). If the toybox service runs (T728_SERVICE_OK in the UART),
the fault was the probe path; if it never starts although the action
marker fired, the fault is the start leg itself. Window: the on boot
action fires ~95 s guest; sys.boot_completed=1 lands ~498 s guest (and
the boot may run slow -- the preceding lane never reached it in 554 s);
the 600 s guest target covers both triggers plus the fence-path hold window.

Env overrides: T728_FLIGHT (default service-flight; re-runs MUST use
a new name, never overwrite a completed flight's folder),
T728_GUEST_TARGET, T728_HOST_CAP, T728_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

def main() -> None:
    os.chdir(WORKTREE)

    env = os.environ.copy()
    env.setdefault("T728_FLIGHT", "service-flight")
    env.setdefault("T728_GUEST_TARGET", "600")
    env.setdefault("T728_HOST_CAP", "700")
    env.setdefault("T728_GDB_PORT", "1259")

    flight = env["T728_FLIGHT"]
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

    os.execve(sys.executable, [sys.executable, "scripts/supervisor_t728.py"], env)

if __name__ == "__main__":
    main()
