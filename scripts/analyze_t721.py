#!/usr/bin/env python3
"""Adjudication report for a Track T721 flight UART.

Usage: analyze_t721.py [flight-dir] [label]   (defaults: flight-01, T721)

Counts every quantity the brief's Step 3/4 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.

Criteria (observable proxy, T719 findings section 3.5): the composer
retire fence is proven by (a) the one-time `T721 fd=` guest log line
from the composer process, (b) the scaffold-revert flight holding fence
errors at 0 WITHOUT the scaffold (DECISIVE), (c) the A/B control
attributing the effect to the new composer. queueBuffer and present
dispatch are NOT criteria (unobservable on success paths, T719 3.5).
getDisplayAttribute failures are TOLERATED (T717 Step 2).
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
FLIGHT = sys.argv[1] if len(sys.argv) > 1 else "flight-01"
LABEL = sys.argv[2] if len(sys.argv) > 2 else "T721"
uart_path = WORKTREE / "flights" / FLIGHT / "uart.log"
if not uart_path.exists():
    print(f"FAIL: {uart_path} does not exist")
    sys.exit(1)

uart = uart_path.read_bytes().decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only, matching grep -n
lines = uart.split("\n")
if lines and lines[-1] == "":
    lines.pop()  # trailing newline: match grep -n / wc -l numbering
print(f"Total lines: {len(lines)} (flight {FLIGHT}, label {LABEL})")

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

# T721 retire-fence evidence (the decisive proxy: guest-origin composer proof)
dump_full(r"T721 fd=", "T721 FENCE LOG lines (one-time trampoline log; full text)", cap=10)
numbered(r"T721 fd=", "T721 fence-log lines (flight-01/revert require >=1; A/B requires 0)")
dump_full(r"debugfs|sw_sync", "ALL debugfs / sw_sync lines", cap=40)
numbered(r"T720_PROBE DONE status=PASS", "T720_PROBE DONE status=PASS lines (kernel/CPIO unchanged: >=1)")
numbered(r"T720_PROBE DONE status=FAIL", "T720_PROBE DONE status=FAIL lines (requires 0)")

# T718/T719 allocator fix evidence
dump_full(r"T628_ALLOCATOR", "ALL T628_ALLOCATOR lines (full text: novel success lines visible here)", cap=120)
numbered(r"REJECT method=allocate2", "T628 REJECT allocate2 lines (brief requires 0)")
numbered(r"CALL method=allocate2", "CALL allocate2 lines (brief requires >=1)")
numbered(r"REGISTERED name=android\.hardware\.graphics\.allocator", "T628 REGISTERED lines")
numbered(r"CALL method=getIMapperLibrarySuffix", "getIMapperLibrarySuffix CALL lines")
dump_full(r"GraphicBufferAllocator.*Failed to allocate", "GraphicBufferAllocator failure lines (brief requires 0)", cap=80)
dump_full(r"allocateBuffers: failed to allocate buffer", "allocateBuffers failure lines (brief requires 0)", cap=60)
numbered(r"getDisplayAttribute failed", "getDisplayAttribute failed lines (TOLERATED per T717 Step 2, report only)")
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

# Fence-scaffold evidence (DECISIVE in the revert flight: 0 WITHOUT scaffold;
# T718 flight-01 with scaffold absent: 2241. Scaffold-IN flights predict 0.)
numbered(r"trackPendingFrame: Invalid present fence", "Invalid present fence lines (revert flight requires 0; T718: 2241)")
numbered(r"failed to dup EGL native fence sync", "RenderEngine EGL dup-fail lines (T718 flight-01: 164; EGL-side, T722 scope)")
numbered(r"Scheduler\( 2488\)", "Scheduler(2488) lines")
numbered(r"RenderEngine\( 2488\)", "RenderEngine(2488) lines")
numbered(r"Displayed.*FallbackHome", "FallbackHome Displayed lines (T718 flight-01: 2)")
numbered(r"E/android\.hardware\.graphics\.composer", "composer-service E-lines (T718: 0)")
numbered(r"onFrameAvailable|validateDisplay|presentOrValidate|doComposition", "SF/HWC verbose lines (absent at this log level in T718)")

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
