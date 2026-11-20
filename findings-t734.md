# Findings — Track T734 (kernel-init exec discrimination)

**Verdict: PASS**

## Summary

The kernel-init exec runs the minimal no-libc binary. `T734_KERNEL_OK` appeared in
the full flight (service-flight-9) at guest 7.80 s, early, before Android init
took over. The discrimination is answered: the fault is in **Android init's exec
dispatch**, not in the binary and not in the kernel-init exec path. This
confirms the T733 guest wall INIT-EXEC-DISPATCH-SILENTLY-FAILED.

The first four flights of this track (service-flight-4/5/6/7) all kernel-panicked
on CPIO unpack — not because of the content changes, but because of a **harness
wall**: the DTB hardcodes `linux,initrd-end` to the ORIGINAL CPIO's size, and
the supervisor loads the CPIO via QEMU's `loader` device (not `-initrd`, so
QEMU does not update the DTB). Every variant CPIO is larger than the original,
so the kernel was handed a truncated initrd and its gunzip ran out of input
("read error"). Patching the DTB's `linux,initrd-end` to match the flown file
fixed it: flight-8 (validation) and flight-9 (full) both booted cleanly.

## Step 5 adjudication (in order)

**(1) Did the kernel-init exec run the minimal binary? YES.**
`T734_KERNEL_OK` appeared in service-flight-9, early, guest 7.80 s, before
Android init. Verbatim UART line:

```
L195: [    7.802587] T734_KERNEL_OK
```

The probe's own completion line, same flight:

```
L197: [T734] kernel-init probe finished rc=0
```

The validation flight (service-flight-8) showed the same marker independently:

```
L194: [    4.091944] T734_KERNEL_OK
```

**(2) Did the trigger fire? YES.** `sys.boot_completed` first fired at guest
790.04 s. Verbatim:

```
L22271: [  790.041275] init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1224)
```

`sys.boot_completed` count in flight-9: 7 (L22271, 22294, 22428, 22452, 22796,
22820, 22846). The trigger fired later than in T733 (628.63 s) — the T734
guest boots slower throughout (leg-1 at 196 s vs 117 s in T733) — but it
fired, and the property leg ran after it.

**(3) Did the registration markers fire? YES, both legs, plus the control.**
- Leg 1 (on-boot write): `T733_PROBE_OK` L10643, guest 196.185 s; control
  `T728_PROBE_OK` L10640, guest 196.109 s.
- Leg 2 (property-action write, after the trigger): `T733_PROBE_OK` L22666,
  guest 797.691 s; control `T728_PROBE_OK` L22665, guest 797.672 s.
- Control `T728_PROBE_OK` count: 2 (legs 1+2, as in T732). The control is
  present, so the verdict is not void.

**(4) What did the Android-init minimal binary's marker say?**
`T733_MINIMAL_OK`: 0 in flight-9. As expected — it reproduces T733 exactly:
the same minimal no-libc binary stays silent when Android init execs it, but
runs when the kernel's init execs it. Nothing here overturns T733.

**(5) Did `T734_KERNEL_OK` appear? YES.** 1 line, L195, guest 7.80 s
(flight-9); independently 1 line, L194, guest 4.09 s (flight-8).

**(6) Did init log any SELinux refusal for either binary?** No refusal for
`t734probe` or `t733probe` / `t728probe`. The single init service-refusal line
in flight-9 is unrelated stock service noise:

```
L10148: [  176.995973] init: Could not start service 'vendor.camera-provider-ranchu' as part of class 'hal': File /system/bin/sh(labeled "u:object_r:shell_exec:s0") has incorrect label or no domain transition from u:r:init:s0 to another SELinux domain defined. ...
```

**(7) Did the guest boot normally after the probe exec? YES.** Android init
took over after the probe: `init: init first stage started!` (guest 10.7 s),
`/system` mounted, root switched, the trigger fired at 790.04 s, and the
property leg ran. No boot stall after the probe exec. The guest ran the full
850 s target (guest 855.994 s) and the flight ended cleanly (QEMU exit 0).

Note: the UART tail carries `/early-ramdisk[256]: can't create /dev/null: No
such file or directory` lines — the modified early-ramdisk script has a
looping/background artifact (it keeps running after the probe). It does not
affect the verdict; it is recorded for the successor to clean up.

## Evidence per flight

| Flight | Boot image (SHA) | DTB (SHA) | Result | Guest | Host | UART (lines, SHA) |
|---|---|---|---|---|---|---|
| service-flight-4 | v1 full `03668437` | unpatched `73d1904b` | KERNEL PANIC (unpack) | 7.87 s | 15.03 s | 214, `a909d2b9` |
| service-flight-5 | v2 full `57c04949` | unpatched `73d1904b` | KERNEL PANIC (unpack) | 6.48 s | 15.02 s | 215, `95a05745` |
| service-flight-6 | probe-only `530adf05` | unpatched `73d1904b` | KERNEL PANIC (unpack) | 7.152 s | 15.06 s | 215, `ba20c09c` |
| service-flight-7 | init-only `ae2b5a18` | unpatched `73d1904b` | KERNEL PANIC (unpack) | 5.936 s | 15.02 s | 215, `9db877cd` |
| service-flight-8 | v2 full `57c04949` | patched `72831bab` | BOOT OK (validation) | 124.671 s | 125.48 s | 8340, `ce664283` |
| service-flight-9 | v2 full `57c04949` | patched `72831bab` | BOOT OK (full) | 855.994 s | 856.93 s | 25539, `33c9b12a` |

Flown stack (all flights): kernel `70d3859d`, system image `af40403a`,
vendor `5e0607ad`, patch `4d74f4d1`, rc `79ad3cc3` (parser citation: VERDICT
ACCEPT), T733 binary `fd189c08`, T728 binary `19db5d31`, t734probe binary
`b7440e1c`. QEMU `8e5c46dc`.

### Flights 4–7: the panic (identical in all four)

```
L108: [    0.397962] Trying to unpack rootfs image as initramfs...   (flight-8 numbering; same line in 4–7)
L109 (flight-4): [    4.313332] rootfs image is not initramfs (read error); looks like an initrd
L188 (flight-4): [    7.856694] RAMDISK: incomplete write (23028 != 32768)
L196 (flight-4): [    7.866566] Kernel panic - not syncing: VFS: Unable to mount root fs on unknown-block(0,0)
```

All four flights: T734_KERNEL_OK 0, T733_PROBE_OK 0, T728_PROBE_OK 0,
T733_MINIMAL_OK 0, FATAL EXCEPTION 0, Kernel panic/Oops/BUG 1. The panic is
the unpack failure, not a verdict on the experiment — the guest never reached
the probe.

### Flight-8 (validation, 120 s): the DTB fix confirmed

Unpack succeeded (no "read error", no panic):

```
L108: [    0.397962] Trying to unpack rootfs image as initramfs...
L109: [    3.690422] Freeing initrd memory: 52140K
L194: [    4.091944] T734_KERNEL_OK
L196: [T734] kernel-init probe finished rc=0
```

Android init started (first stage 10.73 s, /system mounted 11.17 s). Control
markers were 0 at 124.7 s — the short window ended while the guest was still
in early boot (apexd processing APEX); the control is properly tested in the
full flight-9. T721 fence-log / simplefb / sys.boot_completed holds are 0 in
this short window for the same reason; they are not failures.

### Flight-9 (full, 850 s): the terminal flight

Unpack succeeded:

```
L108: [    0.653995] Trying to unpack rootfs image as initramfs...
L109: [    6.762516] Freeing initrd memory: 52140K
```

Marker counts (from `analysis-t734.txt`, 183 lines): T734_KERNEL_OK 1,
T733_PROBE_OK 2, T728_PROBE_OK 2, T733_MINIMAL_OK 0, T728_SERVICE_OK 0,
sys.boot_completed 7, FATAL EXCEPTION 0, Kernel panic/Oops/BUG 0,
avc: denied (two spaces) 2218, /dev/mem 6, sensorservice 4, RenderEngine
dup-fail 4 (rate 4.673/1000 s; baselines T721 flight-01 4.058, revert 6.660).

## Root cause of flights 4–7: DTB initrd-end cap (harness wall)

The DTB `t723-modified.dtb` hardcodes:

```
/chosen/linux,initrd-start = 0x110000000
/chosen/linux,initrd-end   = 0x1132eaef0  (= 0x110000000 + 53391088, the ORIGINAL CPIO size)
```

The supervisor loads the CPIO with QEMU `-device loader,file=<CPIO>,
addr=0x110000000,force-raw=on` — NOT `-initrd`, so QEMU does **not** update
the DTB's initrd-end. The kernel therefore reads exactly 53,391,088 bytes as
the initrd, whatever file was loaded. Every variant CPIO is larger than the
original (v1 53,745,734; v2 53,391,901; probe-only 53,391,524; init-only
53,391,468), so the kernel's gunzip consumed all 53,391,088 bytes without
reaching the end of the deflate stream and reported "read error" — the exact
string `lib/decompress_inflate.c` (`__gunzip`) emits when its `fill` callback
returns < 0 because the input ran out (`init/initramfs.c` returns that message
via `unpack_to_rootfs`, and `populate_rootfs` prints "rootfs image is not
initramfs (%s); looks like an initrd").

Proof, all independently verified:
- The gzip streams are valid (correct headers, ISIZE, CRC; Python decompresses
  each fully) and the newc structure is valid (strict independent walk: correct
  magic, hex fields, NUL-terminated names, zero padding, trailer last, no
  trailing bytes; the rebuilder round-trips the original byte-identically,
  SHA `3ddd4c75`).
- The entry diffs are minimal and clean (probe-only adds one entry; init-only
  changes only early-ramdisk's size; the trailer is identical in all).
- All four variants are larger than the original, and all four fail with the
  identical "read error" at ~3.3–4.3 s (near the end of decompression) — the
  signature of running out of input, not of a corrupt entry.
- Patching only `linux,initrd-end` (to 0x110000000 + the flown file's size)
  fixed it with no other change: flights 8 and 9 boot cleanly.

**WALL: DTB-INITRD-END-CAP (harness, NEW).** Type: harness (host-side
configuration — the DTB describes a stale initrd size while the loader serves
a different-sized file). It is not a guest wall.
Rust par bhi lagu: nahi (the DTB/`loader` mechanism is QEMU-specific; the
Rust emulator has no DTB initrd-end to go stale) — but the lesson carries:
any emulator that loads an initrd must pass the exact size of the file it
actually loaded, never a cached/stale size.

The T734 discrimination result confirms the T733 guest wall:
**WALL: INIT-EXEC-DISPATCH-SILENTLY-FAILED (guest, from T733).** The same
minimal binary runs under the kernel-init exec and stays silent under Android
init's exec. Rust par bhi lagu: haan — the Rust emulator must reproduce
Android init's exec dispatch behaviour faithfully, including this silent
failure mode, or the differential oracle will diverge.

## The bisect is VOID

Flights 6 (probe-only) and 7 (init-only) both panicked identically — but both
were killed by the DTB size cap, not by their content. The bisect cannot
distinguish the two content changes while the cap is in place, and it does not
need to: with the cap fixed, the FULL v2 CPIO (both changes) boots and the
probe runs. There is no content fault to bisect. Do not re-run the bisect.

## Holds (flight-9)

- T720_PROBE DONE status=PASS >= 1: 1 (L226) — MET
- T721 fence-log >= 1: 1 (L22952) — MET
- Invalid present fence 0: 0 — MET
- REJECT allocate2 0: 0 — MET (CALL allocate2 13, context)
- EPERM 0: 0 — MET
- simplefb: mapped >= 1: 1 (L22951) — MET
- Watchdog KILLING 0: 0 — MET
- sys.boot_completed >= 1: 7 — MET
- Kernel panic/Oops/BUG 0: 0 — MET
- FATAL EXCEPTION 0: 0 — MET

All holds met. No secondary failure explains or weakens the verdict.

## Reflection

- **Root cause of the 4-flight stall:** the harness (DTB + loader) silently
  truncated the initrd at the original file's size. The failure looked like a
  content fault (the CPIO "wouldn't unpack") and the bisect was launched on
  that theory — but the bisect variants were doomed by the same cap, so the
  bisect could not have distinguished anything. The tell was that ALL variants
  failed identically regardless of content, and that the failure came at the
  END of decompression (a truncation signature), not at a changed entry.
- **What changes:** (1) the supervisor (or any future flight harness) must
  ensure the DTB's `linux,initrd-end` matches the flown CPIO's size — the
  patched-DTB + `T734_DTB`/`T734_DTB_SHA` env-override path used here is the
  working mechanism; (2) when every variant of a bisect fails identically,
  stop and check the harness inputs (sizes, addresses, DTB) before concluding
  the content is at fault — a bisect that cannot distinguish is void, not
  evidence; (3) the kernel source is the ground truth for a kernel error
  string — "read error" had a precise meaning (input exhausted) that pointed
  straight at the truncation once it was read.
- **Where it lives:** this findings file; the DTB patch script
  `patch_dtb_initrd.py` and the patched DTB `t734-modified.dtb`
  (SHA `72831bab`) in the worktree `guest-inputs/` and the box primary store;
  the supervisor's `T734_DTB`/`T734_DTB_SHA` overrides.

## Deliverables

- Findings: this file (`findings-t734.md`).
- Outcome: `outcome-t734.txt` (5 bytes, `PASS`).
- Death note: none — the track PASSED; a death note is written only on FAIL.
- Flights 4–9 complete in `flights/` (flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log, uart.log + uart.log.gz +
  uart.log.numbered.txt, analysis-t734.txt).
- `progress-t734.txt`, `TIMELINE.md`, `heartbeat.json` (CLOSED, PASS).
- Patched DTB `guest-inputs/t734-modified.dtb` (SHA `72831bab`) and the patch
  script `patch_dtb_initrd.py`, synced to the sandbox and the box.
