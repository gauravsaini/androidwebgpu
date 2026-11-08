#!/usr/bin/env python3
"""Analysis script for T706 Flight-01 UART log."""

import json
import re
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track706-clean-gsi-hwc-hotplug")
LOG_PATH = WORKTREE / "flights/flight-01/uart.log.numbered.txt"

def analyze():
    if not LOG_PATH.exists():
        print(f"Log file not found: {LOG_PATH}")
        sys.exit(1)

    with open(LOG_PATH, "r", encoding="utf-8", errors="replace") as f:
        lines = f.readlines()

    total_lines = len(lines)
    print(f"Total lines in flight-01 log: {total_lines}")

    metrics = {
        "total_lines": total_lines,
        "vda_mount_lines": [],
        "freeing_kernel_memory_lines": [],
        "init_first_stage_lines": [],
        "fb0_simplefb_lines": [],
        "kernel_panic_lines": [],
        "zygote_sigsegv_artfield_count": 0,
        "zygote_sigsegv_lines": [],
        "surfaceflinger_hwc_abort_count": 0,
        "surfaceflinger_hwc_abort_lines": [],
        "hwc_binder_ioctl_neg22_count": 0,
        "system_server_fork_lines": [],
        "system_server_entered_lines": [],
        "boot_completed_lines": [],
        "queue_buffer_count": 0,
        "queue_buffer_lines": [],
        "present_display_count": 0,
        "present_display_lines": [],
        "validate_display_count": 0,
        "dexopt_launcher3_lines": [],
        "launcher3_fatal_exception_count": 0,
        "launcher3_fatal_lines": [],
    }

    for idx, raw in enumerate(lines, 1):
        line = raw.strip()

        if "__mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success" in line:
            metrics["vda_mount_lines"].append(line)

        if "Freeing unused kernel memory" in line:
            metrics["freeing_kernel_memory_lines"].append(line)

        if "init: init first stage started!" in line:
            metrics["init_first_stage_lines"].append(line)

        if "fb0: simplefb registered!" in line:
            metrics["fb0_simplefb_lines"].append(line)

        if "Kernel panic" in line or "Internal error: Oops" in line:
            metrics["kernel_panic_lines"].append(line)

        if "art::ArtField::GetDeclaringClass" in line:
            metrics["zygote_sigsegv_artfield_count"] += 1
            metrics["zygote_sigsegv_lines"].append(line)

        if "Initial display configuration failed: HWC did not hotplug" in line:
            metrics["surfaceflinger_hwc_abort_count"] += 1
            metrics["surfaceflinger_hwc_abort_lines"].append(line)

        if "ioctl 40046210" in line and "returned -22" in line:
            metrics["hwc_binder_ioctl_neg22_count"] += 1

        if "System server process" in line or "forkSystemServer" in line:
            metrics["system_server_fork_lines"].append(line)

        if "Entered the Android system server!" in line:
            metrics["system_server_entered_lines"].append(line)

        if "sys.boot_completed=1" in line or "boot_completed=1" in line:
            metrics["boot_completed_lines"].append(line)

        if "queueBuffer" in line:
            metrics["queue_buffer_count"] += 1
            if len(metrics["queue_buffer_lines"]) < 10:
                metrics["queue_buffer_lines"].append(line)

        if "presentDisplay" in line:
            metrics["present_display_count"] += 1
            if len(metrics["present_display_lines"]) < 10:
                metrics["present_display_lines"].append(line)

        if "validateDisplay" in line:
            metrics["validate_display_count"] += 1

        if "com.android.launcher3" in line and ("dexopt" in line.lower() or "dex2oat" in line.lower()):
            metrics["dexopt_launcher3_lines"].append(line)

        if "FATAL EXCEPTION" in line and ("launcher" in line.lower() or "dotrenderer" in line.lower()):
            metrics["launcher3_fatal_exception_count"] += 1
            metrics["launcher3_fatal_lines"].append(line)

    print("\n--- Key Metrics ---")
    print(f"vda mount: {len(metrics['vda_mount_lines'])} found")
    print(f"Freeing kernel memory: {len(metrics['freeing_kernel_memory_lines'])} found")
    print(f"init first stage: {len(metrics['init_first_stage_lines'])} found")
    print(f"fb0 simplefb: {len(metrics['fb0_simplefb_lines'])} found")
    print(f"Kernel panic: {len(metrics['kernel_panic_lines'])}")
    print(f"Zygote GetDeclaringClass SIGSEGV: {metrics['zygote_sigsegv_artfield_count']}")
    print(f"SurfaceFlinger HWC hotplug aborts: {metrics['surfaceflinger_hwc_abort_count']}")
    print(f"HWC binder ioctl 40046210 -22: {metrics['hwc_binder_ioctl_neg22_count']}")
    print(f"system_server fork: {len(metrics['system_server_fork_lines'])}")
    print(f"system_server entered: {len(metrics['system_server_entered_lines'])}")
    print(f"boot_completed: {len(metrics['boot_completed_lines'])}")
    print(f"queueBuffer: {metrics['queue_buffer_count']}")
    print(f"presentDisplay: {metrics['present_display_count']}")
    print(f"validateDisplay: {metrics['validate_display_count']}")
    print(f"Launcher3 fatal exceptions: {metrics['launcher3_fatal_exception_count']}")

    out_file = WORKTREE / "evidence" / "flight01_analysis.json"
    with open(out_file, "w") as f:
        json.dump(metrics, f, indent=2)
    print(f"\nSaved analysis to {out_file}")

if __name__ == "__main__":
    analyze()
