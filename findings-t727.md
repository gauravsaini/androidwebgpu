# Findings — Track T727: toybox-service discriminator (service-flight)

## Verdict: FAIL

The toybox service produced no `T727_SERVICE_OK` marker in a full 600 s guest window
although the trigger fired and the action marker fired twice — so the PASS criterion
(marker in the UART) was not met. The service DID start (init's own exit line proves
pid 3992 ran 2.466 s), but its payload exited 127 without writing the marker or any
observable error. The start leg is therefore PROVEN to work and hypothesis (b) in its
"init's `start` silently no-ops" form is REFUTED; the silent failure moved one level
deeper, to the service payload exec. The probe-path question (a) remains UNANSWERED:
no payload has ever produced a marker from a service context, so we still do not know
whether a working payload would mark.

## The discrimination, answered in part

- (a) Probe path unreachable from second-stage init: UNANSWERED. The payload that
  would test it (toybox echo to kmsg) never produced output, so reachability of a
  service payload is still unproven from this context.
- (b) init's `start` of this service definition silently no-ops: REFUTED. Init DID
  start the service: L21969 `init: Service 't727probe' (pid 3992) exited with status
  127 oneshot service took 2.466000 seconds in background`. A service that never
  started has no pid, no runtime, and no exit line.
- New finding: the payload exec fails at exit 127. New wall SERVICE-PAYLOAD-EXIT-127
  (guest) — see Wall section below.

## Trigger-fired-first check — sys.boot_completed

`sys.boot_completed` count: 9 lines (L21574, L21689, L21690, L21709, L21717, L21965,
L21975, L21980, L22002). First property processing: L21574, guest 503.881 s —
`init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1224)`
(stock first action). The boot_completed action sequence then ran in full:
bootstat.rc:67 (L21690, guest 506.118 s) and its exec_background (L21709, 506.444 s,
325 ms, succeeded); dmesgd.rc:4 (L21717, 506.556 s); wifi.rc boot_completed commands
(L21965/L21975/L21980/L22002, guest 511.506–512.215 s, all succeeded). Our action's
`write` executed between dmesgd and wifi: L21890 `T727_PROBE_OK`, guest 509.652 s.
No "processing action" line cites t727probe.rc — consistent with T726 (most
boot_completed actions, including wifi's, execute with no processing line; the
marker is the proof the action ran). Boot timing: Zygote START L9931 (guest ~89 s);
first CALL allocate2 L11086, guest 109.328 s; system_server pid 2884 running
(dumping processes from 23:23:56). Conclusion: the trigger FIRED, the boot was
healthy, and the property action RAN before any judgement of the service.

## Step 4 — service flight (the discrimination)

- Launched 23:55:16 AEDT 17 Nov 2026, ended 00:05:24 AEDT 18 Nov (13:05:24Z).
  Guest target reached (604.9 s >= 600 s); guest 604.944 s, host 606.19 s, QEMU
  exit 0, no orphans. UART 27,482 lines, 3,208,505 B, SHA
  `8b1dbb2e0839f910630197eaa6a76270354ef054166bcd09ae7e820a98dd8aee`.
  flight-end.json stop "Guest target reached (604.9s >= 600s)". QEMU 11.1.2.
- Gate opened at recovery-5 poll 25 (23:54:17 AEDT, df 10,006,060 KiB >= flight
  threshold 9,769,216 KiB); launched 23:55:16 at df 9,860,780 KiB (margin above the
  8 GiB supervisor gate ~1.41 GiB at launch). Disk recovered strongly mid-flight
  (11,227,336 KiB at 00:00:36); the supervisor disk gate never fired. No disk bite.
- Recovery-5 polls: 25 polls, 21:31:00–23:54:17 AEDT. Range 2,198,032–10,006,060 KiB.
  Disk behaviour: flat ~5.5M KiB for polls 1–15, then violent multi-gigabyte swings
  — +440,260 KiB (poll 16), −764,240 (poll 18), +530,640 (poll 19), −1,202,308
  (poll 20), −2,222,424 (poll 21, chain low 2,198,032 KiB), +2,631,204 (poll 22),
  +3,547,936 (poll 23), +1,163,776 (poll 24), +465,112 (poll 25, gate opened).
  Two interruptions, honestly disclosed: (1) runtime restart killed the poll-17
  sleep; poll 17 ran directly at 22:53:22 (interval poll 16→17 ~4m01s instead of
  5m), no state lost; (2) ~19 min macssh preflight blindness 23:24–23:43 (five
  consecutive PREFLIGHT_FAILs, egress proxy could not reach the Mac — route-scope
  failure, not a Mac-down signal; route recovered 23:43:05, poll 23 ran 23:43:27,
  gap poll 22→23 ~24 min). Gate-open history across all six lanes
  (28 + 35 + 33 + 35 + 35 + 25 = 191 polls): the gate opened for the FIRST time in
  this lane, at poll 25.
- Preflight: supervisor re-verified all six input SHAs at launch (kernel
  `70d3859d`, dtb `73d1904b`, boot_image/CPIO `a9d6040a`, system_img `8e9914e3`,
  vendor_img `5e0607ad` — all MATCH, recorded in flight-end.json and
  preflight.json). The CPIO whole-file SHA `a9d6040a` was re-verified, which
  deterministically fixes the embedded vendor member at `5e0607ad` (Step 1
  verified); no separate member re-hash was run because the gate was sliding
  shut (df fell ~176 MiB in ~2 min before launch) and the whole-file hash is
  the airtight equivalent. QEMU command line read: system image as virtio-blk
  only, no vendor drive — the guest consumes the CPIO-embedded vendor, as designed.

## Leg 1 — registration: PASS (re-proven in this flight)

- `T727_PROBE_OK` x2: L10623, guest 101.885 s (on-boot action; flood-window but
  survived, sufficient proof) and L21890, guest 509.652 s (property action,
  POST-FLOOD, probative). The trigger file registered in the same flight.

## Leg 2 — service start: STARTED, payload FAILED

- `T727_SERVICE_OK`: 0 lines. The service's own marker never appeared.
- `init: starting service 't727probe'`: 0 lines. The analyzer pattern
  (`starting service 't727probe'`) and a direct grep both return 0. BUT this is
  a FALSE NEGATIVE as a start detector in this flight: 31 `starting service`
  lines exist for other services, including bootanim (guest 541.242 s) and apexd
  (guest 602.472 s) — init logs starting-service lines throughout the flight,
  including AFTER our service's window — yet none for t727probe, while the exit
  line proves it ran. The starting line's absence is therefore an unexplained
  logging gap for this service, NOT proof of non-start. The exit line is the
  authoritative start proof. (Lesson recorded: judge "started" from the exit
  line, not the starting line.)
- POSITIVE proof the service started and ran: L21969, guest 511.560 s —
  `init: Service 't727probe' (pid 3992) exited with status 127 oneshot service
  took 2.466000 seconds in background`. L21970–L21971: init sent SIGKILL to the
  service process group; libprocessgroup killed cgroup uid 0 pid 3992 in 0 ms.
  Start time implied: ~509.09 s (511.560 − 2.466), right at the property action
  (marker write 509.652 s) — the action's `start` then `write` order matches.
- `status 127` is UNIQUE to t727probe in the whole UART (only L21969). Other
  services exit 0 (apexd-bootstrap, boringssl_self_test64, fsverity_init,
  art_boot, bpfloader). The 127 is specific to our payload.
- No observable stderr: a service's stdout/stderr go to /dev/null by default
  (no `console`); no toybox/sh error lines from pid 3992 exist in the UART, and
  the analyzer's exec/start-failure patterns return 0. The payload died silently.
- Exit-127 mechanism UNPROVEN from this UART. Prime suspect: init's argv handoff
  of the quoted `sh -c "..."` string — if init passes the double quotes through
  literally, `sh -c` receives `"echo T727_SERVICE_OK > /dev/kmsg"` as one
  double-quoted word, i.e. a command literally named
  `echo T727_SERVICE_OK > /dev/kmsg`, which is not found → 127. The 2.466 s
  runtime is NOT explained by an instant not-found and remains an open question
  (what the shell did for 2.466 s is unobserved). Alternative suspects: toybox sh
  failing on the `/dev/kmsg` redirect; a slow/blocked open of /dev/kmsg. None is
  proven. The successor must make the payload observable (see below) instead of
  guessing.

## Holds (all intact unless noted)

- `T720_PROBE DONE status=PASS`: 1 (L223). `T720_PROBE DONE status=FAIL`: 0.
- T721 fence-log lines: 1 (L23423). Invalid present fence: 0.
- T628 REJECT allocate2: 0. CALL allocate2: 12 (first L11086, guest 109.328 s).
- EPERM: 0. `simplefb: mapped`: 1 (L23422, `0x5f000000 1280x720`).
- Watchdog KILLING: 0. Kernel panic / Oops / `BUG:`: 0.
- `sys.boot_completed`: 9 (see trigger-fired-first check above).
- RenderEngine dup-fail: 4, rate 4 / 604.9 s × 1000 = 6.612 per 1000 s
  (baselines: T721 flight-01 4.058, T721 revert 6.660) — matches the revert
  baseline; the T722 EGL-stub-ineffective finding stands.
- FATAL EXCEPTION: 6 (system_server self-dump cascade, same class as T724/T726).
- `avc:  denied` (two spaces): 2090; for /dev/mem: 6; sensorservice: 0.
- `T723_PROBE`: 0 (not flown in this lane — expected, context only). SP1/SP2/SP3
  DONE: 0 (expected). DROP_FAIL / SP1 RESOLVE_FAIL / SP2 GETPROCADDRESS_NULL: 0.
- Context: `[T723] boot_completed never detected in 600s; probe not run` (L2013) —
  the early-ramdisk hook again confirms the ramdisk-side path is dead; the
  second-stage path is the live one. `/t501-keystore2-dump.sh[15]:
  /system/bin/toybox: inaccessible or not found` (L594–L609, early-ramdisk phase)
  is the known T726 variance V2 context, not our service.
- Driver loaded: 5. META-EGL banner: 1. D/libEGL: 5.

## Wall

WALL-TYPE-TAG: guest.
New wall SERVICE-PAYLOAD-EXIT-127 (guest): a service started by init from a
property action runs, and its payload exits 127 without writing its marker or
any observable error. SERVICE-START-SILENTLY-FAILED (T726) is REFUTED as a
start-leg fault — init DOES start the service from a property action — and
RE-SCOPED: the silent failure moved one level deeper, from the start leg to the
payload exec. The EGL dispatch question (R1/R3/R4, open since T722) stays open
until a probe payload actually produces a marker.
Rust par bhi lagu: haan — init/service-start and payload-exec behaviour is
guest-side (engine-independent, same guest inputs); the Rust emulator must
reproduce the same init service semantics for the same kernel, DTB, system.img
and UART marker gate.

## parser verdict (G726-1 lesson applied)

parser: VERDICT ACCEPT, 0 errors — parse_init_rc.py on the exact t727probe.rc
bytes (648 B, 17 lines, SHA `eda66cfe`), Step 2 of the first lane. T726 control
also ACCEPT. The parser verdict is cited here explicitly in the findings text,
as the G726-1 gate requires.

## Reflection (REFLECT-ON-EVERY-PROBLEM)

- What happened (evidence): the gate opened for the first time in the chain at
  recovery-5 poll 25; the full 600 s service flight ran clean (QEMU exit 0);
  the trigger fired and the action marker fired twice; the service started
  (pid 3992, 2.466 s) but its payload exited 127 with no marker and no
  observable error. Verdict FAIL on the marker criterion.
- Root cause: the service payload exec fails at exit 127; the mechanism is
  unproven from this UART (prime suspect: init's argv handoff of the quoted
  `sh -c` string). The lane's design assumed marker-absence = service-never-
  started, and the verdict contract's FAIL shorthand said the same — but the
  flight proved the service DID start. The contract did not cover this middle
  outcome, so the findings state it explicitly instead of forcing the evidence
  into the shorthand.
- Missed signal: the T725 toybox-service shape was never trigger-tested (its
  boot_completed never fired), so the `sh -c` payload exec was unproven when
  T727 built on it — the same class of assumption as T726's probe-path
  assumption (a host-side presence check treated as a guest-context proof).
  Separately, the "starting service" line was treated as a required positive
  check for "started", but this flight shows a service can run with no such
  line — the exit line is the authoritative start proof.
- What changes: (1) adjudication judges "started" from the service exit line,
  not the starting line; (2) any future service payload must be observable —
  its argv, exit code and stderr must land somewhere readable (file under
  /data, or `console`, or a static helper binary that logs to a file) — a
  payload whose only output is a kmsg marker cannot be diagnosed when the
  marker is absent; (3) the `sh -c` quoted form must be proven at runtime
  (argv dump) before it is trusted as a payload shape.
- Where it lives: this findings file; the T728 successor brief carries the
  observable-payload requirement and the argv-dump experiment.

## Successor direction

- Next program (T728): observable-payload service. First, prove the argv
  handoff: a service that dumps its received argv (and the exit code path) to
  a file under /data, so the 127 mechanism is identified instead of guessed.
  If the quotes are the fault, replace the `sh -c "..."` form with a direct
  exec (single-command service, or a static helper binary that writes the
  marker itself). Then re-fly the T723 probe: with a working service payload,
  the probe-path question (a) is answered next.
- If the payload still exits 127 with observable argv, the fault is the argv
  handoff itself — fix the .rc quoting/exec form before any probe re-flight.
- NOT in this lane: GateKeeper, frame commit, EGL dispatch (still open until a
  probe payload produces a marker).

## Evidence inventory

- UART: `flights/service-flight/uart.log`, 27,482 lines, 3,208,505 B, SHA
  `8b1dbb2e0839f910630197eaa6a76270354ef054166bcd09ae7e820a98dd8aee`
  (matches flight-end.json).
- flight-end.json: stop "Guest target reached (604.9s >= 600s)", guest 604.944 s,
  host 606.19 s, QEMU exit 0; inputs all SHA-matched (kernel `70d3859d`, dtb
  `73d1904b`, boot_image `a9d6040a`, system_img `8e9914e3`, vendor_img `5e0607ad`).
- preflight.json (QEMU 11.1.2, command line read: system image virtio-blk only,
  no vendor drive); qemu-command.txt; qemu-stderr.log (80 B).
- analysis-t727.txt: SHA `dc71204d966bf08f4d6183259876645118241f54e5b0b45eede14abe16be5462`,
  123 lines (analyzer run on the flight UART; all counts in this file are from it
  plus direct greps cited with line numbers).
- supervisor_service-flight.log (supervisor PID 3923, QEMU PID 3927).
- progress-t727.txt, TIMELINE.md, heartbeat.json (CLOSED at lane end).
- Trigger file: `scripts/t727probe.rc`, 648 B, SHA `eda66cfe`; image
  `guest-inputs/t727-system.img`, SHA `8e9914e3`; patch `guest-inputs/t727.patch`,
  SHA `cfe7d1ee`. No commit made by this lane (driver-owned).
