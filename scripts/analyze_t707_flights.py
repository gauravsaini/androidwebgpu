#!/usr/bin/env python3
"""
Track T707: Comprehensive Flight Analysis Script
Analyzes flight-01, flight-ab, and smoke flight UART logs for all required markers and metrics.
"""

import json
import re
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track707-composer-hotplug-dispatch")
F01_LOG = WORKTREE / "flights/flight-01/uart.log"
FAB_LOG = WORKTREE / "flights/flight-ab/uart.log"
SMOKE_LOG = WORKTREE / "flights/smoke/uart.log"

def analyze_flight01():
    with open(F01_LOG, "r", encoding="latin1") as f:
        lines = f.readlines()

    res = {
        "total_lines": len(lines),
        "freeing_mem": [],
        "init_first_stage": [],
        "simplefb": [],
        "vda_mount": [],
        "hotplug_abort": [],
        "output_buffer_abort": [],
        "failed_composer_client": [],
        "all_aborts": [],
        "tombstones": [],
        "zygote_sigsegv": [],
        "fork_system_server": [],
        "system_server_entered": [],
        "boot_completed": [],
        "queue_buffer": [],
        "present_display": [],
        "validate_display": [],
        "launcher3_fatal": [],
        "binder_22": 0,
    }

    tombstone_re = re.compile(r"T637_TOMBSTONE_DETECTED name=(tombstone_\d+)")

    for idx, line in enumerate(lines, start=1):
        if "Freeing unused kernel memory" in line:
            res["freeing_mem"].append((idx, line.strip()))
        if "init first stage started" in line:
            res["init_first_stage"].append((idx, line.strip()))
        if "fb0: simplefb registered" in line:
            res["simplefb"].append((idx, line.strip()))
        if "__mount" in line and "vda" in line and "system" in line:
            res["vda_mount"].append((idx, line.strip()))
        if "Initial display configuration failed: HWC did not hotplug" in line:
            res["hotplug_abort"].append((idx, line.strip()))
        if "output buffer not gpu writeable" in line:
            res["output_buffer_abort"].append((idx, line.strip()))
        if "failed to create composer client" in line:
            res["failed_composer_client"].append((idx, line.strip()))
        if "Abort message:" in line:
            res["all_aborts"].append((idx, line.strip()))
        if "art::ArtField::GetDeclaringClass" in line:
            res["zygote_sigsegv"].append((idx, line.strip()))
        if "System server process" in line and "has been created" in line:
            res["fork_system_server"].append((idx, line.strip()))
        if "Entered the Android system server" in line:
            res["system_server_entered"].append((idx, line.strip()))
        if "sys.boot_completed=1" in line:
            res["boot_completed"].append((idx, line.strip()))
        if "queueBuffer" in line:
            res["queue_buffer"].append((idx, line.strip()))
        if "presentDisplay" in line:
            res["present_display"].append((idx, line.strip()))
        if "validateDisplay" in line:
            res["validate_display"].append((idx, line.strip()))
        if "FATAL EXCEPTION" in line and "launcher3" in line.lower():
            res["launcher3_fatal"].append((idx, line.strip()))
        if "ioctl 40046210" in line and "returned -22" in line:
            res["binder_22"] += 1

        m = tombstone_re.search(line)
        if m:
            res["tombstones"].append((idx, m.group(1), line.strip()))

    return res

def analyze_flightab():
    with open(FAB_LOG, "r", encoding="latin1") as f:
        lines = f.readlines()

    res = {
        "total_lines": len(lines),
        "freeing_mem": [],
        "init_first_stage": [],
        "partition_not_found": [],
        "fatal_reboot": [],
        "reboot_command": [],
    }

    for idx, line in enumerate(lines, start=1):
        if "Freeing unused kernel memory" in line:
            res["freeing_mem"].append((idx, line.strip()))
        if "init first stage started" in line:
            res["init_first_stage"].append((idx, line.strip()))
        if "partition(s) not found after polling timeout: vda" in line:
            res["partition_not_found"].append((idx, line.strip()))
        if "InitFatalReboot: signal 6" in line:
            res["fatal_reboot"].append((idx, line.strip()))
        if "Restarting system with command" in line:
            res["reboot_command"].append((idx, line.strip()))

    return res

if __name__ == "__main__":
    f01 = analyze_flight01()
    fab = analyze_flightab()

    print("=== FLIGHT-01 ANALYSIS ===")
    print(f"Total lines: {f01['total_lines']}")
    print(f"Freeing memory: {f01['freeing_mem']}")
    print(f"Init first stage: {f01['init_first_stage']}")
    print(f"Simplefb: {f01['simplefb']}")
    print(f"VDA mount: {f01['vda_mount']}")
    print(f"HWC hotplug aborts (exp 0): {len(f01['hotplug_abort'])}")
    print(f"Output buffer not gpu writeable aborts: {len(f01['output_buffer_abort'])} -> {f01['output_buffer_abort']}")
    print(f"Failed to create composer client aborts: {len(f01['failed_composer_client'])}")
    print(f"Total Abort messages: {len(f01['all_aborts'])}")
    print(f"Total tombstones: {len(f01['tombstones'])}")
    print(f"First tombstone: {f01['tombstones'][0] if f01['tombstones'] else None}")
    print(f"Last tombstone: {f01['tombstones'][-1] if f01['tombstones'] else None}")
    print(f"Zygote GetDeclaringClass SIGSEGV: {len(f01['zygote_sigsegv'])}")
    print(f"forkSystemServer: {len(f01['fork_system_server'])}")
    print(f"sys.boot_completed=1: {len(f01['boot_completed'])}")
    print(f"queueBuffer: {len(f01['queue_buffer'])}")
    print(f"presentDisplay: {len(f01['present_display'])}")
    print(f"validateDisplay: {len(f01['validate_display'])}")
    print(f"Launcher3 FATAL EXCEPTION: {len(f01['launcher3_fatal'])}")
    print(f"Binder 40046210 -22: {f01['binder_22']}")

    print("\n=== FLIGHT-AB ANALYSIS ===")
    print(f"Total lines: {fab['total_lines']}")
    print(f"Freeing memory: {fab['freeing_mem']}")
    print(f"Init first stage: {fab['init_first_stage']}")
    print(f"Partition not found: {fab['partition_not_found']}")
    print(f"Fatal reboot: {fab['fatal_reboot']}")
    print(f"Reboot command: {fab['reboot_command']}")

    out_file = WORKTREE / "evidence/analysis_t707.json"
    out_file.parent.mkdir(parents=True, exist_ok=True)
    with open(out_file, "w") as fout:
        json.dump({"flight_01": f01, "flight_ab": fab}, fout, indent=2)
    print(f"\nAnalysis saved to {out_file}")
