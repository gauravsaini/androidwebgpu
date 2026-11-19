# Track T729 — findings (RECOVERY-11 service-flight, labeled-payload re-fly)

## Verdict: FAIL

The labeled direct exec never ran — not because init refused it, but because
the trigger never fired: `sys.boot_completed` never became 1 inside the
600 s guest window, so the `on property:sys.boot_completed=1` action (which
starts `t728probe`) never ran. The SELinux-label hypothesis is UNTESTED by
this flight, not refuted. New wall: **BOOT-COMPLETED-NEVER-FIRED** (guest).

## Flight

- Flight `service-flight`, launched 19:43:32 AEDT 19 Nov 2026 (supervisor PID
  4411, QEMU pid 4418), ended 19:53:34 AEDT. Stop reason: "Guest target
  reached (600.6s >= 600s)". Host elapsed 601.43 s, guest log max 600.59 s,
  QEMU exit code 0, no QEMU left running.
- UART: `flights/service-flight/uart.log`, 18,375 lines, 2,034,071 B, SHA-256
  `2679eb703de1abaed2c12c956b9a4e61b1e34ef764d2ed3a18eb6290517f4b7b`
  (from `flight-end.json`). `qemu-stderr.log` 0 B.
- Inputs supervisor-verified at launch (no SHA errors): kernel `70d3859d`,
  DTB `73d1904b`, system image `568af6ac`, boot image `a9d6040a`, vendor
  `5e0607ad`. Flown image is the labeled image (binary
  `u:object_r:system_file:s0`, driver-verified Step 3 PASS 19:37 AEDT).
- Analyzer: `scripts/analyze_t729_probe.py` (SHA `eebe8e90`), exit 0;
  output `flights/service-flight/analysis-t729.txt` (141 lines, SHA `cb57e6b3`).
  Decisive counts below were independently re-checked with `grep -c` on the
  UART and match the analyzer exactly.
- Polls: 348 total. Poll 348 (19:43:22 AEDT) GATE_OPEN, avail 78,537,756 KiB
  vs threshold 9,769,216 KiB. R10 polls 329–347 per-poll records were lost in
  the 18:03 /Users/Shared wipe (recorded as UNKNOWN in `progress-t729.txt`;
  only poll 347 evidenced: 17:58:21 AEDT, avail 153,624 KiB).

## Adjudication (Step 5, in order)

1. **Trigger fired? NO.** `sys.boot_completed`: 0 lines in the UART
   (analyzer + independent `grep -c`). `boot_completed`: 1 line only — L2008,
   the [T723] hook: `[T723] boot_completed never detected in 600s; probe not
   run`. No init `processing action (sys.boot_completed=1)` line at all.
   (T728 had 2: `hw/init.rc:1224` at guest 563.938 s and `bootstat.rc:67` at
   566.401 s.) TRIGGER-FIRED-FIRST: the trigger never fired, so no judgement
   of the trigger action is possible.
2. **Registration marker? YES, leg 1 only.** `T728_PROBE_OK` x1: L10676,
   guest 113.734 s — the `on boot` action in `t728probe.rc:17` fired, so init
   did parse the rc. T728 had x2 (109.049 s + 569.563 s); the second is the
   boot_completed action's re-print and is absent here because the trigger
   never fired.
3. **Service started? NO.** `starting service 't728probe'`: 0. Service EXIT
   line (`Service 't728probe' ... exited with status`): 0. No pid, no
   runtime, no exit code. There is no exit line to judge from (T727 lesson
   observed: nothing started at all).
4. **Argv dump? NONE.** `T728_ARGV`: 0. `T728_ARGC`: 0. No argv lines exist
   to quote — the payload never ran, so there is no handoff to examine.
5. **`T728_SERVICE_OK`? 0.** Verdict FAIL by the brief's criterion (PASS
   iff `T728_SERVICE_OK` appears in the UART).
6. **SELinux refusal? NONE.** `Could not start service`: 0.
   `incorrect label|no domain transition`: 0. Unlike T728 (refusal L22099,
   guest 569.484 s, binary labeled `u:object_r:unlabeled:s0`), init never
   refused the labeled binary — because init was never asked to start it.

## Why the trigger never fired (mechanism)

- Boot milestones, guest time: zygote ~94.6 s; SystemServer entered
  167.151 s; `boot_progress_pms_ready` 208.348 s; `boot_progress_ams_ready`
  555.040 s (L16666). After AMS ready, system_server entered a watchdog
  dump storm: `libdebuggerd_client: started dumping process` for ~10
  processes (2903, 2785, 2787, 2789, 2822, 2759, 872, 2793, 877, 3574) at
  23:25:23–23:25:38, and the pre-watchdog crash file
  `/data/system/dropbox/system_server_pre_watchdog@1699658751667.txt.gz` was
  written (END CRASH FILE line in the UART tail). ActivityManager logged
  `Slow operation` lines (startProcess 2683–2693 ms) in the same window.
  The 600 s window expired with `boot_completed` still unset.
- T728 comparison (vault UART, `grep -a` verified): `system_run` 180.315 s,
  `pms_ready` 220.723 s, `ams_ready` 484.500 s, first boot_completed action
  563.938 s — 79.4 s after AMS ready. T729 was FASTER to `system_run`
  (167.2 s vs 180.3 s) and `pms_ready` (208.3 s vs 220.7 s), but 70.5 s
  SLOWER to `ams_ready` (555.0 s vs 484.5 s). At T728's pace (79.4 s from
  AMS ready to boot_completed), T729's boot_completed would land at ~634 s
  — past the 600 s window. The window, not the label, is what failed here.
- KMSG-FLOOD-LOSS: the boot_completed absence is post-flood (guest
  555–600 s) — probative, not flood loss. The missing second `T728_PROBE_OK`
  (expected ~569 s, post-flood) is likewise probative.

## Parser

- parser: the offline init parser (`scripts/parse_init_rc.py`) run first-hand
  by this lane on the exact flown rc bytes (`guest-inputs/t728probe.rc`, SHA
  `f61b9c0f`, 691 B, byte-identical to T728's): **VERDICT ACCEPT (0 errors;
  1 service(s), 2 action(s), 0 import(s))**. Structure: service `t728probe`
  `/system/bin/t728probe` (line 5, class main, user root, group root,
  oneshot, disabled); `on property:sys.boot_completed=1` (line 12): `start
  t728probe` + write `T728_PROBE_OK`; `on boot` (line 17): write
  `T728_PROBE_OK`.

## Label

- The binary's label (`u:object_r:system_file:s0`) was verified by the driver
  pre-flight via box debugfs read-back (Step 3 PASS, 19:37 AEDT 19 Nov). It
  was never exercised in this flight: init never attempted the start, so the
  label's correctness remains unproven either way. The T728 wall
  (SERVICE-SELINUX-LABEL-REFUSED) is neither re-proven nor refuted here.

## Holds / controls

- `T720_PROBE DONE status=PASS`: 1 (L228); FAIL: 0. Hold met.
- `T721 fd=` fence-log lines: 0 — hold (>=1) NOT met. But the same count is
  0 in the certified T728 flight (vault `grep -c`): the fence lines appear
  in the T721 flight itself, not in later service-flights. Analyzer
  expectation artifact, not a regression.
- `simplefb: mapped`: 0 — hold (>=1) NOT met. But T728 is also 0, and both
  flights carry the bind line (`simple-framebuffer 5f000000.framebuffer:
  fb0: simplefb registered!` — T729 L97, guest 0.333 s; T728 L97, 0.346 s).
  The framebuffer bound; the analyzer's `mapped` pattern matches no line in
  either flight. Artifact, not a regression.
- `CALL method=allocate2`: 2 (L11097, L11102) vs T728's 12 — context only.
  `REJECT method=allocate2`: 0. EPERM (`simplefb: open /dev/mem failed`):
  0. `Invalid present fence`: 0. `Watchdog: ... KILLING`: 0. `Kernel
  panic|Oops:|BUG:`: 0. `FATAL EXCEPTION`: 0.
- T727 controls: `T727_SERVICE_OK` 0, `T727_PROBE_OK` 0, `t727probe` 0 (old
  rc replaced). T723 probe: 0 (not flown); [T723] hook line 1 (L2008).
- `avc:  denied` (two spaces): 1409 total (T728: 1898); `/dev/mem`: 0;
  sensorservice: 6. `sw_sync` lines: 7 (L195–226, early).
- RenderEngine dup-fail: 0; rate 0.000 per 1000 s (T721 baselines 4.058 /
  6.660) — context only.
- Marker uniqueness: the T728 markers in this flight are this track's
  evidence (same binary, same rc, re-flown); T727 markers are 0.

## Wall

- WALL-TYPE-TAG: **guest**. New wall: **BOOT-COMPLETED-NEVER-FIRED** —
  `sys.boot_completed` never became 1 within the 600 s guest window, so the
  `on property:sys.boot_completed=1` action (`start t728probe`) never ran.
- Rust par bhi lagu: **haan** — the same Android image must reach
  boot_completed inside the flight window on any engine; the fix (a longer
  guest window and/or a faster boot) applies to both engines.

## Successor note

- Do not re-litigate the label until the trigger fires. Re-fly with a guest
  window that covers this boot's pacing: T728's pace suggests ≥640 s guest;
  this flight's pacing suggests ~634 s at minimum — recommend a 700 s guest
  target (with the host cap raised to match). Alternatively speed the boot
  (the pms_ready→ams_ready stretch cost 347 s here vs 264 s in T728).
  The labeled image (`568af6ac`) and rc (`f61b9c0f`) are unchanged and ready
  to re-fly as-is.

## Deliverables

- `findings-t729.md` (this file), `outcome-t729.txt` (`FAIL`), 
...[truncated 1583 chars]