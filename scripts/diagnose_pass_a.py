import re
import sys

UART_LOG = "evidence/t700-flight-01-uart.log"

def analyze():
    with open(UART_LOG, "r", encoding="utf-8", errors="replace") as f:
        lines = f.readlines()

    print(f"Total lines in {UART_LOG}: {len(lines)}")

    # 1. System server forks
    # Patterns:
    # "System server process <PID> has been created"
    # "Entered the Android system server!"
    pids = [9301, 12708, 14487]

    fork_lines = {}
    enter_lines = {}
    goodbye_lines = {}
    
    # Track watcher lines
    boot_state_lines = []

    # Counts
    avc_total = 0
    avc_permissive_1 = 0
    avc_permissive_0 = 0
    tombstone_files = []
    tombstoned_mentions = 0
    watchdog_lines = []

    for idx, line in enumerate(lines, 1):
        if "avc:  denied" in line or "avc: denied" in line:
            avc_total += 1
            if "permissive=1" in line:
                avc_permissive_1 += 1
            elif "permissive=0" in line:
                avc_permissive_0 += 1

        if "tombstone" in line.lower():
            tombstoned_mentions += 1
            if "/data/tombstones/tombstone_" in line:
                tombstone_files.append((idx, line.strip()))

        if "Watchdog" in line:
            watchdog_lines.append((idx, line.strip()))

        if "T642_BOOT_STATE" in line:
            boot_state_lines.append((idx, line.strip()))

        for pid in pids:
            if f"System server process {pid} has been created" in line:
                fork_lines[pid] = (idx, line.strip())
            if f"Entered the Android system server!" in line and f" {pid} " in line:
                enter_lines[pid] = (idx, line.strip())
            if "GOODBYE!" in line and f" {pid} " in line:
                if pid not in goodbye_lines:
                    goodbye_lines[pid] = []
                goodbye_lines[pid].append((idx, line.strip()))

    print("\n=== SYSTEM_SERVER INSTANCES ===")
    for pid in pids:
        print(f"\n--- PID {pid} ---")
        if pid in fork_lines:
            print(f"Fork: L{fork_lines[pid][0]}: {fork_lines[pid][1]}")
        else:
            print(f"Fork: NOT FOUND")
        if pid in enter_lines:
            print(f"Enter: L{enter_lines[pid][0]}: {enter_lines[pid][1]}")
        else:
            print(f"Enter: NOT FOUND")
        if pid in goodbye_lines:
            for g in goodbye_lines[pid]:
                print(f"Goodbye: L{g[0]}: {g[1]}")
        else:
            print(f"Goodbye: NONE (survived or killed silently)")

    print("\n=== RE-COUNTS ===")
    print(f"AVC Total: {avc_total} (permissive=1: {avc_permissive_1}, permissive=0: {avc_permissive_0})")
    print(f"Tombstone mentions: {tombstoned_mentions}")
    print(f"Tombstone files: {len(tombstone_files)}")
    for t in tombstone_files:
        print(f"  L{t[0]}: {t[1]}")
    print(f"Watchdog lines count: {len(watchdog_lines)}")
    for w in watchdog_lines[:20]:
        print(f"  L{w[0]}: {w[1]}")

if __name__ == "__main__":
    analyze()
