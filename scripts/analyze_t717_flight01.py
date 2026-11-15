#!/usr/bin/env python3
"""Adjudication report for the Track T717 flight-01 UART.

Counts every quantity the brief's Step 3b/4 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.

Criteria: nonzero allocation (0 T628 REJECT + 0 GraphicBufferAllocator
failures with the shim exercised) AND queueBuffer > 0 AND first present
dispatch (guest-origin evidence only) AND the T716 holds intact (EPERM 0,
mapped >= 1, Dexopt NOT SKIPPED for com.android.launcher3) AND Launcher3
FATAL EXCEPTION 0 — or every Launcher3 FATAL proven DeadSystemException
collateral of a system_server death whose crash file names the GateKeeper
NPE. Genuine composer-path FATALs (e.g. Couldn't
removeRegionSamplingListener) must be 0. getDisplayAttribute failures are
TOLERATED (T717 Step 2: the display forms from attributes 1-5).
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

def dump_full(pattern, label, cap=80):
    hits = [(i + 1, l) for i, l in enumerate(lines) if re.search(pattern, l)]
    print(f"\n--- {label}: {len(hits)} line(s) ---")
    shown = hits if len(hits) <= cap else hits[:40] + hits[-40:]
    for n, l in shown:
        print(f"L{n}: {l.rstrip()}")
    if len(hits) > cap:
        print(f"... ({len(hits) - cap} middle lines omitted; count is exact)")
    return len(hits)

# T717 allocator fix evidence
dump_full(r"T628_ALLOCATOR", "ALL T628_ALLOCATOR lines (full text: novel success lines visible here)", cap=120)
numbered(r"REJECT method=allocate2", "T628 REJECT allocate2 lines (brief requires 0)")
numbered(r"REGISTERED name=android\.hardware\.graphics\.allocator", "T628 REGISTERED lines")
numbered(r"CALL method=getIMapperLibrarySuffix", "getIMapperLibrarySuffix CALL lines")
dump_full(r"GraphicBufferAllocator.*Failed to allocate", "GraphicBufferAllocator failure lines (brief requires 0)", cap=80)
dump_full(r"allocateBuffers: failed to allocate buffer", "allocateBuffers failure lines (brief requires 0)", cap=60)
numbered(r"getDisplayAttribute failed", "getDisplayAttribute failed lines (TOLERATED, report only)")
dump_full(r"DisplayDeviceInfo", "DisplayDeviceInfo lines (display formed)", cap=20)
numbered(r"T637_MAPPER", "T637_MAPPER lines")
dump_full(r"GraphicBufferAllocator", "ALL GraphicBufferAllocator lines", cap=80)

# T716 composer fix evidence (holds)
numbered(r"simplefb: open /dev/mem failed", "EPERM lines (brief requires 0)")
numbered(r"simplefb: mapped", "simplefb: mapped lines (brief requires >=1)")
numbered(r"/dev/mem", "ALL /dev/mem lines")
numbered(r"vendor\.hwc-2-1", "vendor.hwc-2-1 lines")

# Watchdog kills (true kill lines) and system_server forks
kills = numbered(r"Watchdog:.*KILLING", "Watchdog KILLING lines")
forks = numbered(r"system_server_start\(", "system_server_start lines")
numbered(r"Entered the Android system server!", "system_server entered lines")
numbered(r"SensorService took .* in onBootPhase", "SensorService onBootPhase RETURN lines")
numbered(r"onBootPhase", "ALL onBootPhase lines", cap=60)
numbered(r"FutureTask\.get", "FutureTask.get lines")

# installd / fixAppsData (T715 holds)
numbered(r"installd not found", "installd not found lines")
numbered(r"installd firing up|Command 'start installd'|service 'installd'", "installd STARTED lines")
numbered(r"installd", "ALL installd lines", cap=60)
numbered(r"fixAppsData", "ALL fixAppsData lines (0 is healthy per T715 findings)")
numbered(r"socket/installd", "ALL /dev/socket/installd lines")

# Display / graphics criteria
numbered(r"queueBuffer", "queueBuffer lines")
numbered(r"presentDisplay", "presentDisplay lines")
numbered(r"present.*dispatch|Present.*success", "present dispatch lines")
numbered(r"Dexopt.*SKIPPED|dexopt.*skip", "Dexopt skipped lines")
dex_hits = numbered(r"Dexopt result: \[packageName = com\.android\.launcher3\]", "Dexopt result x Launcher3 lines")
for n in dex_hits:
    m = re.search(r"status=(\w+)", lines[n - 1])
    print(f"  L{n}: Launcher3 dexopt status={m.group(1) if m else 'UNKNOWN'}")
numbered(r"Dexopt result: \[packageName = com\.android\.launcher3\].*status=SKIPPED", "Launcher3 dexopt SKIPPED lines (brief requires 0)")
numbered(r"com\.android\.launcher3", "Launcher3 lines", cap=60)
numbered(r"FATAL EXCEPTION", "FATAL EXCEPTION lines")
numbered(r"DeadSystemException", "DeadSystemException lines")
numbered(r"GateKeeper|LockSettingsService", "GateKeeper / LockSettingsService lines")
numbered(r"removeRegionSamplingListener", "removeRegionSamplingListener lines")
numbered(r"boot_completed", "boot_completed lines")
numbered(r"sys\.boot_completed", "sys.boot_completed lines")

# FATAL lines with following context (exception class is on following lines)
fatal_idx = [i for i, l in enumerate(lines) if "FATAL EXCEPTION" in l]
print(f"\nFATAL EXCEPTION contexts ({len(fatal_idx)}):")
for i in fatal_idx[:40]:
    print(f"L{i + 1}: {lines[i].rstrip()}")
    for j in range(i + 1, min(i + 4, len(lines))):
        print(f"  L{j + 1}: {lines[j].rstrip()[:200]}")
if len(fatal_idx) > 40:
    print(f"({len(fatal_idx) - 40} further FATAL contexts omitted; count is exact)")

# Structural holds
print(f"\nFailed to load subHal config file: {len(re.findall(r'Failed to load subHal config file', uart))}")
print(f"output buffer not gpu writeable: {len(re.findall(r'output buffer not gpu writeable', uart))}")
print(f"ISensors/default: {len(re.findall(r'ISensors/default', uart))}")
print(f"avc:  denied (two spaces) total: {sum(1 for l in lines if 'avc:  denied' in l)}")
print(f"avc:  denied for /dev/mem: {sum(1 for l in lines if 'avc:  denied' in l and '/dev/mem' in l)}")
print(f"avc denied (sensorservice): {sum(1 for l in lines if 'avc:  denied' in l and 'sensorservice' in l)}")
print(f"Kernel Oops/panic: {len(re.findall(r'Kernel panic|Oops:|BUG:', uart))}")
print(f"RAMDISK incomplete write: {len(re.findall(r'RAMDISK: incomplete write', uart))}")

# Guest time span (kernel timestamps)
times = [float(m.group(1)) for l in lines for m in [re.match(r"\[\s*([0-9]+\.[0-9]+)\]", l)] if m]
if times:
    print(f"\nGuest kernel time: first={times[0]}s last={times[-1]}s max={max(times)}s")
print("\n=== REPORT COMPLETE ===")
