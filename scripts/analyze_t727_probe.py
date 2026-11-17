#!/usr/bin/env python3
"""Adjudication report for the Track T727 service-flight UART (toybox-service discriminator).

Counts every quantity the brief's Step 4/5 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.

Criteria (discrimination): the toybox service's own T727_SERVICE_OK marker
plus the init 'starting service' line are the positive proof the start leg
works (fault = probe path); their absence with the action marker
T727_PROBE_OK present means the start leg itself failed (fault = start
leg). The registration marker re-proves the trigger file registered in
the same flight; the trigger-fired-first check (sys.boot_completed count
+ boot timing) comes before any judgement of the property action. The
stock boot_completed actions (dmesgd/bootstat/hw/init.rc:1224) are the
dispatch control. The T723 probe is NOT flown in this lane: its counts
are context only (expect 0). Holds: T720_PROBE DONE PASS >=1, T721 fence
log >=1, Invalid present fence 0, REJECT 0, EPERM 0, mapped >=1, Watchdog
KILLING 0, sys.boot_completed >=1, panic/Oops/BUG 0. The RenderEngine
dup-fail count/rate is reported against the T721 baselines (4.058
flight-01, 6.660 revert per 1000 s) for context only.

UART handling (standing rule): read as bytes, split on \n only, matching
grep -n numbering. The trailing partial line (if any) counts by iteration
and is reported separately.
"""
import re
import sys
from pathlib import Path

WORKTREE = Path(__file__).resolve().parent.parent

def main() -> None:
    if len(sys.argv) > 1:
        uart_path = Path(sys.argv[1])
    else:
        uart_path = WORKTREE / "flights/service-flight/uart.log"
    if not uart_path.exists():
        print(f"FAIL: {uart_path} does not exist")
        sys.exit(1)

    raw = uart_path.read_bytes()
    uart = raw.decode("latin1", errors="replace")  # bytes: no newline translation; split on \n only
    lines = uart.split("\n")
    trailing_partial = 0
    if lines and lines[-1] == "":
        lines.pop()  # trailing newline: match grep -n / wc -l numbering
    elif lines and lines[-1] != "":
        trailing_partial = 1  # last line has no trailing newline: counts by iteration only
    print(f"Total lines: {len(lines)} (trailing partial line: {trailing_partial})")
    print(f"Total bytes: {len(raw)}")

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

    # T723 probe evidence (decisive: guest-origin dispatch answer)
    dump_full(r"T723_PROBE", "ALL T723_PROBE lines (full text)", cap=80)
    numbered(r"T723_PROBE DONE", "T723_PROBE DONE summary lines (brief requires >=1)")
    numbered(r"T723_PROBE SP1 DONE", "SP1 DONE lines (direct driver call)")
    numbered(r"T723_PROBE SP2 DONE", "SP2 DONE lines (META-EGL dispatch call)")
    numbered(r"T723_PROBE SP3 DONE", "SP3 DONE lines (sw_sync control)")
    numbered(r"T723_PROBE: DROP_FAIL", "DROP_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP1: RESOLVE_FAIL", "SP1 RESOLVE_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP2: GETPROCADDRESS_NULL", "SP2 GETPROCADDRESS_NULL lines (brief requires 0)")
    dump_full(r"\[T723\]", "ALL [T723] early-ramdisk hook lines (context: hook timeout ~33 s)", cap=20)

    # T727 marker + trigger evidence (decisive: toybox-service discrimination)
    dump_full(r"T727_SERVICE_OK", "ALL T727_SERVICE_OK marker lines (full text) — service ran", cap=60)
    dump_full(r"T727_PROBE_OK", "ALL T727_PROBE_OK marker lines (full text) — registration", cap=60)
    dump_full(r"t727probe", "ALL t727probe lines (service + rc citations)", cap=60)
    numbered(r"starting service 't727probe'", "init starting service 't727probe' (trigger fired)")
    numbered(r"t727probe\.rc", "t727probe.rc citations (init parsed the new file)")
    numbered(r"Command 'start t727probe'", "start-command lines citing t727probe (trigger fired)")
    numbered(r"processing action \(sys\.boot_completed=1\) from \(/system/etc/init/", "ALL boot_completed action processing lines (stock control + mine)", cap=40)
    numbered(r"processing action \(boot\) from \(/system/etc/init/t727probe\.rc", "on boot action processing lines citing t727probe.rc")
    numbered(r"cannot exec.*t727probe|Failed to start service 't727probe'", "service exec/start failures (trigger requires 0)")
    numbered(r"write /dev/kmsg.*T727", "write-to-kmsg command lines (context)")

    # T720 probe hold (fence source still proven every flight)
    numbered(r"T720_PROBE DONE status=PASS", "T720_PROBE DONE status=PASS lines (hold requires >=1)")
    numbered(r"T720_PROBE DONE status=FAIL", "T720_PROBE DONE status=FAIL lines (hold requires 0)")

    # T721 fence-path holds
    numbered(r"T721 fd=", "T721 fence-log lines (hold requires >=1)")
    numbered(r"trackPendingFrame: Invalid present fence", "Invalid present fence lines (hold requires 0)")
    numbered(r"sw_sync", "ALL sw_sync lines (context)")

    # T722 holds: allocator + composer + watchdog
    numbered(r"REJECT method=allocate2", "T628 REJECT allocate2 lines (hold requires 0)")
    numbered(r"CALL method=allocate2", "T628 CALL allocate2 lines")
    numbered(r"simplefb: open /dev/mem failed", "EPERM lines (hold requires 0)")
    numbered(r"simplefb: mapped", "simplefb: mapped lines (hold requires >=1)")
    numbered(r"Watchdog:.*KILLING", "Watchdog KILLING lines (hold requires 0)")
    numbered(r"sys\.boot_completed", "sys.boot_completed lines (hold requires >=1)")
    numbered(r"boot_completed", "boot_completed lines")
    numbered(r"FATAL EXCEPTION", "FATAL EXCEPTION lines")
    numbered(r"Kernel panic|Oops:|BUG:", "Kernel panic/Oops/BUG lines (hold requires 0)")

    # RenderEngine dup-fail signal (context vs T721 baselines 4.058/6.660 per 1000 s)
    dup_hits = numbered(r"failed to dup EGL native fence sync", "RenderEngine EGL dup-fail lines")
    numbered(r"loaded /vendor/lib64/egl/", "driver loaded lines")
    numbered(r"1\.4 Android META-EGL", "META-EGL banner lines")
    numbered(r"D/libEGL", "D/libEGL lines")

    # SELinux denials (TWO spaces, proven pattern)
    print(f"\navc:  denied (two spaces) total: {sum(1 for l in lines if 'avc:  denied' in l)}")
    print(f"avc:  denied for /dev/mem: {sum(1 for l in lines if 'avc:  denied' in l and '/dev/mem' in l)}")
    print(f"avc denied (sensorservice): {sum(1 for l in lines if 'avc:  denied' in l and 'sensorservice' in l)}")

    # Guest time span + normalized dup-fail rate (T722 method: count / guest span * 1000)
    times = [float(m.group(1)) for l in lines for m in [re.match(r"\[\s*([0-9]+\.[0-9]+)\]", l)] if m]
    if times:
        span = max(times)
        print(f"\nGuest kernel time: first={times[0]}s last={times[-1]}s max={span}s")
        if span > 0:
            rate = len(dup_hits) / span * 1000.0
            print(f"Dup-fail rate: {len(dup_hits)} / {span:.1f}s * 1000 = {rate:.3f} per 1000 s (baselines: T721 flight-01 4.058, T721 revert 6.660)")
    print("\n=== REPORT COMPLETE ===")

if __name__ == "__main__":
    main()
