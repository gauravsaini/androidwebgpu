#!/usr/bin/env python3
"""Verification REPORT for a Track T718 smoke flight (smoke or smoke-ext).

Usage: verify_smoke_t718.py [flight-dir]   (default: smoke)

Reports structural checks plus every T628 allocator / composer / installd /
onBootPhase line with its 1-based line number. The PASS/FAIL/INCONCLUSIVE
adjudication is made by the lane agent from this report against the brief's
hard smoke criterion for the ALLOCATOR wall (0 T628 REJECT lines AND 0
GraphicBufferAllocator failures AND >=1 nonzero allocation AND the T716
holds intact) — this script asserts only the structural boot checks and
the must-be-zero holds, never the allocator criterion itself.

DECISIVE WINDOW: the first T628 REJECT in T716 landed at guest ~104 s,
inside the 240 s window — unlike the composer mapped line (first open
attempt at guest ~522 s in T715), which is adjudicated from flight-01,
not from this smoke. If the shim was exercised (T628 lines present,
system_server started, guest past ~150 s) with 0 REJECT + 0 GBA failures
+ >=1 CALL allocate2 line, the allocator criterion is adjudicated from
this smoke: a CALL with no following GBA failure is the nonzero-allocation
evidence in the smoke window (the Bp surfaces any reply failure as a GBA
failure line, as in T717 smoke #4).

AVC NOTE (T716 correction 01:20 AEDT): the log format is `avc:  denied`
(TWO spaces). Single-space `avc: denied` matches 0 lines. SELinux runs
permissive, so /dev/mem denials are EXPECTED to persist (permissive=1,
logged not enforced) even when the fix works — avc counts are NEVER a
pass criterion. Only the sensorservice subset must be 0 (T715 hold).
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
FLIGHT = sys.argv[1] if len(sys.argv) > 1 else "smoke"
uart_path = WORKTREE / "flights" / FLIGHT / "uart.log"
if not uart_path.exists():
    print(f"FAIL: {uart_path} does not exist")
    sys.exit(1)

uart = uart_path.read_bytes().decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only, matching grep -n
lines = uart.split("\n")
if lines and lines[-1] == "":
    lines.pop()  # trailing newline: match grep -n / wc -l numbering

print(f"=== T718 SMOKE FLIGHT VERIFICATION REPORT ({FLIGHT}) ===")
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

# 7. sensorservice avc denials (TWO spaces — the real format)
avc_sensors = [l for l in lines if "avc:  denied" in l and "sensorservice" in l]
print(f"sensorservice avc denials: {len(avc_sensors)}")
assert len(avc_sensors) == 0, f"FAIL: Found {len(avc_sensors)} sensorservice avc denials"

# 7b. AVC format audit (report only — never a criterion)
avc_two = sum(1 for l in lines if "avc:  denied" in l)
avc_one = sum(1 for l in lines if "avc: denied" in l)
avc_devmem = sum(1 for l in lines if "avc:  denied" in l and "/dev/mem" in l)
print(f"avc:  denied (two spaces) total: {avc_two}")
print(f"avc: denied (one space) total: {avc_one} (expected 0: format check)")
print(f"avc:  denied for /dev/mem: {avc_devmem} (permissive logging; expected to persist)")

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

# 11. T718-specific evidence dump (report only, no asserts)
def dump(pattern, label, cap=60):
    hits = [(i + 1, l) for i, l in enumerate(lines) if re.search(pattern, l)]
    print(f"\n--- {label}: {len(hits)} line(s) ---")
    shown = hits if len(hits) <= cap else hits[:30] + hits[-30:]
    for n, l in shown:
        print(f"L{n}: {l.rstrip()}")
    if len(hits) > cap:
        print(f"... ({len(hits) - cap} middle lines omitted; count is exact)")
    return len(hits)

n_t628 = dump(r"T628_ALLOCATOR", "ALL T628_ALLOCATOR lines")
n_reject = dump(r"REJECT method=allocate2", "T628 REJECT allocate2 lines (brief requires 0)")
n_call = dump(r"CALL method=allocate2", "CALL allocate2 lines (shim reached write_result; brief requires >=1)")
n_gba = dump(r"GraphicBufferAllocator.*Failed to allocate", "GraphicBufferAllocator failures (brief requires 0)")
n_allocbuf = dump(r"allocateBuffers: failed to allocate buffer", "allocateBuffers failures (brief requires 0)")
dump(r"allocateBuffers", "ALL allocateBuffers lines (success evidence)")
n_attr = dump(r"getDisplayAttribute failed", "getDisplayAttribute failed lines (TOLERATED, report only)")
dump(r"DisplayDeviceInfo", "DisplayDeviceInfo lines (display formed)")
n_eperm = dump(r"simplefb: open /dev/mem failed", "EPERM lines (T716 hold requires 0)")
n_mapped = dump(r"simplefb: mapped", "simplefb: mapped lines (adjudicated from flight-01, not this smoke)")
n_devmem = dump(r"/dev/mem", "ALL /dev/mem lines")
dump(r"composer@2\.1-service|vendor\.hwc-2-1", "ALL composer service lines")
n_notfound = dump(r"installd not found", "installd not found")
dump(r"installd firing up|Command 'start installd'|service 'installd'", "installd STARTED lines")
dump(r"SensorService took .* in onBootPhase", "SensorService onBootPhase RETURN lines")
dump(r"installd", "ALL installd lines")
dump(r"fixAppsData", "ALL fixAppsData lines (0 is healthy per T715 findings)")
dump(r"socket/installd", "ALL /dev/socket/installd lines")
dump(r"onBootPhase|FutureTask\.get", "onBootPhase / FutureTask.get lines")
dump(r"boot_completed", "boot_completed lines")
dump(r"queueBuffer|presentDisplay", "queueBuffer / presentDisplay lines")
dump(r"T637_MAPPER|AIMapper", "T637_MAPPER / AIMapper lines")

# Guest time span (kernel timestamps)
times = [float(m.group(1)) for l in lines for m in [re.match(r"\[\s*([0-9]+\.[0-9]+)\]", l)] if m]
if times:
    print(f"\nGuest kernel time: first={times[0]}s last={times[-1]}s max={max(times)}s")

allocator_exercised = n_t628 > 0
print("\n=== REPORT COMPLETE — adjudicate manually ===")
print(f"T628 lines: {n_t628} (shim exercised: {allocator_exercised})")
print(f"REJECT count: {n_reject} (brief requires 0)")
print(f"CALL allocate2 count: {n_call} (brief requires >=1)")
print(f"GraphicBufferAllocator failure count: {n_gba} (brief requires 0)")
print(f"allocateBuffers failure count: {n_allocbuf} (brief requires 0)")
print(f"getDisplayAttribute failed count: {n_attr} (tolerated, not a criterion)")
print(f"EPERM count: {n_eperm} (T716 hold requires 0)")
print(f"simplefb: mapped count: {n_mapped} (adjudicated from flight-01)")
print(f"installd-not-found count: {n_notfound} (brief requires 0)")
print("Hard smoke criterion (allocator): REJECT == 0 AND GBA-fail == 0 AND CALL >= 1 AND T716 holds intact,")
print("with the shim exercised (T628 lines present, system_server started, guest past ~150 s).")
print("If the shim was NOT exercised, the smoke is INCONCLUSIVE — NOT a pass, NOT a fail.")
