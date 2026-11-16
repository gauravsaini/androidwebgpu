# Findings — Track T726: re-fly the T723 probe with the proven file shape

## Verdict

**Verdict: FAIL**

The probe never ran in a full 600 s guest window: 0 `T723_PROBE` lines, no DONE
summary, no SP1/SP2/SP3 lines. The EGL dispatch question (R1/R3/R4, open since
T722) is UNANSWERED — not answered negatively. The flight itself completed
cleanly (guest target reached, QEMU exit 0), so the failure is in the guest
dispatch path, not the harness.

## Flight record (Step 4)

- Flight `probe-flight`, launched 05:05:43 AEDT 17 Nov 2026 (supervisor PID 87893,
  QEMU pid 87904), ended 05:15:44 AEDT. Host elapsed 600.93 s, guest 600.168 s.
- Stop reason (flight-end.json): "Guest target reached (600.2s >= 600s)".
  QEMU exit code 0. No orphans (PID 88707 gone at close-out check).
- UART: `flights/probe-flight/uart.log`, 27,917 lines, 3,258,567 B,
  SHA `e9d3d30e522cb7d0f1ba0eba44a9016567dc393a84444569400afe37e99fcc1e`
  (matches flight-end.json).
- Inputs (supervisor-verified at launch, flight-end.json): kernel Image
  `70d3859d`, DTB `73d1904b`, boot image (CPIO) `a9d6040a`, system image
  `a7141b45`, vendor `5e0607ad`. CPIO embedded `ramdisk-images/vendor.img`
  re-hashed pre-flight: `5e0607ad` MATCH. QEMU command line: system image only
  as a virtio-blk drive; NO vendor drive (guest consumes the CPIO-embedded
  vendor).
- Disk: gate opened at poll 26 (05:02:33, 10,044,940 KiB; re-read 05:02:55
  10,022,788 KiB — the +2.84 GiB jump confirmed real, not APFS lag). Launch
  margin ~1.50 GiB above the 8 GiB supervisor gate. Disk slid to 9,352,896 KiB
  mid-flight and recovered to 9,566,552 KiB; the supervisor disk gate never
  fired. No disk bite.

## TRIGGER-FIRED-FIRST check (reported BEFORE judging the property action)

The trigger `sys.boot_completed=1` FIRED. This is not a T725-style
trigger-never-fired case.

- `sys.boot_completed` lines: 7 (L21634, L21734, L21764, L21765, L22231,
  L22240, L22244).
- First property processing: L21634, guest 501.540118 s —
  `init: processing action (sys.boot_completed=1) from
  (/system/etc/init/hw/init.rc:1224)` (stock first action).
- Further stock actions processed after the trigger: bootstat.rc:67
  (L21734, guest 503.848952 s), dmesgd.rc:4 (L21765, guest 504.305806 s),
  wifi.rc boot_completed commands (L22231+, guest ~508.4 s, all succeeded).
  At least four stock boot_completed actions processed — the T724
  "only the FIRST boot_completed action processed" stall is REFUTED for this
  flight.
- Boot timing: first CALL allocate2 guest 109.827116 s (L11064);
  FinishBooting wall 23:24:22.689; `sys.boot_completed=1` at guest 501.540 s.
  system_server forked and completed boot (no Watchdog KILLING, no panic).

## What the added file did (registration: PROVEN again)

- `T726_PROBE_OK` marker lines: 2.
  - L10606, guest 104.493266 s — the `on boot` action. Flood-window
    (guest 60–200 s) but survived; sufficient proof per the T725 reflection
    (flood loss deletes lines, never fabricates a unique-tag kmsg line).
  - L22116, guest 506.508080 s — POST-FLOOD, probative. The `on boot` action
    fires once per boot, and no other writer of `T726_PROBE_OK` exists in the
    guest, so this line is the property action's `write` command executing,
    inside the boot_completed action sequence (dmesgd 504.306 s, wifi
    508.378 s).
- The trigger file therefore REGISTERS and its actions EXECUTE — both writes
  fired. The T724 wall ADDED-RC-SILENTLY-IGNORED is REFUTED for this file
  shape: in T724 zero actions of the added file executed; here both did.

## What did NOT happen (the failure)

- `T723_PROBE` lines: 0. DONE summary: 0. SP1/SP2/SP3 DONE: 0.
- `init: starting service 't726probe'`: 0 lines in the whole UART.
- `t726probe` citations (any kind): 0. `t723-probe` citations: 0.
  `first_stage` citations: 0. `disabling` lines: 0.
- No `init: processing action (sys.boot_completed=1) from
  (/system/etc/init/t726probe.rc:12)` line — but this absence alone does NOT
  prove the action did not run: the wifi boot_completed action's commands
  also executed with NO processing line in this UART, so init's
  "processing action" logging does not cover every executed action here.
  The marker (write executed) is the proof the action ran.
- The early-ramdisk hook line L2014: `[T723] boot_completed never detected in
  600s; probe not run` — with the live mechanism beside it (guest ~31 s):
  `/early-ramdisk[268]: /system/bin/toybox: inaccessible or not found`.
  The ramdisk-side wait never saw the property (T723 RAMDISK-MOUNT-BLIND,
  confirmed live again in this flight). That is the hook's path, not the
  second-stage service path — the two must not be conflated.

## The failure mechanism (discriminated)

The property action RAN its `write` command (marker 2, post-flood) but its
`start t726probe` command produced NO observable effect: no service start,
no init error, no probe output, in the remaining ~94 guest-seconds of the
window after the marker (506.5 s to 600.2 s). The failure is isolated to the
SERVICE START leg, not registration and not the trigger.

What is PROVEN about the start leg: it left no trace of any kind.
What is NOT proven: whether `start` executed and failed silently, or never
executed. Two candidate mechanisms, stated as hypotheses:

- (a) The binary path is not reachable in second-stage init. The design
  assumed guest init bind-mounts the first-stage ramdisk at
  `/first_stage_ramdisk`, but NO line in this UART proves that mount exists
  (0 `first_stage` citations), and the path's availability in second-stage
  init was never verified from the guest. If the binary is absent there,
  `start` fails — but init's missing-binary error, if emitted, left no trace
  in this UART either.
- (b) init's `start` of this service definition (disabled, oneshot, class
  main, user root) from a property action silently no-ops.

The successor discriminates (a) from (b) in one flight: re-fly the identical
property-action shape with the service command replaced by a known-good
binary (`/system/bin/toybox echo` to a distinct marker, the T725 service
shape). If the toybox service runs, the start leg works and the fault is the
probe path (a) — then move the probe binary to a known-good path or prove
the mount. If the toybox service also never starts, the fault is the start
leg itself (b).

## Holds (exact patterns, analyzer `analysis-t726.txt` SHA 93c18fd6)

- T720_PROBE DONE status=PASS: 1 (L225) — required >=1: MET.
- T721 fence log: 1 (L23738, `T721 fd=10 tl=9`) — required >=1: MET.
- Invalid present fence: 0 — required 0: MET.
- T628 REJECT allocate2: 0 — required 0: MET.
- CALL allocate2: 12 (first L11064, guest 109.827 s).
- EPERM: 0 — required 0: MET.
- simplefb: mapped: 1 (L23737, `0x5f000000 1280x720`) — required >=1: MET.
- Watchdog KILLING: 0 — required 0: MET.
- Kernel panic / Oops / `BUG:`: 0 — required 0: MET.
- FATAL EXCEPTION: 6 (L25590–L25657, wall 23:25:22) — system_server
  self-dump cascade, same class as T724 (also 6). Context, not a hold
  criterion: boot completed and the window ran its course.
- RenderEngine dup-fail: 4, rate 6.665/1000 s — matches the T721 revert
  baseline (6.660), above T721 flight-01 (4.058). Context only: the T722
  EGL-stub-ineffective finding stands; the fence path itself is healthy
  (T721 fence log present).
- avc:  denied (two spaces): 2044; /dev/mem: 6. Context only.

## WALL-TYPE-TAG

WALL-TYPE-TAG: SERVICE-START-SILENTLY-FAILED (guest)

The T724 wall ADDED-RC-SILENTLY-IGNORED is REFUTED for this file shape (both
actions executed). The new wall is the service-start leg: a property action
whose `write` executes while its `start` leaves no trace. Type: guest
(engine-independent init dispatch — fix once, both engines reuse).

Rust par bhi lagu: haan

## Reflection (REFLECT-ON-EVERY-PROBLEM)

- What happened (evidence): full 600 s window flown; trigger fired at
  501.540 s; both file actions' writes executed (markers 104.493 s and
  506.508 s); the probe binary never ran (0 T723_PROBE); the service never
  started (0 citations of any kind for t726probe / t723-probe / first_stage).
- Root cause: the service-start leg failed silently. The lane's design
  assumed the probe path `/first_stage_ramdisk/t723-probe` is reachable from
  second-stage init, and that assumption was never proven from the guest —
  Step 1 verified the binary is present in the CPIO (host-side), not that
  the second-stage root exposes it at that path.
- Missed signal: the T725 service (`t725probe`, toybox echo) was never
  trigger-tested either — boot_completed never fired in T725 — so the
  "service starts from a property action" leg was unproven when this lane
  built on it. The brief carried the T725 shape as "proven" for
  registration, which is correct, but the service-start leg rode along
  unproven. The distinction (registration proven / start unproven) was not
  drawn sharply enough in the Step 2 design.
- What changes: the successor discriminates with a known-good binary in the
  identical shape before trusting any probe path again; any future service
  command path must be proven reachable from the guest context that starts
  it (a second-stage stat/listing, not a host-side CPIO membership check).
  The brief template's Step 2 should state the two legs separately:
  registration (marker) and service start (starting-service line + binary
  output), each with its own positive check.
- Where it lives: this file; the successor brief; the standing rules
  (PATTERN-PROOF: the CPIO-membership check proved the binary exists, not
  that second-stage init can reach it — the check must name the consuming
  context).

## Successor direction

Re-fly the identical property-action shape with a known-good service binary
(`/system/bin/toybox echo` to a distinct marker). If the toybox service
starts and marks, the start leg works and the fault is the probe path — then
prove `/first_stage_ramdisk` from second-stage (or move the probe binary to
`/system/bin`). If the toybox service also never starts, the fault is the
start leg itself — then try a single-action file or a different trigger
property. Either way the EGL dispatch question stays open until a probe
binary actually runs; display completion / frame commit follows only after
the probe's DONE verdict lands.

## Evidence index

- UART `flights/probe-flight/uart.log` SHA e9d3d30e (27,917 lines).
- Analyzer `analysis-t726.txt` SHA 93c18fd6 (119 lines).
- flight-end.json (inputs + stop reason, QEMU exit 0).
- supervisor log `supervisor_probe-flight.log`.
- progress-t726.txt (Steps 0–5 + close-out); heartbeat.json CLOSED FAIL.
