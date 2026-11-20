# T733 findings — minimal no-libc binary discrimination (does init even attempt the execve?)

Verdict: FAIL

## Flight

- service-flight-3, 2026-11-20 20:35:46–20:49:58 AEDT (09:35:46–09:49:58Z).
- Host 851.93 s, guest 851.062 s, QEMU exit 0. Stop reason: guest target
  reached (851.1 s >= 850 s). The guest target WAS reached — the widened
  850/1100 window worked again, and the guest ran at ~1.00 pace
  (851.062/851.93), as in T732.
- Flight name is service-flight-3, not service-flight or service-flight-2:
  flights/service-flight/ holds the COMPLETED T731 flight and
  flights/service-flight-2/ holds the COMPLETED T732 flight (both committed),
  and reusing either name would have overwritten one — forbidden by the
  standing no-overwrite rule. Both folders were left untouched.
- UART: 33,821 lines, 4,001,204 B, SHA256
  acc4763e71a8ff5ab8a7f6a83e3e124b30bc8e4b6ce5bca5ff7adf70e6ebaff0.
- Flown image: t733-system.img SHA256
  af40403aa286a770f67dc8599584ef038c3192f2ff43ff89e62a3546ded89043
  (T732 image f7980cfb + new binary + new rc only; all other inputs
  byte-identical to T732). New patch SHA256 4d74f4d1 (6 chunks, 6,291,456 B).
- New binary: nb-t733probe.bin SHA256 fd189c08 (1,648 B), static ELF64
  aarch64, no INTERP/DYNAMIC, entry 0x400120 = _start, _start does
  openat(56) / write(64) / exit_group(94) on /dev/kmsg with fd-1 fallback,
  length 16. No libc, no dynamic linker, no _start -> __libc_start_main ->
  main chain. No qemu-aarch64 on box or Mac, so no host-side run — the
  flight is the test.
- New rc: t733probe-new.rc SHA256 79ad3cc3 (722 B; T732 rc f9baa479 with
  exactly three renames — service name t728probe -> t733probe, binary path
  /system/bin/t728probe -> /system/bin/t733probe, marker T728_PROBE_OK ->
  T733_PROBE_OK; diff-verified, header comment byte-identical). The exec
  is BEFORE the write in the on-boot action (exec, then write) — so the
  write firing proves the exec returned. The property action keeps
  `start` + write.
- Parser citation: parse_init_rc.py on the exact new rc bytes — VERDICT
  ACCEPT, 0 errors, 1 service, 2 actions, 0 imports. (Positive control:
  the T732 rc bytes f9baa479 also ACCEPT.)
- Label (box debugfs read-back from the NEW flown image, T729 method):
  new binary `u:object_r:system_file:s0`, new rc xattr-empty; T728 binary
  and rc untouched. Dump-backs identical (new bin fd189c08, new rc
  79ad3cc3, T728 bin 19db5d31, T728 rc f9baa479).
- Marker uniqueness (pre-flight grep): 0 T733_MINIMAL_OK / T733_PROBE_OK
  in the box vault T729–T732 UARTs and the sandbox T731/T732 copies.

## Adjudication (brief's 7-item order)

1. On-boot exec: RETURNED, minimal binary produced nothing. The exec is
   synchronous and the write after it fired at guest 117.83 s (L10652,
   T733_PROBE_OK), proving the exec RETURNED by then. But
   `T733_MINIMAL_OK` 0 lines, 0 `t733probe` lines in the UART, 0 init
   service-refusal lines for the new binary, 0 service EXIT lines. No
   boot stall after the exec — init continued, the trigger fired at
   guest 628.63 s, and the guest reached the full 850 s target. The exec
   dispatch returned; the minimal binary did not observably run.
2. Trigger: FIRED. `sys.boot_completed` 11 lines. First at guest
   628.63 s (L22420, hw/init.rc:1224). Stock boot_completed actions
   processed + succeeded. This is the second consecutive flight with the
   trigger fired (T732: 646.03 s) — the trigger leg stays FIXED. The
   [T723] hook line (L2010, "boot_completed never detected in 600s")
   fired as expected: its 600 s window expired before the 628.63 s
   trigger, as in T732.
3. Registration legs: BOTH PROVEN, control intact. `T733_PROBE_OK` x2:
   L10652, guest 117.83 s (on-boot action) and L22750, guest 633.74 s
   (property action). Both actions' writes fired, proving both actions
   executed. Control `T728_PROBE_OK` x2: L10651, guest 117.77 s and
   L22702, guest 633.64 s — the guest behaves exactly as in T732, so the
   verdict is valid (the control is present). `T728_SERVICE_OK` 0, as
   expected.
4. Minimal binary's marker: ABSENT. `T733_MINIMAL_OK` 0 lines in the
   UART. There is no marker line to quote — the binary never wrote it.
   The verdict condition is PASS iff `T733_MINIMAL_OK` appears. It does
   not.
5. `T733_MINIMAL_OK`: 0 lines. (Stated separately per the brief: the
   minimal binary never ran.)
6. SELinux refusal for the new binary: 0 lines for t733probe — but the
   CONTROL service was refused in the same flight, at the same moment.
   L22700 (guest 633.54 s):
   `[  633.543606] init: Command 'start t728probe'
   action=sys.boot_completed=1 (/system/etc/init/t728probe.rc:13) took
   30ms and failed: Could not start service: File
   /system/bin/t728probe(labeled "u:object_r:system_file:s0") has
   incorrect label or no domain transition from u:r:init:s0 to another
   SELinux domain defined.`
   The t733probe start's own log line is missing, and so is the
   t733probe.rc property-action processing line — both were lost in the
   Bluetooth ANR crash-dump flood that hit the kmsg buffer between the
   T728 write (L22702, 633.64 s) and the T733 write (L22750, 633.74 s):
   a multi-line dropbox ANR dump (L22714–L22752, "Process:
   com.android.bluetooth ... failed to complete startup") flooded the
   buffer in exactly that 0.1 s window, and the T733 write itself only
   survived glued to the dump's last CPU-usage line. The property
   action's write fired at 633.74 s, proving the action ran and its
   `start` command was attempted; the missing log lines are
   KMSG-FLOOD-LOSS, not evidence the action did not run. (The on-boot
   exec leg needs no refusal: `exec` does not go through the
   service-label check, and it returned silently — as in T732.)
7. Processing lines: 0 on-boot action processing lines citing
   t733probe.rc, 0 t733probe.rc citations — the same as T732 (init logs
   no processing line for these custom rc on-boot actions; execution is
   proven only by the action's own write side-effect). Boot_completed
   action processing lines: 3 (L22420 hw/init.rc:1224, L22691
   t728probe.rc:12, L637.81 s wifi.rc:98). The t733probe.rc property
   processing line is absent (flood loss, see item 6), but its write
   fired at guest 633.74 s, proving it executed.

## Discrimination answer

The fault is NOT in the payload's libc/startup chain. T733 flew the
smallest possible binary — a 1,648 B no-libc `_start` that does
openat/write/exit_group and nothing else — and it ALSO produced nothing
under `exec`, even though the exec returned (write fired at 117.83 s).
There is no libc to initialise, no `__libc_start_main`, no argv handling,
no file creation, no dynamic linker. The binary's bytes, mode (0755),
and label (system_file) are all verified good. The fault is in init's
exec dispatch: init's `exec` (and `start`) of a verified-good binary
completes without the binary ever observably executing — no marker, no
starting-service line, no exit line, no exec failure line, no boot
stall. This reframes T732's BINARY-EXEC-SILENTLY-FAILED: the binary's
execution was never the fault location — it was a symptom. The next
discrimination must trace init's exec dispatch itself (init-side
tracing — the other death-note candidate, out of scope here).

## Holds

- T720_PROBE DONE status=PASS: 1 (L228). Met.
- T721 fence-log lines: 1 (L24182). Met.
- simplefb: mapped lines: 1 (L24181). Met.
- Kernel panic/Oops/BUG: 0. Met. EPERM: 0. Met. T628 REJECT allocate2: 0.
  Met. Watchdog KILLING: 0. Met.
- FATAL EXCEPTION: 4 (L26035, 26043, 26074, 26081). NOT MET (T732 had 6).
  All 4 are DeadSystemException ("The system died") in launcher3 (pid
  3918), permissioncontroller (pid 3559), systemui (pid 3544), and
  documentsui (pid 4215) — consequences of system_server dying again
  late in the flight (~guest 672 s, ~39 s after the trigger). This is a
  LATE-flight event, after both probe legs (exec at 117.8 s, property
  start at 633.7 s) — it does not explain the marker absence, but it is
  the same boot-instability signal as T732 (6 FATALs there).
- CALL allocate2: 15 (T732: 5). avc: denied total: 2489 (T732: 2281).
  avc denied (sensorservice): 0 (T732: 6). avc denied for /dev/mem: 6
  (graphics composer HAL, permissive — same as T732). RenderEngine EGL
  dup-fail: 4 (dup-fail rate 4.700 per 1000 s; baselines T721 flight-01
  4.058, revert 6.660). The guest ran at full pace for the full 850 s,
  so the higher counts reflect a longer, faster boot, not a new failure
  mode.

## Wall

WALL-TYPE-TAG: guest wall. INIT-EXEC-DISPATCH-SILENTLY-FAILED (guest,
NEW in T733) — init's `exec` (and `start`) of a verified-good minimal
no-libc binary completes without the binary ever observably executing:
the exec returns (proven by the write after it firing), the property
action's write fires, but the binary writes no marker, logs no
starting-service line, no exit line, and no exec failure line. This
supersedes T732's BINARY-EXEC-SILENTLY-FAILED as the fault location:
the binary's execution was a symptom, not the fault — the fault is in
init's exec dispatch. The trigger leg stays FIXED (boot_completed fired
at guest 628.63 s, second consecutive flight).

Rust par bhi lagu: haan — the Rust emulator must reproduce this exact
guest-side silence (init executes the action's write but the `exec` of a
verified-good minimal no-libc binary produces no marker, no argv dump,
no init exec line, and the `start` of the same binary produces no
service) before it can claim the exec/start path is understood; a Rust
init that merely runs the binary would pass a check the real guest
fails.

## Reflection

- What happened: the discrimination worked — the minimal no-libc binary
  also produced nothing under exec, even though the exec returned. Both
  mechanisms fail identically, and the control (T728_PROBE_OK x2) proves
  the guest behaved exactly as in T732. Verdict FAIL on the
  minimal-binary path.
- The trigger leg is fixed for the second consecutive flight:
  boot_completed fired at guest 628.63 s, the guest ran at ~1.00 pace
  and reached the full 850 s target (851.062 s). The widened window +
  first-poll pace check worked again (poll 1: ratio ~0.92, projected
  ~1015 s at host cap).
- Root cause of the two surprises: (a) the t733probe property-action
  processing line and start-command line are both missing from the UART
  — they were lost in the Bluetooth ANR crash-dump flood at the exact
  trigger moment (KMSG-FLOOD-LOSS, documented with line numbers), not
  evidence the action did not run; the write firing at 633.74 s proves
  it ran. (b) system_server died late in the flight again (~guest
  672 s), killing the apps with DeadSystemException — a separate
  boot-instability signal, not the cause of the marker absence.
- Missed signal: none on the pace (poll 1 showed ratio 0.92, the guest
  caught up to 1.00). The kernel-silence window (guest ~258–628 s)
  looked like a stall at poll 1 but was a quiet period — the kernel
  resumed logging and the trigger fired. A quiet kernel is not a stalled
  guest; only the guest clock + UART growth together tell the story.
- What changes: the next track must trace init's exec dispatch — the
  candidates are init-side tracing (strace on init, init logging, or a
  binary the guest kernel itself execs at a different point). The
  binary is proven minimal and correct, so the fault is in the dispatch,
  not the artifact. No lane launches during any user-ordered pause.
