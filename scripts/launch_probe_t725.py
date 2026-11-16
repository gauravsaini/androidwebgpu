#!/usr/bin/env python3
"""Launch T725 registration smoke detached (600 s guest target, 700 s host cap).

The smoke answers whether ANY added .rc file registers: the minimal
t725probe.rc (one service + two actions, marker T725_REG_OK written to
/dev/kmsg) is flown; the marker (or the service-start / start-command
lines) post-flood is the decisive evidence. Window: the on boot action
fires ~95 s guest; sys.boot_completed=1 lands ~498 s guest; the 600 s
guest target covers both triggers plus the T721 fence-path hold window.

Env overrides: T725_FLIGHT (default registration-smoke; re-runs MUST use
a new name, never overwrite a completed flight's folder),
T725_GUEST_TARGET, T725_HOST_CAP, T725_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""
import os
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

def main() -> None:
    os.chdir(WORKTREE)

    env = os.environ.copy()
    env.setdefault("T725_FLIGHT", "registration-smoke")
    env.setdefault("T725_GUEST_TARGET", "600")
    env.setdefault("T725_HOST_CAP", "700")
    env.setdefault("T725_GDB_PORT", "1258")

    flight = env["T725_FLIGHT"]
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

    os.execve(sys.executable, [sys.executable, "scripts/supervisor_t725.py"], env)

if __name__ == "__main__":
    main()
