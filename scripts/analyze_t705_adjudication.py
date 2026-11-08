#!/usr/bin/env python3
"""Adjudication analysis for T705 flights:
Checks:
- sys.boot_completed
- system_server fork & tombstone count
- SurfaceFlinger markers: queueBuffer, presentDisplay, validateDisplay, presentOrValidate, executeCommands
- Dexopt status for Launcher3
- Launcher3 / Zygote fatal exceptions or SIGSEGVs
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track705-virtio-blk-kernel-rebuild")

def analyze_uart(log_path: Path) -> dict:
    if not log_path.exists():
        return {"error": f"Log missing: {log_path}"}

    metrics = {
        "boot_completed": False,
        "boot_completed_line": None,
        "system_server_forked": False,
        "system_server_pid": None,
        "system_server_lines": [],
        "zygote_sigsegv_count": 0,
        "odrefresh_exit_codes": [],
        "queue_buffer_count": 0,
        "present_display_count": 0,
        "validate_display_count": 0,
        "present_or_validate_count": 0,
        "execute_commands_count": 0,
        "launcher3_fatal_exceptions": 0,
        "launcher3_fatal_lines": [],
        "launcher3_dexopt_lines": [],
        "vda_mount_lines": [],
        "tombstones": 0,
    }

    with open(log_path, "r", encoding="latin1", errors="replace") as f:
        for idx, line in enumerate(f, start=1):
            if "sys.boot_completed=1" in line or "boot_completed=1" in line:
                metrics["boot_completed"] = True
                metrics["boot_completed_line"] = f"{idx}: {line.strip()}"
            if "Entered system_server" in line or "system_server" in line and "forked" in line.lower():
                metrics["system_server_forked"] = True
                metrics["system_server_lines"].append(f"{idx}: {line.strip()}")
            if "Zygote" in line and "SIGSEGV" in line:
                metrics["zygote_sigsegv_count"] += 1
            if "odrefresh" in line and "exit" in line:
                metrics["odrefresh_exit_codes"].append(f"{idx}: {line.strip()}")
            if "queueBuffer" in line:
                metrics["queue_buffer_count"] += 1
            if "presentDisplay" in line:
                metrics["present_display_count"] += 1
            if "validateDisplay" in line:
                metrics["validate_display_count"] += 1
            if "presentOrValidate" in line:
                metrics["present_or_validate_count"] += 1
            if "executeCommands" in line:
                metrics["execute_commands_count"] += 1
            if "FATAL EXCEPTION" in line and ("launcher" in line.lower() or "launcher3" in line.lower()):
                metrics["launcher3_fatal_exceptions"] += 1
                metrics["launcher3_fatal_lines"].append(f"{idx}: {line.strip()}")
            if "dexopt" in line.lower() and "launcher" in line.lower():
                metrics["launcher3_dexopt_lines"].append(f"{idx}: {line.strip()}")
            if "__mount(source=/dev/block/vda" in line:
                metrics["vda_mount_lines"].append(f"{idx}: {line.strip()}")
            if "tombstone" in line.lower() and "pid" in line.lower():
                metrics["tombstones"] += 1

    return metrics

def main():
    flight_name = sys.argv[1] if len(sys.argv) > 1 else "flight-01"
    log_path = WORKTREE / "flights" / flight_name / "uart.log"
    print(f"Analyzing {log_path}...")
    metrics = analyze_uart(log_path)
    print(json.dumps(metrics, indent=2))

if __name__ == "__main__":
    main()
