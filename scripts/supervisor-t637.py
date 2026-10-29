#!/usr/bin/env python3
"""T637 supervisor: executes tools/run_t637_fix.py under detached supervisor with heartbeat and outcome reporting."""

import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys
import time

WORKTREE = Path("/Users/Shared/wt-track637-gralloc-mapper-fix")
HEARTBEAT = WORKTREE / "heartbeat.json"
OUTCOME = Path("/Users/Shared/outcome-t637.txt")
RUNNER = WORKTREE / "tools" / "run_t637_fix.py"


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def main() -> int:
    print(f"[{utc_now()}] Starting T637 supervisor for {RUNNER}")
    proc = subprocess.Popen([sys.executable, "-u", str(RUNNER)])
    start_time = time.monotonic()

    while True:
        rc = proc.poll()
        if rc is not None:
            print(f"[{utc_now()}] T637 runner process exited with code {rc}")
            return rc

        now = time.monotonic()
        if HEARTBEAT.exists():
            try:
                hb = json.loads(HEARTBEAT.read_text())
                hb["supervisor_uptime_sec"] = int(now - start_time)
                HEARTBEAT.write_text(json.dumps(hb, indent=2) + "\n")
            except Exception:
                pass

        time.sleep(5)


if __name__ == "__main__":
    sys.exit(main())
