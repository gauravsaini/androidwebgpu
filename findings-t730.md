# Track T730 — findings (labeled-payload re-fly, 700 s guest window)

Verdict: FAIL

## Flight

- `service-flight` 20:12:42–20:24:28 AEDT 19 Nov 2026 (09:12:42–09:24:28Z).
  Stop reason: "Guest target reached (705.1s >= 700s)". Host elapsed 706.04 s,
  guest log max 705.059 s, QEMU exit code 0. UART 33,588 lines, 3,970,988 B,
  SHA `f400ac5e1ea791bd8a9e6d45693dfc231607a67509a39e0966c58fe0765f752e`.
- Inputs (from `flight-end.json`, all match T729): kernel `70d3859d`
  (t720-Image, 20,396,544 B), DTB `73d1904b` (t723-modified.dtb, 2,049 B),
  CPIO `a9d6040a` (t723-fix.cpio.gz, 53,391,088 B), system.img `568af6ac`
  (t729-system.img, 2,059,816,960 B), vendor `5e0607ad` (t722-vendor.img,
  134,217,728 B). One variable changed vs T729: the window (700 s guest /
  800 s host). Image, patch, rc, binary byte-identical to T729 by design.
- Parser citation: parser VERDICT ACCEPT (0 errors; 1 service, 2 actions,
  0 imports) on the exact rc bytes `f61b9c0f` (Step 2, 20:12 AEDT 19 Nov 2026).
  The rc parses; the failure is in the guest start leg, not the rc.

## Adjudication (brief Step 5, in order)

1. Trigger FIRED — `sys.boot_completed=1` first processed at guest
   495.717 s (UART L21325, `hw/init.rc:1224`). `sys.boot_completed`: 8 lines
   (L21325, L21500, L21511, L21513, L21832, L33490, L33491, L33492).
   The 700 s window worked. T729's wall BOOT-COMPLETED-NEVER-FIRED is
   CLOSED: the trigger fires inside 700 s.
2. Registration — `T728_PROBE_OK` x2: leg 1 guest 104.700 s (L10671, on-boot
   action); leg 2 guest 500.955 s (L21648, property action's write command).
   Both legs proven.
3. Service started? NO — "starting service 't728probe'": 0; service EXIT
   line ("Service 't728probe' ... exited"): 0; "Command 'start t728probe'":
   0; `t728probe.rc` citations in the whole UART: 0; all `t728probe` lines:
   0. No pid, no runtime, no exit code. Judged from the exit line per the
   T727 lesson — the exit line is absent.
4. Argv dump: NONE — `T728_ARGV` 0, `T728_ARGC` 0. There are no argv lines
   to quote; the payload never ran far enough to print them.
5. `T728_SERVICE_OK`: 0.
6. SELinux refusal: NONE — "Could not start service": 0; label /
   domain-transition lines: 0. The label is STILL UNTESTED (never refused,
   never started).

## Mechanism

The trigger fired (495.717 s) and the property action's write command
executed (`T728_PROBE_OK` #2 at 500.955 s, 5.2 s after the trigger) — so the
action ran. The `start t728probe` command in the SAME action produced zero
observable effect: no start, no exit, no refusal, no rc citation. This is
the T726 wall SERVICE-START-SILENTLY-FAILED, recurring verbatim: the action
executed, the service never started, init logged no error.

- T728's "refutation" of the T726 wall was narrow: labeling causes get
  explicit refusals (T728: "Could not start service" + `unlabeled` label,
  L22099). T730's binary IS labeled (`u:object_r:system_file:s0`, Step 1 /
  Step 3 verified on the flown image) and the start still fails silently —
  so the silent-start failure is NOT explained by the label. The
  SELinux-label hypothesis remains UNTESTED.
- Open anomaly: init never logged
  "processing action (sys.boot_completed=1) from
  (/system/etc/init/t728probe.rc:12)". The 5 processing lines cite
  hw/init.rc:1224 (495.717 s), bootstat.rc:67 (498.321 s), dmesgd.rc:4
  (498.544 s and 703.400 s), flags_health_check.rc:7 (703.442 s),
  logd.rc:34 (703.457 s). The action's write ran without its processing
  line — post-flood (guest >200 s), so the absence is probative per
  KMSG-FLOOD-LOSS. Two readings: a logging gap in init's action dispatch,
  or a dispatch path that doesn't log. It does not change the verdict —
  the marker is positive guest-origin proof the action's write executed;
  the start leg still produced nothing.
- Secondary observation: the first system_server (pid 2885) died mid-flight
  — watchdog dump storm from guest ~456–457 s (kernel lines just before
  L18108), DeadSystemException cascade (5 FATAL EXCEPTIONs, L25324–L25382,
  "The system died") at guest ~548 s, restart as pid 4358 (system_run
  589.139 s, pms_ready 616.738 s, ams_ready 652.932 s — the ONLY ams_ready in
  the flight; pid 2885 never logged one). Boot instability is real but does
  NOT explain the silent start: the start failed at ~501 s, before the
  death at ~548 s. At guest 703.400–703.457 s the boot_completed actions
  re-processed (dmesgd, flags_health_check, logd) — t728probe.rc:12 was NOT
  among them and no third `T728_PROBE_OK` appeared.
- Correction to the T729 reading of the [T723] hook line:
  "[T723] boot_completed never detected in 600s; probe not run" (L2004)
  sits at guest ~30.5 s, surrounded by early-ramdisk /dev/null errors
  (L2001–L2003, "can't create /dev/null: No such file or directory"). The
  hook's wait loop exits immediately (broken /dev/null) and prints the
  timeout message — it is NOT a 600 s observation. In T729 the conclusion
  (never fired) was independently true; the hook line itself was never the
  evidence.

## Holds (regression vectors)

- `T720_PROBE DONE status=PASS`: 1 (L226); FAIL 0.
- T721 fence: "T721 fd=10 tl=9" (L23598); simplefb mapped 0x5f000000
  1280x720 stride=1280 bpp=4 (L23596).
- `CALL method=allocate2`: 15; T628 REJECT 0; EPERM 0; Kernel panic / Oops /
  `BUG:` 0; Watchdog KILLING 0; Invalid present fence 0.
- T727 markers 0; `T723_PROBE` 0; `T728_PROBE_OK` 2; `sys.boot_completed` 8.
- `avc:  denied` (two spaces) 2570 (T729: 1409, T728: 1898); /dev/mem 6;
  sensorservice 0. sw_sync 242. RenderEngine EGL dup-fail 4, rate 5.673 per
  1000 s (T721 baselines 4.058 / 6.660) — in range.
- Independent `grep -c` reproduces the analyzer counts exactly:
  T728_PROBE_OK 2, sys.boot_completed 8, T728_SERVICE_OK 0, T728_ARGV 0,
  "Could not start service" 0, "starting service 't728probe'" 0, service
  exit line 0, avc 2570, CALL allocate2 15, T720 PASS 1, simplefb mapped 1,
  "T721 fd=" 1, Kernel panic 0, FATAL EXCEPTION 5.

## Wall

- WALL-TYPE-TAG: guest
- Wall: SERVICE-START-SILENTLY-FAILED (guest) — the T726 wall, recurring.
  The property action executed (marker proof) yet the service it starts
  never started and init logged no error.
- Rust par bhi lagu: haan — type-1 (guest) wall. The Rust emulator's init
  must reproduce / observe the same silent start-failure behaviour for this
  probe shape; a Rust init that starts the service where the QEMU guest's
  init stays silent would diverge from the oracle.

## Reflection

- What happened: the window fix worked (trigger fired at 495.7 s), but the
  next leg — the start itself — failed silently, the same wall as T726.
- Root cause: unknown, not guessed. Init executed the property action's
  write but the `start` in the same action left no trace. The label is
  ruled out as the explanation (binary is labeled; T728 proved labeling
  failures are loud, not silent).
- Missed signal: the T726 wall was treated as refuted by T728; it was only
  narrowed. A recurring-wall check — does the wall reappear when its
  supposed cause is removed? — should have been the successor's first
  question.
- What changes: the successor must discriminate the start leg itself — e.g.
  start the service from a different trigger (on-boot direct start, or a
  manual `start` from the shell) to separate "action dispatch" from
  "service start"; and init's action-dispatch logging gap (missing
  t728probe.rc:12 processing line) is a named open question for the
  successor.
- Where it lives: this file + `DEATH_NOTE-T730.md`.

## Evidence SHAs

- UART `f400ac5e1ea791bd8a9e6d45693dfc231607a67509a39e0966c58fe0765f752e`
  (33,588 lines, 3,970,988 B)
- Analysis `cce87a0eae1a8294a79ad99709286587f9e80665c82ec4a04fde756929b01c39`
  (144 lines, `flights/service-flight/analysis-t730.txt`)
- flight-end.json `3e565a858b2c2810cfc4400f57c4f67b601c752d1c9a26d3032dbe3dcb74b046`
- preflight.json `04baa41bdb1a9818f2ec79b87b22434529c15818cedbaaa0b2de6989311884a6`
- qemu-command.txt `83bda4d0efce4c4ced42211382e91cacd96f53d16659616d828708035fedf0aa`
- qemu-stderr.log `d30e97a236bd1d3d5e9dd86887400a41d94e84c7bce67d9248f884ad4be8fad5`
- uart.log.gz `b980b321ccd1fbacd986a1f7f24b16cdb795b3045eb5560d65f9a75d37c5358c`
- uart.log.numbered.txt `56dfdf72a9e09e9f685a2acae9578fea485b947b3781655f74f1df76480ca86f`

## Verdict

Verdict: FAIL
