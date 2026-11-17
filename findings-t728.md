# Findings — T728 observable-payload service

## Verdict: FAIL

`T728_SERVICE_OK` never appeared in the flight UART. The direct-exec service
never started: init refused it on SELinux labeling, proven by the quoted init
error line below. The quoting hypothesis (T727 prime suspect) was never tested
— the direct exec never ran.

## Flight

- service-flight, launched 08:34:20 AEDT 18 Nov 2026 (supervisor PID 32874,
  QEMU pid 32884), ended 08:44:21 AEDT. Guest 600.263 s, host 601.02 s, QEMU
  exit 0. Stop reason: guest target reached (600.3 s >= 600 s).
- UART: `flights/service-flight/uart.log`, 23,946 lines, 2,766,460 B,
  SHA `44fdd2ad2a37fcad8f554de4c829b3f900136b981ef673e94b09266cb6cfb284`
  (re-verified after flight-end; matches flight-end.json).
- Gate opened at poll 77 (08:31:40 AEDT, df 9,871,284 KiB — disk jumped
  +2,027,132 KiB in one 5-min interval); launch df 9,799,544 KiB.
- Preflight inputs all MATCH (kernel `70d3859d`, DTB `73d1904b`, CPIO
  `a9d6040a`, system `9b8ef7ff`, vendor `5e0607ad`).
- Analyzer: `scripts/analyze_t728_probe.py` SHA `4e5df2a9`.

## Adjudication (in order)

1. Trigger fired: YES. `sys.boot_completed=1` — 7 lines; first action
   processing line guest 563.938 s (`/system/etc/init/hw/init.rc:1224`);
   `t728probe.rc:12` action processed guest 569.472 s (UART L22097).
2. Registration marker (leg 1): YES. `T728_PROBE_OK` x2 — guest 109.049 s
   (on-boot leg, L10541) and guest 569.563 s (post-boot_completed leg, L22104).
3. Service start: NO. No service exit line, no pid, no "starting service"
   line. init's `start t728probe` command FAILED at guest 569.484 s (11 ms).
4. Argv dump: NONE. `T728_ARGV` 0, `T728_ARGC` 0 — the payload never ran, so
   no argv was ever handed off.
5. `T728_SERVICE_OK`: 0 — absent.

## Mechanism (proven, quoted verbatim from UART L22099)

```
[  569.483734] init: Command 'start t728probe' action=sys.boot_completed=1 (/system/etc/init/t728probe.rc:13) took 11ms and failed: Could not start service: File /system/bin/t728probe(labeled "u:object_r:unlabeled:s0") has incorrect label or no domain transition from u:r:init:s0 to another SELinux domain defined. Have you configured your service correctly? https://source.android.com/security/selinux/device-policy#label_new_services_and_address_denials. Note: this error shows up even in permissive mode in order to make auditing denials possible.
```

The binary was added via debugfs with no SELinux context, so init refuses it
even in permissive mode. This is a third outcome, distinct from T727 (service
started, exit 127) and T726 (service never started, silently, no explicit
error cited): here init EXPLICITLY refused with a clear error.

## New wall

SERVICE-SELINUX-LABEL-REFUSED — WALL-TYPE-TAG: guest (type 1).
"Rust par bhi lagu: haan" — the Rust emulator runs the same guest init and
SELinux policy; an unlabeled binary will be refused the same way. Fix
direction: label `/system/bin/t728probe` with a proper SELinux context
(file_contexts entry / domain transition, or reuse a labeled path) and re-fly
the T728 direct-exec probe. If the labeled direct exec marks, quotes were the
fault; if it exits 127, the fault is deeper than quoting.

## Holds / controls

- `T720_PROBE DONE status=PASS` 1, FAIL 0 (fence source live). T721 fence-log
  lines 0 (hold variance — the T720 DONE PASS line is the fence proof).
- simplefb: `simple-framebuffer 5f000000.framebuffer: fb0: simplefb
  registered!` 1 at guest 0.346 s. Analyzer "simplefb: mapped" pattern 0 —
  pattern wording, guest evidence present.
- `T628 REJECT allocate2` 0, `CALL allocate2` 12; EPERM 0; Invalid present
  fence 0; Watchdog KILLING 0; FATAL EXCEPTION 0; Kernel panic/Oops/BUG 0.
- RenderEngine EGL dup-fail 0 (0.000 per 1000 s over 600.3 s; baselines T721
  flight-01 4.058, T721 revert 6.660). EGL dispatch R1/R3/R4 stays open: no
  probe payload marker was produced.
- `avc:  denied` 1898 total; `/dev/mem` 6; sensorservice 6. `sw_sync` 57 lines.
- T727 controls 0 (`T727_SERVICE_OK` 0, `T727_PROBE_OK` 0, `t727probe` 0 —
  old rc replaced). `T723_PROBE` 0.
- `sys.boot_completed` 7 lines; `boot_completed` 8 lines (incl. the stale T723
  early-ramdisk hook line L2008, context only).

## Parser citation

parser: VERDICT ACCEPT, 0 errors — `parse_init_rc.py` (SHA `d42b5b6e`) on the
exact `t728probe.rc` bytes (Step 2, driver-verified 01:45 AEDT 18 Nov; rc SHA
`f61b9c0f`).

## Steps 0–3 (carried over, driver-verified)

- Step 1 PASS, Step 2 PASS (helper `19db5d31`, static aarch64; parser ACCEPT
  0 errors; markers unique), Step 3 PASS (image `9b8ef7ff`, patch `7ecbe959`,
  supervisor pin filled; analyzer sanity reproduced driver counts exactly on
  the T727 UART).
- Step 4: 77 df readings across three lanes (32 + 34 + 11); gate opened once,
  at poll 77; flight launched the same turn.

## Successor

Label the payload binary and re-fly the T728 direct-exec probe (same image
shape, same trigger, same gates). The T727 exit-127 question is still open
until a labeled direct exec runs.
