#!/usr/bin/env python3
"""Verification REPORT for the Track T715 smoke flight.

Reports structural checks plus every installd / fixAppsDataOnBoot /
onBootPhase line with its 1-based line number. The PASS/FAIL
adjudication is made by the lane agent from this report against the
brief's hard criterion (fixAppsDataOnBoot COMPLETION line + 0
"installd not found" + installd starting) — this script asserts only
the structural boot checks, never a guessed completion string.
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
uart_path = WORKTREE / "flights/smoke/uart.log"
if not uart_path.exists():
    print(f"FAIL: {uart_path} does not exist")
    sys.exit(1)

uart = uart_path.read_bytes().decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only, matching grep -n
lines = uart.split("\n")
if lines and lines[-1] == "":
    lines.pop()  # trailing newline: match grep -n / wc -l numbering

print("=== T715 SMOKE FLIGHT VERIFICATION REPORT ===")
print(f"Total lines: {len(lines)}")

# 1. Freeing unused kernel memory
m_mem = re.search(r"\[\s*([0-9.]+)\] Freeing unused kernel memory: (\d+K)", uart)
assert m_mem, "FAIL: Freeing unused kernel memory not found"
print(f"PASS: Freeing unused kernel memory: {m_mem.group(2)} at guest {m_mem.group(1)}s")

# 2. init first stage started
m_init = re.search(r"\[\s*([0-9.]+)\] init: init first stage started!", uart)
assert m_init, "FAIL: Init first stage not found"
print(f"PASS: Init first stage started at guest {m_init.group(1)}s")

# 3. simplefb registered
m_fb = re.search(r"\[\s*([0-9.]+)\] .*fb0: simplefb registered!", uart)
assert m_fb, "FAIL: simplefb not found"
print(f"PASS: fb0 simplefb registered at guest {m_fb.group(1)}s")

# 4. VDA mount
m_vda = re.search(r"\[\s*([0-9.]+)\] init: \[libfs_mgr\] .*source=/dev/block/vda,target=/system.*=0: Success|\[\s*([0-9.]+)\] init: \[libfs_mgr\]Mounted /dev/block/vda as /system Success", uart)
assert m_vda, "FAIL: /dev/block/vda not mounted as /system Success"
guest_vda_time = m_vda.group(1) or m_vda.group(2)
print(f"PASS: /dev/block/vda mounted as /system Success at guest {guest_vda_time}s")

# 5. SensorsMultiHal and hals.conf
load_failures = len(re.findall(r"Failed to load subHal config file", uart))
print(f"Failed to load subHal config file errors: {load_failures}")
assert load_failures == 0, f"FAIL: Found {load_failures} subHal config load failures"

# 6. Dummy sensorservice registration
dummy_reg = re.findall(r"dummy_sensorservice.*registered|Starting dummy sensorservice daemon", uart)
print(f"dummy_sensorservice lines: {len(dummy_reg)}")

# 7. sensorservice avc denials
avc_sensors = [l for l in lines if "avc:  denied" in l and "sensorservice" in l]
print(f"sensorservice avc denials: {len(avc_sensors)}")
assert len(avc_sensors) == 0, f"FAIL: Found {len(avc_sensors)} sensorservice avc denials"

# 8. Watchdog kills
watchdog_kills = len(re.findall(r"Watchdog:.*KILLING", uart))
print(f"Watchdog kills: {watchdog_kills}")

# 9. Prime cache
prime_errors = len(re.findall(r"output buffer not gpu writeable", uart))
print(f"Prime cache errors: {prime_errors}")
assert prime_errors == 0, f"FAIL: Prime cache errors found: {prime_errors}"

# 10. system_server started
ss_starts = re.findall(r"Entered the Android system server!", uart)
print(f"system_server starts: {len(ss_starts)}")
assert len(ss_starts) > 0, "FAIL: system_server did not start"

# 11. T715-specific evidence dump (report only, no asserts on guessed strings)
def dump(pattern, label, cap=60):
    hits = [(i + 1, l) for i, l in enumerate(lines) if re.search(pattern, l)]
    print(f"\n--- {label}: {len(hits)} line(s) ---")
    shown = hits if len(hits) <= cap else hits[:30] + hits[-30:]
    for n, l in shown:
        print(f"L{n}: {l.rstrip()}")
    if len(hits) > cap:
        print(f"... ({len(hits) - cap} middle lines omitted; count is exact)")
    return len(hits)

n_notfound = dump(r"installd not found", "installd not found")
dump(r"installd", "ALL installd lines")
dump(r"fixAppsData", "ALL fixAppsData lines")
dump(r"socket/installd", "ALL /dev/socket/installd lines")
dump(r"onBootPhase|FutureTask\.get", "onBootPhase / FutureTask.get lines")
dump(r"phase 200|boot phase", "phase 200 / boot phase lines")
dump(r"boot_completed", "boot_completed lines")
dump(r"mSensorServiceStart", "mSensorServiceStart lines")
dump(r"queueBuffer|presentDisplay", "queueBuffer / presentDisplay lines")

print("\n=== REPORT COMPLETE — adjudicate manually ===")
print(f"installd-not-found count: {n_notfound} (brief requires 0)")
print("Hard smoke criterion: fixAppsDataOnBoot COMPLETION line present +")
print("  installd-not-found == 0 + installd starting. A 0-kill smoke")
print("  WITHOUT the completion line is NOT a pass (T714 V1 lesson).")
