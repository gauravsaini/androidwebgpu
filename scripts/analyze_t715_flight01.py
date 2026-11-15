#!/usr/bin/env python3
"""Adjudication report for the Track T715 flight-01 UART.

Counts every quantity the brief's Step 3b/4 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
uart_path = WORKTREE / "flights/flight-01/uart.log"
if not uart_path.exists():
    print(f"FAIL: {uart_path} does not exist")
    sys.exit(1)

uart = uart_path.read_bytes().decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only, matching grep -n
lines = uart.split("\n")
if lines and lines[-1] == "":
    lines.pop()  # trailing newline: match grep -n / wc -l numbering
print(f"Total lines: {len(lines)}")

def numbered(pattern, label, cap=40):
    hits = [i + 1 for i, l in enumerate(lines) if re.search(pattern, l)]
    print(f"\n{label}: {len(hits)}")
    shown = hits if len(hits) <= cap else hits[:20] + hits[-20:]
    print("lines: " + ", ".join(map(str, shown)))
    if len(hits) > cap:
        print(f"({len(hits) - cap} middle lines omitted; count is exact)")
    return hits

# Watchdog kills (true kill lines) and system_server forks
kills = numbered(r"Watchdog:.*KILLING", "Watchdog KILLING lines")
forks = numbered(r"system_server_start \[", "system_server_start lines")
numbered(r"Entered the Android system server!", "system_server entered lines")
numbered(r"SensorService\.onBootPhase", "SensorService.onBootPhase lines")
numbered(r"FutureTask\.get", "FutureTask.get lines")

# installd / fixAppsData
numbered(r"installd not found", "installd not found lines")
numbered(r"installd", "ALL installd lines", cap=60)
numbered(r"fixAppsData", "ALL fixAppsData lines")
numbered(r"socket/installd", "ALL /dev/socket/installd lines")

# Display / graphics criteria
numbered(r"queueBuffer", "queueBuffer lines")
numbered(r"presentDisplay|present.*dispatch|Present.*success", "present dispatch lines")
numbered(r"Dexopt.*SKIPPED|dexopt.*skip", "Dexopt skipped lines")
numbered(r"com\.android\.launcher3", "Launcher3 lines", cap=60)
numbered(r"FATAL EXCEPTION", "FATAL EXCEPTION lines")
numbered(r"boot_completed", "boot_completed lines")
numbered(r"sys\.boot_completed", "sys.boot_completed lines")

# Structural holds
print(f"\nFailed to load subHal config file: {len(re.findall(r'Failed to load subHal config file', uart))}")
print(f"output buffer not gpu writeable: {len(re.findall(r'output buffer not gpu writeable', uart))}")
print(f"ISensors/default: {len(re.findall(r'ISensors/default', uart))}")
print(f"avc denied (sensorservice): {sum(1 for l in lines if 'avc:  denied' in l and 'sensorservice' in l)}")
print(f"Kernel Oops/panic: {len(re.findall(r'Kernel panic|Oops:|BUG:', uart))}")

# Guest time span (kernel timestamps)
times = [float(m.group(1)) for l in lines for m in [re.match(r"\[\s*([0-9]+\.[0-9]+)\]", l)] if m]
if times:
    print(f"\nGuest kernel time: first={times[0]}s last={times[-1]}s max={max(times)}s")
print("\n=== REPORT COMPLETE ===")
