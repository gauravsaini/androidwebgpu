import re

UART_LOG = "evidence/t700-flight-01-uart.log"

def recount():
    with open(UART_LOG, "r", newline="\n", errors="replace") as f:
        lines = f.readlines()

    total_lines = len(lines)
    print(f"Total lines: {total_lines}")

    # AVC counts
    avc_total = 0
    avc_permissive_1 = 0
    avc_permissive_0 = 0

    # Watchdog counts
    watchdog_lines = []
    watchdog_kill_lines = []

    # Tombstones
    tombstone_written = []
    tombstoned_mentions = 0

    for idx, line in enumerate(lines, 1):
        if "avc:  denied" in line or "avc: denied" in line:
            avc_total += 1
            if "permissive=1" in line:
                avc_permissive_1 += 1
            elif "permissive=0" in line:
                avc_permissive_0 += 1

        if "tombstone" in line.lower():
            tombstoned_mentions += 1
            if "T637_TOMBSTONE_BEGIN" in line or "T637_TOMBSTONE_END" in line:
                tombstone_written.append((idx, line.strip()))

        if "Watchdog" in line or "watchdog" in line.lower():
            watchdog_lines.append((idx, line.strip()))
            if "WATCHDOG KILLING" in line or "GOODBYE!" in line:
                watchdog_kill_lines.append((idx, line.strip()))

    print("=== RE-COUNTS AUDIT ===")
    print(f"Total lines: {total_lines}")
    print(f"AVC Total: {avc_total}")
    print(f"AVC Permissive 1: {avc_permissive_1}")
    print(f"AVC Permissive 0: {avc_permissive_0}")
    print(f"Tombstone mentions (case-insensitive): {tombstoned_mentions}")
    print(f"Tombstones written markers: {len(tombstone_written)}")
    for tw in tombstone_written:
        print(f"  L{tw[0]}: {tw[1]}")
    print(f"Watchdog lines total: {len(watchdog_lines)}")
    print(f"Watchdog kill / goodbye lines: {len(watchdog_kill_lines)}")
    for wk in watchdog_kill_lines:
        print(f"  L{wk[0]}: {wk[1]}")

if __name__ == "__main__":
    recount()
