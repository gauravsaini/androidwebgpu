#!/usr/bin/env python3
"""Transform the T726 scripts into the T727 scripts (Mac worktree scripts/).

Reads supervisor_t726.py, launch_probe_t726.py, analyze_t726_probe.py from
the worktree scripts/ dir and writes supervisor_t727.py, launch_probe_t727.py,
analyze_t727_probe.py with T727 pins. Every replacement asserts its count;
the script exits 1 if any expected replacement does not land. Run on the Mac.
"""
import re
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent  # this script lives in scripts/ itself
NEW_SYSTEM_SHA = "8e9914e3444bdac2cad65771dc8cbc1d8a1490fc5ba26fa276a7ba8878fe9bd7"
OLD_SYSTEM_SHA = "a7141b4515ce5a1dc1e35e870aed54eae5c12727ef6b971027488fdbe1131391"

SUP_DOC = '''"""T727 service-flight supervisor for the toybox-service discriminator (Mac QEMU).

Same stack as the preceding lanes (kernel t720-Image, CPIO t723-fix.cpio.gz, DTB
t723-modified.dtb) EXCEPT the system image is t727-system.img: the T721
revert image plus /system/etc/init/t727probe.rc, a trigger file on the
proven prior-lane shape (one service running /system/bin/toybox sh -c echo to
/dev/kmsg, two actions on property:sys.boot_completed=1 and on boot that
both write the marker T727_PROBE_OK to /dev/kmsg). Vendor.img is the T722
vendor image (patched stub) as a consistency check only: the guest loop-mounts
the CPIO-embedded copy.

T722 LESSON BUILT IN: the T720 supervisor's mid-flight check_disk_space()
called sys.exit(10) while QEMU was running, orphaning QEMU (manual SIGTERM
needed). Here the mid-flight disk gate terminates QEMU cleanly, waits for
it, writes flight-end.json, and THEN exits 10. No exit path leaves QEMU
running.

A flight re-run NEVER reuses a completed flight's folder: re-runs use a
new T727_FLIGHT name (e.g. service-flight-2), never the same name.
"""'''

LAUNCH_DOC = '''"""Launch T727 service flight detached (600 s guest target, 700 s host cap).

The flight discriminates the prior-lane service-start failure in one flight:
the identical property-action shape with the service command replaced by
a known-good binary (/system/bin/toybox sh -c "echo T727_SERVICE_OK >
/dev/kmsg"). If the toybox service runs (T727_SERVICE_OK in the UART),
the fault was the probe path; if it never starts although the action
marker fired, the fault is the start leg itself. Window: the on boot
action fires ~95 s guest; sys.boot_completed=1 lands ~498 s guest (and
the boot may run slow -- the preceding lane never reached it in 554 s);
the 600 s guest target covers both triggers plus the fence-path hold window.

Env overrides: T727_FLIGHT (default service-flight; re-runs MUST use
a new name, never overwrite a completed flight's folder),
T727_GUEST_TARGET, T727_HOST_CAP, T727_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""'''

ANALYZE_DOC = r'''"""Adjudication report for the Track T727 service-flight UART (toybox-service discriminator).

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
"""'''

def replace(src, old, new, expect_min=1, label=""):
    n = src.count(old)
    assert n >= expect_min, f"{label}: expected >= {expect_min} of {old!r}, found {n}"
    return src.replace(old, new), n

def transform_supervisor(src):
    # Whole-docstring replacement (first triple-quoted block).
    m = re.search(r'"""(?:.|\n)*?"""\n', src)
    assert m, "supervisor docstring not found"
    src = src[:m.start()] + SUP_DOC + "\n" + src[m.end():]
    src, _ = replace(src, '"T726"', '"T727"', 1, "TRACK")
    src, _ = replace(src, "T726_", "T727_", 10, "env vars")
    src, _ = replace(src, "t726-system.img", "t727-system.img", 1, "system img name")
    src, _ = replace(src, "progress-t726.txt", "progress-t727.txt", 1, "progress file")
    src, _ = replace(src, OLD_SYSTEM_SHA, NEW_SYSTEM_SHA, 1, "system img SHA")
    src, _ = replace(src, '"probe-flight"', '"service-flight"', 1, "flight default")
    assert "T726" not in src and "t726" not in src and OLD_SYSTEM_SHA not in src, "supervisor leftovers"
    assert "t720" in src, "supervisor lost t720 kernel refs"
    return src

def transform_launcher(src):
    m = re.search(r'"""(?:.|\n)*?"""\n', src)
    assert m, "launcher docstring not found"
    src = src[:m.start()] + LAUNCH_DOC + "\n" + src[m.end():]
    src, _ = replace(src, "T726", "T727", 5, "track id")
    src, _ = replace(src, "t726", "t727", 1, "file names")
    src, _ = replace(src, '"probe-flight"', '"service-flight"', 1, "flight default")
    assert "T726" not in src and "t726" not in src, "launcher leftovers"
    return src

def transform_analyzer(src):
    m = re.search(r'"""(?:.|\n)*?"""\n', src)
    assert m, "analyzer docstring not found"
    src = src[:m.start()] + ANALYZE_DOC + "\n" + src[m.end():]
    # Marker section FIRST (it still carries T726 marker text).
    old_marker = r'''    # T726 marker + trigger evidence (decisive: registration re-proof + service trigger fired)
    dump_full(r"T726_PROBE_OK", "ALL T726_PROBE_OK marker lines (full text)", cap=60)
    dump_full(r"t726probe", "ALL t726probe lines (service + rc citations)", cap=60)
    numbered(r"starting service 't726probe'", "init starting service 't726probe' (trigger fired)")
    numbered(r"t726probe\.rc", "t726probe.rc citations (init parsed the new file)")
    numbered(r"Command 'start t726probe'", "start-command lines citing t726probe (trigger fired)")
    numbered(r"processing action \(sys\.boot_completed=1\) from \(/system/etc/init/", "ALL boot_completed action processing lines (stock control + mine)", cap=40)
    numbered(r"processing action \(boot\) from \(/system/etc/init/t726probe\.rc", "on boot action processing lines citing t726probe.rc")
    numbered(r"cannot exec.*t726probe|Failed to start service 't726probe'", "service exec/start failures (trigger requires 0)")
    numbered(r"write /dev/kmsg.*T726", "write-to-kmsg command lines (context)")
'''
    new_marker = r'''    # T727 marker + trigger evidence (decisive: toybox-service discrimination)
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
'''
    assert old_marker in src, "analyzer marker section not found verbatim"
    src = src.replace(old_marker, new_marker)
    src, _ = replace(src, "flights/probe-flight/uart.log", "flights/service-flight/uart.log", 1, "uart path")
    assert "T726_PROBE_OK" not in src and "t726probe" not in src and "T726" not in src, "analyzer leftovers"
    return src

def main():
    jobs = [
        ("supervisor_t726.py", "supervisor_t727.py", transform_supervisor),
        ("launch_probe_t726.py", "launch_probe_t727.py", transform_launcher),
        ("analyze_t726_probe.py", "analyze_t727_probe.py", transform_analyzer),
    ]
    for old_name, new_name, fn in jobs:
        src = (SCRIPTS / old_name).read_text(encoding="utf-8")
        out = fn(src)
        (SCRIPTS / new_name).write_text(out, encoding="utf-8")
        print(f"{old_name} -> {new_name} ({len(out)} bytes)")
    print("TRANSFORM OK")

if __name__ == "__main__":
    main()
