#!/usr/bin/env python3
"""Verification report for the Track T717 flight-ab canonical control.

The control's expected pattern (T714/T715/T716 precedent): the canonical
kernel exits early (T716: 576 lines, guest 20.36 s, QEMU self-exit 0)
without reaching system_server. This script reports what actually
happened; the lane agent compares it against the T716 control.
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent
uart_path = WORKTREE / "flights/flight-ab/uart.log"
if not uart_path.exists():
    print(f"FAIL: {uart_path} does not exist")
    sys.exit(1)

uart = uart_path.read_bytes().decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only, matching grep -n
lines = uart.split("\n")
if lines and lines[-1] == "":
    lines.pop()  # trailing newline: match grep -n / wc -l numbering
print(f"Total lines: {len(lines)}")
print(f"system_server starts: {len(re.findall(r'Entered the Android system server!', uart))}")
print(f"installd lines: {len(re.findall(r'installd', uart))}")
print(f"queueBuffer lines: {len(re.findall(r'queueBuffer', uart))}")
print(f"presentDisplay lines: {len(re.findall(r'presentDisplay', uart))}")
print(f"EPERM lines: {len(re.findall(r'simplefb: open /dev/mem failed', uart))}")
print(f"simplefb: mapped lines: {len(re.findall(r'simplefb: mapped', uart))}")
print(f"/dev/mem lines: {len(re.findall(r'/dev/mem', uart))}")
print(f"simplefb registered: {len(re.findall(r'fb0: simplefb registered!', uart))}")
print(f"T628_ALLOCATOR lines: {len(re.findall(r'T628_ALLOCATOR', uart))}")
print(f"REJECT method=allocate2 lines: {len(re.findall(r'REJECT method=allocate2', uart))}")
print(f"GraphicBufferAllocator failures: {len(re.findall(r'GraphicBufferAllocator.*Failed to allocate', uart))}")
print(f"allocateBuffers failures: {len(re.findall(r'allocateBuffers: failed to allocate buffer', uart))}")
print(f"getDisplayAttribute failed lines: {len(re.findall(r'getDisplayAttribute failed', uart))}")
print(f"boot_completed lines: {len(re.findall(r'boot_completed', uart))}")
times = [float(m.group(1)) for l in lines for m in [re.match(r"\[\s*([0-9]+\.[0-9]+)\]", l)] if m]
if times:
    print(f"Guest kernel time: first={times[0]}s last={times[-1]}s max={max(times)}s")
print("First 5 lines:")
for l in lines[:5]:
    print("  " + l.rstrip())
print("Last 5 lines:")
for l in lines[-5:]:
    print("  " + l.rstrip())
print("\n=== REPORT COMPLETE ===")
