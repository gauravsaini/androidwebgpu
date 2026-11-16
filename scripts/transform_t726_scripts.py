#!/usr/bin/env python3
"""Transform the T725 scripts into the T726 scripts (Mac worktree scripts/).

Reads supervisor_t725.py, launch_probe_t725.py, analyze_t725_probe.py from
the worktree scripts/ dir and writes supervisor_t726.py, launch_probe_t726.py,
analyze_t726_probe.py with T726 pins. Every replacement asserts its count;
the script exits 1 if any expected replacement does not land. Run on the Mac.
"""
import re
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent  # this script lives in scripts/ itself
NEW_SYSTEM_SHA = "a7141b4515ce5a1dc1e35e870aed54eae5c12727ef6b971027488fdbe1131391"
OLD_SYSTEM_SHA = "1571a10fecb8f5ef20c08e8f5e837b9c16798b6ea11e19417c58bd2a41f961df"

SUP_DOC = '''"""T726 probe-flight supervisor for the EGL dispatch re-flight (Mac QEMU).

Same stack as the preceding lanes (kernel t720-Image, CPIO t723-fix.cpio.gz, DTB
t723-modified.dtb) EXCEPT the system image is t726-system.img: the T721
revert image plus /system/etc/init/t726probe.rc, a trigger file on the
proven minimal shape (one service running the early-ramdisk probe binary,
two actions on property:sys.boot_completed=1 and on boot that both write
the marker T726_PROBE_OK to /dev/kmsg). Vendor.img is the T722 vendor image
(patched stub) as a consistency check only: the guest loop-mounts the
CPIO-embedded copy.

T722 LESSON BUILT IN: the T720 supervisor's mid-flight check_disk_space()
called sys.exit(10) while QEMU was running, orphaning QEMU (manual SIGTERM
needed). Here the mid-flight disk gate terminates QEMU cleanly, waits for
it, writes flight-end.json, and THEN exits 10. No exit path leaves QEMU
running.

A flight re-run NEVER reuses a completed flight's folder: re-runs use a
new T726_FLIGHT name (e.g. probe-flight-2), never the same name.
"""'''

LAUNCH_DOC = '''"""Launch T726 probe flight detached (600 s guest target, 700 s host cap).

The flight re-flies the early-ramdisk probe with the proven trigger-file
shape: the service runs the probe binary on sys.boot_completed=1, and the
dual-trigger marker re-proves registration in the same flight. The probe's
own lines plus its DONE verdict are the EGL dispatch answer. Window: the
on boot action fires ~95 s guest; sys.boot_completed=1 lands ~498 s guest
(and the boot may run slow -- the preceding lane never reached it in 554 s);
the 600 s guest target covers both triggers plus the fence-path hold window.

Env overrides: T726_FLIGHT (default probe-flight; re-runs MUST use
a new name, never overwrite a completed flight's folder),
T726_GUEST_TARGET, T726_HOST_CAP, T726_GDB_PORT.

Import-safe: the fork/exec happens only under __main__.
"""'''

ANALYZE_DOC = '''"""Adjudication report for the Track T726 probe-flight UART (EGL dispatch re-flight).

Counts every quantity the brief's Step 4/5 criteria need, with exact
1-based line numbers from grep-equivalent scans. Report only: the lane
agent adjudicates PASS/FAIL from this output against the preserved UART.

Criteria (probe re-flight): the probe's lines plus its DONE summary
line are the dispatch answer; the registration marker re-proves the
trigger file registered in the same flight; the trigger-fired-first
check (sys.boot_completed count + boot timing) comes before any
judgement of the property action. The stock boot_completed actions
(dmesgd/bootstat/hw/init.rc:1224) are the dispatch control -- if they
processed and mine did not, the failure is file-specific; if neither did,
the earlier multi-action stall is the likelier mechanism. Holds: T720_PROBE
DONE PASS >=1, T721 fence log >=1, Invalid present fence 0, REJECT 0,
EPERM 0, mapped >=1, Watchdog KILLING 0, sys.boot_completed >=1, panic/
Oops/BUG 0. The RenderEngine dup-fail count/rate is reported against the
T721 baselines (4.058 flight-01, 6.660 revert per 1000 s) for context only.

UART handling (standing rule): read as bytes, split on \\n only, matching
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
    src, _ = replace(src, '"T725"', '"T726"', 1, "TRACK")
    src, _ = replace(src, "T725_", "T726_", 10, "env vars")
    src, _ = replace(src, "t725-system.img", "t726-system.img", 1, "system img name")
    src, _ = replace(src, "progress-t725.txt", "progress-t726.txt", 1, "progress file")
    src, _ = replace(src, OLD_SYSTEM_SHA, NEW_SYSTEM_SHA, 1, "system img SHA")
    src, _ = replace(src, '"registration-smoke"', '"probe-flight"', 1, "flight default")
    assert "T725" not in src and "t725" not in src and OLD_SYSTEM_SHA not in src, "supervisor leftovers"
    assert "t720" in src, "supervisor lost t720 kernel refs"
    return src

def transform_launcher(src):
    m = re.search(r'"""(?:.|\n)*?"""\n', src)
    assert m, "launcher docstring not found"
    src = src[:m.start()] + LAUNCH_DOC + "\n" + src[m.end():]
    src, _ = replace(src, "T725", "T726", 5, "track id")
    src, _ = replace(src, "t725", "t726", 1, "file names")
    src, _ = replace(src, '"registration-smoke"', '"probe-flight"', 1, "flight default")
    assert "T725" not in src and "t725" not in src, "launcher leftovers"
    return src

def transform_analyzer(src):
    m = re.search(r'"""(?:.|\n)*?"""\n', src)
    assert m, "analyzer docstring not found"
    src = src[:m.start()] + ANALYZE_DOC + "\n" + src[m.end():]
    # Marker section FIRST (it still carries T725 marker text).
    old_marker = '''    # T725 marker evidence (decisive: minimal .rc file registered)
    dump_full(r"T725_REG_OK", "ALL T725_REG_OK marker lines (full text)", cap=60)
    dump_full(r"\\[T723\\]", "ALL [T723] early-ramdisk hook lines (context: hook timeout ~33 s)", cap=20)

    # T725 trigger evidence (decisive: init .rc service trigger fired)
    dump_full(r"t725probe", "ALL t725probe lines (service + rc citations)", cap=60)
    numbered(r"starting service 't725probe'", "init starting service 't725probe' (trigger fired)")
    numbered(r"t725probe\\.rc", "t725probe.rc citations (init parsed the new file)")
    numbered(r"Command 'start t725probe'", "start-command lines citing t725probe (trigger fired)")
    numbered(r"processing action \\(sys\\.boot_completed=1\\) from \\(/system/etc/init/", "ALL boot_completed action processing lines (stock control + mine)", cap=40)
    numbered(r"processing action \\(boot\\) from \\(/system/etc/init/t725probe\\.rc", "on boot action processing lines citing t725probe.rc")
    numbered(r"cannot exec.*t725probe|Failed to start service 't725probe'", "service exec/start failures (trigger requires 0)")
    numbered(r"write /dev/kmsg.*T725", "write-to-kmsg command lines (context)")
'''
    new_marker = '''    # T723 probe evidence (decisive: guest-origin dispatch answer)
    dump_full(r"T723_PROBE", "ALL T723_PROBE lines (full text)", cap=80)
    numbered(r"T723_PROBE DONE", "T723_PROBE DONE summary lines (brief requires >=1)")
    numbered(r"T723_PROBE SP1 DONE", "SP1 DONE lines (direct driver call)")
    numbered(r"T723_PROBE SP2 DONE", "SP2 DONE lines (META-EGL dispatch call)")
    numbered(r"T723_PROBE SP3 DONE", "SP3 DONE lines (sw_sync control)")
    numbered(r"T723_PROBE: DROP_FAIL", "DROP_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP1: RESOLVE_FAIL", "SP1 RESOLVE_FAIL lines (brief requires 0)")
    numbered(r"T723_PROBE SP2: GETPROCADDRESS_NULL", "SP2 GETPROCADDRESS_NULL lines (brief requires 0)")
    dump_full(r"\\[T723\\]", "ALL [T723] early-ramdisk hook lines (context: hook timeout ~33 s)", cap=20)

    # T726 marker + trigger evidence (decisive: registration re-proof + service trigger fired)
    dump_full(r"T726_PROBE_OK", "ALL T726_PROBE_OK marker lines (full text)", cap=60)
    dump_full(r"t726probe", "ALL t726probe lines (service + rc citations)", cap=60)
    numbered(r"starting service 't726probe'", "init starting service 't726probe' (trigger fired)")
    numbered(r"t726probe\\.rc", "t726probe.rc citations (init parsed the new file)")
    numbered(r"Command 'start t726probe'", "start-command lines citing t726probe (trigger fired)")
    numbered(r"processing action \\(sys\\.boot_completed=1\\) from \\(/system/etc/init/", "ALL boot_completed action processing lines (stock control + mine)", cap=40)
    numbered(r"processing action \\(boot\\) from \\(/system/etc/init/t726probe\\.rc", "on boot action processing lines citing t726probe.rc")
    numbered(r"cannot exec.*t726probe|Failed to start service 't726probe'", "service exec/start failures (trigger requires 0)")
    numbered(r"write /dev/kmsg.*T726", "write-to-kmsg command lines (context)")
'''
    assert old_marker in src, "analyzer marker section not found verbatim"
    src = src.replace(old_marker, new_marker)
    src, _ = replace(src, "flights/registration-smoke/uart.log", "flights/probe-flight/uart.log", 1, "uart path")
    assert "T725_REG_OK" not in src and "t725probe" not in src and "T725" not in src, "analyzer leftovers"
    return src

def main():
    jobs = [
        ("supervisor_t725.py", "supervisor_t726.py", transform_supervisor),
        ("launch_probe_t725.py", "launch_probe_t726.py", transform_launcher),
        ("analyze_t725_probe.py", "analyze_t726_probe.py", transform_analyzer),
    ]
    for old_name, new_name, fn in jobs:
        src = (SCRIPTS / old_name).read_text(encoding="utf-8")
        out = fn(src)
        (SCRIPTS / new_name).write_text(out, encoding="utf-8")
        print(f"{old_name} -> {new_name} ({len(out)} bytes)")
    print("TRANSFORM OK")

if __name__ == "__main__":
    main()
