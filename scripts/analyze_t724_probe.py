#!/usr/bin/env python3
"""Adjudication report for the Track T724 probe-flight UART (init .rc trigger; same T723 probe binary).

Counts every quantity the brief's Step 3a/4 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.

Criteria (investigation lane): the T723_PROBE DONE summary line answers
the dispatch question (drop_ok=1, control_fd/direct_fd/meta_fd), BOTH
sub-probes SP1 (direct) and SP2 (META-EGL) answered, the T721 fence path
intact (T721 fence log present, Invalid present fence 0), all T722 holds
intact (REJECT 0, EPERM 0, mapped >=1, kills 0, boot_completed present),
and no new crash/regression. The RenderEngine dup-fail count/rate is
reported against the T721 baselines (4.058 flight-01, 6.660 revert per
1000 s, normalized over the guest span) for context; the probe lines,
not the dup-fail rate, decide this lane.

UART handling (standing rule): read as bytes, split on \\n only, matching
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
        uart_path = WORKTREE / "flights/probe-flight/uart.log"
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
    dump_full(r"T723_PROBE", "ALL T723_PROBE lines (full text)", cap=60)
    dump_full(r"\[T723\]", "ALL [T723] hook lines (full text)", cap=20)

    # T724 trigger evidence (decisive: init .rc service trigger fired)
    dump_full(r"t723probe", "ALL t723probe lines (service + rc citations)", cap=60)
    numbered(r"starting service 't723probe'", "init starting service 't723probe' (trigger fired)")
    numbered(r"t723probe\.rc", "t723probe.rc citations (init parsed the new file)")
    numbered(r"Command 'exec.*t723probe\.rc", "exec diagnostic commands citing t723probe.rc")
    numbered(r"cannot exec.*t723probe|Failed to start service 't723probe'", "service exec/start failures (trigger requires 0)")
    numbered(r"T723_PROBE DONE", "T723_PROBE DONE summary lines (brief requires >=1)")
    numbered(r"T723_PROBE SP1 DONE", "SP1 DONE lines (direct driver call)")
    numbered(r"T723_PROBE SP2 DONE", "SP2 DONE lines (META-EGL dispatch call)")
    numbered(r"T723_PROBE SP3 DONE", "SP3 DONE lines (sw_sync control)")
    numbered(r"T723_PROBE: DROP_FAIL", "DROP_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP1: RESOLVE_FAIL", "SP1 RESOLVE_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP2: GETPROCADDRESS_NULL", "SP2 GETPROCADDRESS_NULL lines (brief requires 0)")

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
