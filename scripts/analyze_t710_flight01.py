#!/usr/bin/env python3
"""Comprehensive analysis script for Track T710 Flight-01 UART log."""

import json
import re
import sys
from pathlib import Path

WORKTREE = Path("/Users/Shared/wt-track710-sensorservice-wall-cure")
LOG_PATH = WORKTREE / "flights/flight-01/uart.log"

def analyze():
    if not LOG_PATH.exists():
        print(f"Log file not found: {LOG_PATH}")
        sys.exit(1)

    print(f"Analyzing {LOG_PATH}...")
    metrics = {
        "total_lines": 0,
        "freeing_kernel_memory": [],
        "init_first_stage": [],
        "vda_mount": [],
        "simplefb": [],
        "prime_cache_abort_count": 0,
        "composer_client_fail_count": 0,
        "boot_completed_count": 0,
        "queue_buffer_count": 0,
        "present_display_count": 0,
        "validate_display_count": 0,
        "system_server_starts": [],
        "system_server_pids": [],
        "watchdog_kills": [],
        "dummy_sensorservice_attempts": 0,
        "sensorservice_avc_denials": 0,
        "total_avc_denials": 0,
        "launcher3_fatal_exceptions": 0,
        "dexopt_lines": [],
    }

    ss_start_re = re.compile(r"I/system_server_start\(\s*(\d+)\):")
    watchdog_re = re.compile(r"W/Watchdog\(\s*(\d+)\): \*\*\* WATCHDOG KILLING SYSTEM PROCESS")

    with open(LOG_PATH, "r", encoding="latin1", newline="\n") as f:
        for idx, raw in enumerate(f, 1):
            metrics["total_lines"] += 1
            line = raw.strip()

            if "Freeing unused kernel memory" in line:
                metrics["freeing_kernel_memory"].append((idx, line))

            if "init: init first stage started!" in line:
                metrics["init_first_stage"].append((idx, line))

            if "__mount(source=/dev/block/vda,target=/system" in line:
                metrics["vda_mount"].append((idx, line))

            if "simple-framebuffer" in line or "simplefb registered" in line:
                metrics["simplefb"].append((idx, line))

            if "output buffer not gpu writeable" in line:
                metrics["prime_cache_abort_count"] += 1

            if "failed to create composer client" in line:
                metrics["composer_client_fail_count"] += 1

            if "sys.boot_completed=1" in line:
                metrics["boot_completed_count"] += 1

            if "queueBuffer" in line:
                metrics["queue_buffer_count"] += 1

            if "presentDisplay" in line:
                metrics["present_display_count"] += 1

            if "validateDisplay" in line:
                metrics["validate_display_count"] += 1

            m_ss = ss_start_re.search(line)
            if m_ss:
                pid = int(m_ss.group(1))
                metrics["system_server_starts"].append((idx, pid, line))
                if pid not in metrics["system_server_pids"]:
                    metrics["system_server_pids"].append(pid)

            m_wd = watchdog_re.search(line)
            if m_wd:
                pid = int(m_wd.group(1))
                metrics["watchdog_kills"].append((idx, pid, line))

            if "AServiceManager_addService returned" in line:
                metrics["dummy_sensorservice_attempts"] += 1

            if "avc:" in line or "avc:  denied" in line:
                metrics["total_avc_denials"] += 1
                if "name=sensorservice" in line:
                    metrics["sensorservice_avc_denials"] += 1

            if "FATAL EXCEPTION" in line and "launcher3" in line.lower():
                metrics["launcher3_fatal_exceptions"] += 1

            if "dex2oat" in line and "launcher3" in line.lower():
                metrics["dexopt_lines"].append((idx, line))

    print("\n================ T710 FLIGHT-01 ANALYSIS ================")
    print(f"Total lines: {metrics['total_lines']}")
    print(f"Freeing kernel memory: {metrics['freeing_kernel_memory']}")
    print(f"Init first stage: {metrics['init_first_stage']}")
    print(f"VDA mount: {metrics['vda_mount']}")
    print(f"SimpleFB: {metrics['simplefb']}")
    print(f"PrimeCache aborts ('output buffer not gpu writeable'): {metrics['prime_cache_abort_count']}")
    print(f"Composer client failures: {metrics['composer_client_fail_count']}")
    print(f"sys.boot_completed=1 count: {metrics['boot_completed_count']}")
    print(f"queueBuffer count: {metrics['queue_buffer_count']}")
    print(f"presentDisplay count: {metrics['present_display_count']}")
    print(f"validateDisplay count: {metrics['validate_display_count']}")
    print(f"System Server instances count: {len(metrics['system_server_pids'])} (PIDs: {metrics['system_server_pids']})")
    for s in metrics['system_server_starts']:
        print(f"  Line {s[0]}: PID {s[1]} -> {s[2]}")
    print(f"Watchdog kills count: {len(metrics['watchdog_kills'])}")
    for k in metrics['watchdog_kills']:
        print(f"  Line {k[0]}: PID {k[1]} -> {k[2]}")
    print(f"dummy_sensorservice addService attempts: {metrics['dummy_sensorservice_attempts']}")
    print(f"sensorservice SELinux denials: {metrics['sensorservice_avc_denials']}")
    print(f"Total avc denials: {metrics['total_avc_denials']}")
    print(f"Launcher3 FATAL EXCEPTIONs: {metrics['launcher3_fatal_exceptions']}")
    print(f"Launcher3 dex2oat lines: {len(metrics['dexopt_lines'])}")
    print("=========================================================\n")

    return metrics

if __name__ == "__main__":
    analyze()
