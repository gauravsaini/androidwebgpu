# T724 findings — EGL dispatch probe via init .rc service trigger — FAIL

## Verdict

**FAIL.** The probe never ran. The EGL dispatch question is UNANSWERED.

- `T723_PROBE`: 0 lines in the full 600 s probe flight.
- `t723probe` (service start, .rc citations, exec diagnostics): 0 mentions.
- SP1 (direct driver call), SP2 (META-EGL call), SP3 (sw_sync control): all 0.
- No `T723_PROBE DONE` summary. No dispatch answer.

## WALL-TYPE-TAG: ADDED-RC-SILENTLY-IGNORED

Rust par bhi lagu: haan/nahi
Answer: haan — the wall is a guest-side init mechanism, independent of the emulator.
The Rust emulator must solve the same registration question (or use a different
trigger route) before it can run guest probes via init .rc files. Any probe that
relies on an added /system/etc/init/*.rc file faces this wall in the Rust
emulator exactly as it does under QEMU.

## What was flown

| Artifact | SHA-256 (first 8) | Bytes | Role |
|---|---|---|---|
| Probe binary `t723-probe` | `4b275e48` | 68,792 | Byte-identical T723 probe, never modified |
| CPIO `t723-fix.cpio.gz` | `a9d6040a` | 53,391,088 | T723 CPIO (ramdisk hook + probe) |
| DTB `t723-modified.dtb` | `73d1904b` | 2,049 | T723 DTB |
| Kernel `t720-Image` | `70d3859d` | 20,396,544 | T720 fence-source kernel, unchanged |
| System image `t724-system.img` | `6b66cabe` | 2,059,816,960 | Revert `0ba6d6d3` + `t723probe.rc` |
| Vendor image `t722-vendor.img` | `5e0607ad` | 134,217,728 | T722 vendor (4 EGL stubs) |
| Trigger file `t723probe.rc` | `9c6b3e12` | 976 | NEW: service + 2 `on boot` exec diagnostics + `on property:sys.boot_completed=1` |

The flown system image SHA `6b66cabe` is confirmed by the probe flight's
`flight-end.json` (`inputs.system_img.sha256`). The trigger file inside it was
re-read on the box after the flight (debugfs cat): SHA `9c6b3e12`, regular file,
mode 0664, root:root, 976 B, 20 lines, LF endings, no CR characters. The file
was in the flown image, byte-identical to the built file.

## Flights

### Smoke flight `smoke-t724` — disk-gate exit, inconclusive

- Launched 17:55:54 AEDT 16 Nov 2026 (supervisor PID 41861, QEMU 41868).
- Ended guest 72.707 s / host 75.15 s: supervisor disk gate fired mid-flight
  (8.00 GiB < 8 GiB). Clean exit, QEMU exit 0, `flight-end.json` written.
- UART 9,124 lines, 768,431 B, SHA `2f523454698714f780cc597a9783cfe7503f6ea726454fa5457f7d26bc3eccca`.
- Result: 0 `t723probe` mentions AND 0 installd mentions — the flight ended
  before ANY trigger phase (post-fs-data fires ~76 s, early-boot ~89 s,
  installd early-boot ~99 s in the T723 flight). Inconclusive on the .rc
  question: the flight ended before triggers, not because the file was skipped.
- Holds sanity: 1 `[T723]` hook timeout line; `T720_PROBE DONE PASS` at L228
  (same line as the T723 flight).
- Disk cause: free space at launch was 8,390,516 KiB (gate open by only
  1,908 KiB — the 2 GiB `t724-system.img` copy had just landed); the disk slid
  ~2 MB during the 75 s flight and tripped the gate. Lane scratch was then
  cleaned (134 MB vendor.img copy + ~4 MB /tmp files), df 8,530,960 KiB.

### Probe flight `probe-flight` — complete 600 s window, probe never ran

- Launched 17:59:46 AEDT 16 Nov 2026 (supervisor PID 42164, QEMU 42168).
- Ended 18:09:54 AEDT (07:09:54 UTC). Host elapsed 605.9 s, guest max
  604.976 s, QEMU exit 0. `flight-end.json` written, clean supervisor exit.
- UART 28,381 lines, 3,294,964 B, SHA
  `e1bfd466c34b4c2b5d85cd72275b2d671fedd86348eb9eb988d0585d0584ea35`
  (matches `flight-end.json` `log_sha256`).
- df at launch 8,530,820 KiB (gate open ~139 MB). df after the flight
  8,057,612 KiB at 18:14 (below the gate — the disk slid ~467 MB during/after
  the flight; neural dir still growing), recovering to 8,504,064 KiB by 18:24.
  The gate state matters for the successor, not for this lane's verdict.

## Step results

- **Step 0 — Disk gate: PASS at launch.** df 10,988,536 KiB at 17:32
  (gate open ~2.5 GiB); 8,530,820 KiB at probe-flight launch (gate open
  ~139 MB). Static-first design honoured: Steps 1–2 completed before any
  flight; the smoke flight's gate bite was a clean exit, not a bypass.
- **Step 1 — Verify inputs: PASS.** All 13 guest-input SHAs re-hashed on the
  Mac; every one matches the brief. Embedded-vendor rule PASS: CPIO member
  `ramdisk-images/vendor.img` hashes to `5e0607ad`. QEMU command line read
  (T723 probe-flight `qemu-command.txt`): kernel + DTB + CPIO loader +
  virtio-blk system only (readonly), NO vendor drive, `rdinit=/early-ramdisk`,
  GDB 1258. No network fetch.
- **Step 2 — Trigger design + build: COMPLETE.** SYSTEM ROUTE decided
  (ramdisk route rejected: init's parsed config is process state, lost on
  exec). Service file `/system/etc/init/t723probe.rc` (976 B, SHA `9c6b3e12`):
  `service t723probe /first_stage_ramdisk/t723-probe` (class main, user root,
  group root, oneshot, disabled) + `on property:sys.boot_completed=1` →
  `start t723probe` + `on boot` exec diagnostics (toybox test -x on
  `/first_stage_ramdisk/t723-probe`, EXPECTED to succeed, and on
  `/t723-probe`, EXPECTED to fail). Built on the box with Linux debugfs only
  (no macOS debugfs anywhere): work copy SHA `0ba6d6d3` verified before the
  edit; neighbours byte-identical after (surfaceflinger `259d39f5`,
  installd.rc `3ea3466b`, hw/init.rc `60cb3d15`); `e2fsck -f -n` clean;
  file count 7342 → 7343; new image SHA `6b66cabe`. Transferred box → Mac by
  1 MiB-chunk block delta (4 differing chunks, 4,194,379 B patch, SHA
  `bc29e135`); Mac-side SHA after applying the patch: `6b66cabe` — MATCH.
  Box → Mac direct SSH fails (box key not authorized on the Mac), so the
  box → sandbox → Mac route stands. Supervisor re-pinned to the flown SHAs;
  scripts import-tested before launch.
- **Step 3a — Smoke: INCONCLUSIVE** (disk gate, see above).
- **Step 3a — Probe flight: FAIL** (probe never ran, see below).
- Steps 3b/3c NOT flown (probe-only branch; the dispatch point was never
  proven by probe evidence).

## The trigger evidence (the core of this track)

### The property was set, and the stock action for it ran

- `sys.boot_completed` occurs 5 times in the probe-flight UART (L21486,
  L21503, L22083, L22110, L22114).
- L21486: `[  498.413390] init: processing action (sys.boot_completed=1) from
  (/system/etc/init/hw/init.rc:1224)` — the stock `on
  property:sys.boot_completed=1` action processed at guest **498.413 s**,
  earlier than T723's 516.118 s.
- So the property-trigger MECHANISM works in this flight: the property was set
  and at least one stock action for it dispatched.

### No action from the added file ever executed

- `t723probe`: **0** mentions in all 28,381 UART lines (service start,
  .rc citations, exec diagnostics — all zero).
- `T723_PROBE`: **0**. SP1 DONE / SP2 DONE / SP3 DONE: **0**.
- The two `on boot` exec diagnostics (expected at guest ~94–100 s, when the
  boot action runs) never produced a `Command 'exec ...'
  (/system/etc/init/t723probe.rc:...)` line.
- The `on property:sys.boot_completed=1` → `start t723probe` never produced
  a `starting service 't723probe'` line, although the same property's stock
  action (1224) processed at 498.413 s — in the quiet post-flood window where
  log lines survive (the 1224 line itself survived).

### The boot trigger DID dispatch (the stock `on boot` action ran)

- The stock `on boot` action is `hw/init.rc:1104`. Its `class_start hal`
  (line 1210) and `class_start core` (line 1212) are the ONLY occurrences of
  those commands in any `/system/etc/init/**/*.rc` file (proven by rdump +
  grep of the whole directory from the revert image) and no vendor
  `/etc/init/**/*.rc` file contains any `class_start` at all.
- In the probe flight, vendor HAL services started at guest 94–97 s
  (`vendor.usb_default` 94.56, `vendor.wifi_hal_legacy` 94.90,
  `vendor.bluetooth-default` 95.25, `vendor.cas-default` 96.62,
  `vendor.contexthub-default` 96.87) and surfaceflinger (class core) started
  at 100.13 s. Those services can only start via the boot action's
  `class_start hal` / `class_start core`. **The boot action ran at ~94–100 s.**
- The `processing action (boot)` log line itself is absent — a kmsg logging
  loss, not a non-execution. The same loss class is proven in this flight
  (the `processing action (zygote-start)` line and the `starting service
  'zygote'` / `'netd'` lines are also absent, although zygote demonstrably
  started — system_server runs as pid 2892, then 4298) and in T723 (no
  post-fs-data / early-boot processing lines, although those actions
  demonstrably ran — installd started at 99.58 s). During the boot flood
  (guest ~66–200 s) the kernel log path demonstrably drops lines (bpfloader:
  2786 lines suppressed at guest 86.03 s; init: 3 lines at 9.22 s, 6 lines at
  13.61 s). A missing log line in guest 60–200 s is NOT proof of
  non-execution; only post-flood (guest >200 s) absences are probative.
- The probe's property trigger fires at 498.4 s — post-flood. Its absence
  there is probative: the action genuinely did not execute.

### The directory is parsed; no parse error was logged

- Second-stage init parses `/system/etc/init/*.rc`: proven by stock actions
  processing (installd.rc:108 early-boot at guest 90.61 s, post-fs-data
  actions at 30.3 s and 76.1–76.2 s, hw/init.rc:1224 at 498.413 s).
- No init `.rc` parse error appears anywhere in the UART (grep for
  `could not parse` / `parse error` / `error parsing` finds only an unrelated
  odrefresh metrics line). Init does log errors at ERROR level (fstab/vbmeta/
  sepolicy errors at 8–9 s, `service vndservicemanager not found` at 24.97 s,
  socket bind failures at 100.9 s) — a parse error at second-stage startup
  (~guest 16–20 s, before the flood) would very likely have survived.
- The file is findable by the glob (debugfs `cat` by path resolves it, so the
  directory entry exists), is a regular file, mode 0664, LF line endings.
- Init logs `Parsing file ...` only for /apex/... files (12 lines, guest
  66.0 s); there is NO per-file parse log for `/system/etc/init/*.rc`, so
  parse vs non-parse cannot be distinguished from the UART directly.

### Root cause (evidence level)

Second-stage init never executed any action from `t723probe.rc`, although the
file was present in the flown image byte-identical, the directory is parsed,
the trigger mechanism works (the boot action ran; the boot_completed property
action ran), and no parse error was logged. The non-registration mechanism is
NOT proven by this flight: it could be a silent parse rejection of this file,
a registration failure, or a trigger-matching failure — the UART cannot
distinguish them, because init emits no per-file parse record for
`/system/etc/init/*.rc`. What IS proven is that the .rc SERVICE-TRIGGER route
did not fail on its own merits as a design (the design's premises — init
parses the directory, the boot trigger dispatches, the property is set — all
held); it failed because the added file's actions were silently never
executed. The EGL dispatch question is therefore UNANSWERED, not answered in
the negative.

### Secondary anomaly: partial boot_completed dispatch

Only the FIRST boot_completed action (hw/init.rc:1224) processed. The other
boot_completed actions never processed in the full 605 s flight: dmesgd.rc:4
(unconditional `setprop dmesgd.start 1`), wifi.rc boot_completed actions
(T723: lines 30/46/51/53). In T723 they processed 2–3 s after 1224
(bootstat.rc:67 at 518.2 s, dmesgd.rc:4 at 518.6 s). bootstat.rc:67 is
conditional on `sys.bootstat.first_boot_completed=0` and may legitimately not
fire if the flag persisted; dmesgd/wifi have no such condition. The 1224
action itself is non-blocking (`bootchart stop`, `exec rm -rf /data/per_boot`,
`mkdir /data/per_boot`), so the queue did not stall on it.

### Boot-stability regression vs T723

system_server began self-dumping BEFORE boot_completed in this flight:

- Wall 23:23:57.445 (UART L18235) `libdebuggerd_client: started dumping
  process 2892` — guest ≈ 461.8 s (boot_completed action at guest 498.413 s
  = wall 23:24:34.022; the dump is 36.6 s earlier in wall time).
- Second self-dump wall 23:24:28.128 (L21178) — guest ≈ 492.5 s.
- boot_completed action guest 498.413 s (wall 23:24:34.022).
- FATAL EXCEPTION cascade wall 23:25:13.823–23:25:14.422 (L25198–L25325:
  RoleControllerService, main ×3, wmshell.main, SysUiBg) — guest ≈ 538–539 s.
- New system_server pid 4298 first line wall 23:25:48.360 (L26308) —
  guest ≈ 573 s.
- Guest times are estimates (± a few s; wall-clock and kernel-time rates can
  differ slightly under TCG).

In T723 the crash came AFTER boot_completed (zygote SIGKILL guest 560.16 s vs
boot_completed 516.118 s). The crash does NOT explain the boot-trigger
evidence (the boot action ran at ~94–100 s, ~360 s before the first dump),
but it may contribute to the partial boot_completed dispatch. This is a
boot-stability regression to note for the successor, not the root cause of the
probe failure.

### The T723 hook re-fired (expected)

L2012: `[T723] boot_completed never detected in 600s; probe not run` — the
early-ramdisk wait-loop (T723's broken detector) ran again and timed out, as
expected: it is in the flown CPIO. This re-confirms the T723 wall
(RAMDISK-MOUNT-BLIND) and confirms the CPIO flew.

## Holds (all intact)

Analyzer: `scripts/analyze_t724_probe.py flights/probe-flight/uart.log`
(report saved as `analysis-t724.txt`).

| Check | Required | Observed | Lines |
|---|---|---|---|
| T720_PROBE DONE status=PASS | ≥1 | 1 | L228 |
| T720_PROBE DONE status=FAIL | 0 | 0 | — |
| T721 fence-log | ≥1 | 1 | L23587 (`fd=10 tl=9`) |
| Invalid present fence | 0 | 0 | — |
| T628 REJECT allocate2 | 0 | 0 | — |
| T628 CALL allocate2 | — | 12 | L11113, L11117, L19069, L19073, L19077, L22840, L22963, L22967, L22971, L23000, L23141, L23211 |
| EPERM | 0 | 0 | — |
| simplefb: mapped | ≥1 | 1 | L23586 |
| Watchdog KILLING | 0 | 0 | — |
| sys.boot_completed | ≥1 | 5 | L21486, L21503, L22083, L22110, L22114 |
| Kernel panic/Oops/BUG | 0 | 0 | — |
| RenderEngine EGL dup-fail | — | 4 | L23852, L24057, L24357, L24449 |
| Dup-fail rate | — | 6.612 / 1000 s | 4 / 605.0 s (baselines: T721 flight-01 4.058, T721 revert 6.660) |
| avc: denied (two spaces) | — | 2083 | — |
| avc denied for /dev/mem | — | 3 | — |

The dup-fail count (4) and rate (6.612) sit between the T721 baselines and
match T723's context reading (4 / 6.449). Context only — the patch is
ineffective-or-unreached, same as T722/T723; no new harm signal.

## Variances

- V1: The `on boot` exec diagnostics' absence is WEAK evidence (their log
  lines would fall in the guest 60–200 s kmsg flood, where lines are
  demonstrably lost). The load-bearing evidence is the property trigger's
  absence at 498.4 s (post-flood) plus the 0 service-start lines. Stated here
  so the driver does not over-weight the `on boot` diagnostics.
- V2: Guest times for the system_server crash sequence (§Boot-stability
  regression) are estimates derived from wall-clock offsets against the
  boot_completed anchor (guest 498.413 s = wall 23:24:34.022); ± a few s.
- V3: The vendor `.rc` class_start grep used the T717 vendor image (same
  AOSP .rc set; the T722 vendor differs only in 4 EGL stub binaries + the
  composer .rc). The system `.rc` grep used the flown revert image and is
  complete. APEX `.rc` files were not grepped (payloads inside .apex files);
  no APEX file is known to `class_start hal`/`core`, and the T723 flight's
  identical APEX set showed the same services starting only after the boot
  action processed.
- V4: `boot_completed` line count is 6 (5 `sys.boot_completed` + the [T723]
  hook line mentioning boot_completed). The analyzer's `sys.boot_completed`
  count (5) is the exact-pattern count.

## What remains / successor direction

1. **The EGL dispatch question is UNANSWERED.** SP1 vs SP2, the dispatch
   point, and the mechanism for T722's zero effect are all still open.
2. **T725 (recommended): minimal-.rc registration experiment.** Before
   betting another 600 s flight on a trigger file, prove whether ANY added
   .rc file registers: a file with a single trivial action (e.g. `on boot` +
   `exec` writing a kmsg-visible marker, or a service whose first line is a
   UART marker) flown in a smoke that runs PAST guest ~120 s (past the first
   trigger) — the T724 smoke ended at 72.7 s, before any trigger, so it
   proved nothing about registration. If a minimal file registers, re-fly
   the probe with the proven file shape; if even a minimal file is silently
   ignored, the .rc route is dead and the successor must use a different
   trigger (e.g. a repaired guest-local wait on a later property, or a
   zygote-start-path trigger).
3. **Offline parser check (cheap, do first):** run the AOSP init parser (or a
   minimal init) against `t723probe.rc` (SHA `9c6b3e12`) to rule in/out a
   silent parse rejection of this exact file (e.g. the `exec - root root --`
   form, or the `service` block). If the parser rejects it, the fix is a
   file-shape change, not a new trigger route.
4. **Disk:** df was 8,504,064 KiB at 18:24 AEDT 16 Nov (gate 8,388,608 KiB)
   — marginal, sliding around the gate (8,057,612 KiB at 18:14). The
   successor needs the gate COMFORTABLY open before a 600 s flight; the
   T724 smoke's gate bite at 72.7 s wasted the registration proof.
5. **No fix was built** (probe-only branch; the dispatch point was never
   proven by probe evidence). Nothing to carry forward except the proven
   inputs (probe `4b275e48`, CPIO `a9d6040a`, DTB `73d1904b`, kernel
   `70d3859d`, vendor `5e0607ad`, system `6b66cabe`).

## Reflection

- **What happened (evidence):** the 600 s probe flight completed cleanly
  (guest 604.98 s, QEMU exit 0) and the probe never ran — 0 `T723_PROBE`, 0
  `t723probe`, 0 service start, 0 .rc citations. The guest booted,
  `sys.boot_completed` was set (first stock action at guest 498.413 s), and
  every hold from T720/T721 stayed intact.
- **Root cause:** second-stage init never executed any action from the added
  `t723probe.rc`, although the file was present in the flown image
  byte-identical, the directory is parsed by init, the trigger mechanism
  works (boot action ran at ~94–100 s; boot_completed property action ran at
  498.413 s), and no parse error was logged. The non-registration mechanism
  is unproven — the UART cannot distinguish silent parse rejection from
  registration failure from trigger-matching failure, because init emits no
  per-file parse record for `/system/etc/init/*.rc`.
- **Missed signal:** I assumed "file in the parsed directory + valid syntax
  = actions registered". The lane's design had no POSITIVE registration proof
  that survives the boot flood: the `on boot` exec diagnostics were the
  proof, but their log lines fall in guest 60–200 s where kmsg loss is a
  known class (T723 precedent: missing post-fs-data/early-boot processing
  lines). The first check (guest 165 s) correctly found 0 mentions, but I
  then spent ~10 min resolving whether the boot trigger had fired, instead
  of immediately separating the two questions — (a) did the trigger dispatch
  (answerable from service starts, not log lines) and (b) did the file
  register (only answerable post-flood, at the property trigger). The deeper
  miss: the smoke flight ended at guest 72.7 s on the disk gate, BEFORE any
  trigger phase, so it proved nothing about registration — and I launched the
  600 s flight without a registration proof in hand.
- **What changes:** (1) The successor brief must include a registration-
  proving minimal-.rc experiment (single trivial action, smoke past guest
  ~120 s) BEFORE the full probe flight. (2) The disk gate must be comfortably
  open before a 600 s flight — a gate bite at 72.7 s wasted the registration
  proof. (3) Record the kmsg flood-loss class in the brief's evidence rules:
  a missing log line in guest 60–200 s is NOT proof of non-execution; only
  post-flood (guest >200 s) absences are probative — service starts and
  post-flood property actions are the probative evidence, not processing
  lines. (4) Run the offline init-parser check on the exact file before
  designing the next trigger file.
- **Where it lives:** this reflection is in findings-t724.md §Reflection; the
  T725 successor brief must carry the minimal-.rc experiment as its Step 2
  and the flood-loss rule in its Standing evidentiary rules.

## Evidence index

- Probe flight UART: `flights/probe-flight/uart.log` (28,381 lines,
  3,294,964 B, SHA `e1bfd466c34b4c2b5d85cd72275b2d671fedd86348eb9eb988d0585d0584ea35`).
- Probe flight-end: `flights/probe-flight/flight-end.json` (guest 604.976 s,
  host 605.9 s, QEMU exit 0).
- Probe analysis: `analysis-t724.txt` (analyzer report, 28,381 lines).
- Smoke flight UART: `flights/smoke-t724/uart.log` (9,124 lines, 768,431 B,
  SHA `2f523454698714f780cc597a9783cfe7503f6ea726454fa5457f7d26bc3eccca`).
- Smoke flight-end: `flights/smoke-t724/flight-end.json` (guest 72.707 s,
  disk-gate exit).
- Trigger file: `guest-inputs/t724-system.img` (SHA `6b66cabe`) contains
  `/system/etc/init/t723probe.rc` (SHA `9c6b3e12`); build copy on the box at
  `/home/muse/t724-work/t724-system.img`; patch `t724.patch` (SHA `bc29e135`).
- Scripts: `scripts/supervisor_t724.py`, `scripts/launch_probe_t724.py`,
  `scripts/analyze_t724_probe.py` (import-tested before launch).
- T723 findings (input): `/Users/Shared/wt-track723-egl-dispatch/findings-t723.md`
  (SHA `2d1631fc`, Verdict FAIL, wall RAMDISK-MOUNT-BLIND).
