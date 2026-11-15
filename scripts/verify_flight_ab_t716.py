#!/usr/bin/env python3
"""Verification report for the Track T716 flight-ab canonical control.

The control's expected pattern (T714/T715 precedent): the canonical kernel
exits early (T715: 575 lines, 47,928 B) without reaching system_server.
This script reports what actually happened; the lane agent compares it
against the T715 control.
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
print(f"EPERM lines: {len(re.findall(r'simplefb: open /dev/mem failed', uart))}")
print(f"simplefb: mapped lines: {len(re.findall(r'simplefb: mapped', uart))}")
print(f"/dev/mem lines: {len(re.findall(r'/dev/mem', uart))}")
print(f"simplefb registered: {len(re.findall(r'fb0: simplefb registered!', uart))}")
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
