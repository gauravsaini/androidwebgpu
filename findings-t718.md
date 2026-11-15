# T718 findings — write_result reply path: the missing AIDL null flag

WALL-TYPE-TAG: guest
Rust par bhi lagu: haan

Verdict: FAIL

## 1. Mission and outcome summary

Mission (brief track718-write-result-reply.md, SHA 039149ba): diagnose
why the T628 AIDL allocator shim's `write_result` reply path fails
`allocate2` with -129 (EX_TRANSACTION_FAILED) in T717 smoke #4, apply
the smallest fix yielding a nonzero allocation, then fly for nonzero
allocation + queueBuffer > 0 + first present dispatch. Base: the T717
stack (v4 parser binary bf8661d0, vendor 5d2d82b0, CPIO fceef5df, DTB
1a961795, system.img 1283cbf2, Image 53d9563e). Constraints honoured:
v4 parser bytes untouched, SYS_RAWIO fix not regressed, installd
trigger untouched, no agent process on the Mac (sandbox subagent
driving via macssh only), no macOS debugfs image patching, no commits.

Outcome: the -129 wall is BROKEN. Framework-first disassembly proved
`write_result` omits the AIDL non-null int32 flag the framework's Bp
reply reader requires before `AllocationResult::readFromParcel`. A
17-instruction epilogue rewrite plus a 4-instruction flag trampoline
hosted in the dead `AParcel_readInt64` PLT stub fixed it. The smoke
PASSED (REJECT 0, CALL 2, GBA-fail 0, importBuffer x2). Flight-01
proves allocation works at scale: 604 `allocate2` calls, 695
`importBuffer`s, 0 REJECT, 0 GraphicBufferAllocator failures, 0
`allocateBuffers` failures, all T716 holds intact, Dexopt PERFORMED
for Launcher3, sys.boot_completed reported 11 times. But the display
criteria are NOT met: queueBuffer 0, presentDisplay 0, no present
dispatch. The exact first failing link is the present path: every
frame SurfaceFlinger's Scheduler tried to track carried an invalid
present fence (2241 identical errors, first L23250, last L323495).
The GateKeeper NPE parallel wall persists (1 system_server death,
all 6 FATALs DeadSystemException collateral). The precise next fix
is in section 8.

## 2. Step 0 and Step 1 — gates and inputs

Step 0 PASS (Mac clock 04:31:19 AEDT 16 Nov 2026): Mac data volume
95%, 12 Gi avail, above the 8 GiB gate. Re-checked before the smoke
(94%, 12 Gi) and by every supervisor launch (disk gate inside the
supervisor, 8 GiB floor).

Step 1 PASS: every input SHA re-hashed and matched, no network fetch.
T717 findings 2e2381cd (42,724 B); T717 death note b126e186 (4,158 B);
T717 smoke UARTs v1 c0d63737 / v2 0e048c1c / v3 6af2f8b9 / v4 4c877ebd;
T716 flight-01 b178e4c6; T717 vendor 5d2d82b0; T717 CPIO fceef5df;
T717 DTB 1a961795; T715 system.img 1283cbf2 (also re-verified on the
box vault copy); T705 Image 53d9563e; extracted_kernel.config
d840310a; staged composer ELF 8d62a642; original T628 binary 745d19d0
(x2, box); v4 T628 binary bf8661d0 (x2, box, full SHA
bf8661d0bbb67c996fbf8857cdfa88eb6fbc4175529480e7202ae5e969853a98).
The framework libraries were extracted fresh from the vault system
image via read-only debugfs on the box: allocator-V2-ndk.so
a941d842 (136,432 B, BuildID 3b4a319f68bd9cd626f180f7097a6f23,
byte-identical to the T716 staged copy), common-V2-ndk.so 088a79a8
(133,864 B), graphics-common-V4-ndk.so 276e876e (134,536 B).
findings-t717.md was read in full (709 lines); the T717 patch script,
build_patch.sh, and splice_patch.py were read and reused as patterns.

## 3. Step 2 diagnosis — framework disassembly FIRST

All disassembly below is from the guest's own libraries (box
aarch64-linux-gnu-objdump), not from convention or headers.

### 3.1 The bug, proven: write_result omits the null flag

The framework's server stub (Bn) for `allocate2` in
allocator-V2-ndk.so (0x1106c-0x111ec) reads the flag, descriptor, and
count, dispatches through vtable[56], writes the status header, checks
`AStatus_isOk`, then calls the helper at 0x13078. The helper is
`AParcel_writeParcelable<AllocationResult>`: it calls
`AParcel_writeInt32(parcel, 1)` and then tail-calls
`AllocationResult::writeToParcel`. The int32 value 1 is the AIDL
non-null flag for the parcelable return value.

The framework's client stub (Bp) at 0x117b4, called from
`BpAllocator::allocate2` (0x11a54), reads an int32 flag FIRST:
flag == 0 is an error (0x80000008); otherwise it calls
`AllocationResult::readFromParcel`. The Bp REQUIRES the flag.

The shim's `write_result` (0x11a58-0x11f3c) success path at 0x11eac
calls `AParcel_writeStatusHeader(AStatus_newOk)`, then at 0x11ee0
calls `AllocationResult::writeToParcel` directly — with NO flag write
between them. `on_transact` code 2 (0x116b0) returns write_result's
return value verbatim. The shim therefore emits a reply the Bp cannot
parse: the Bp reads the parcelable size field as the flag, then
misparses the rest of the reply.

### 3.2 What was refuted (do not re-investigate)

- AllocationResult layout `{int32 @ +0, vector<NativeHandle> @ +8}`,
  32 bytes: proven from `writeToParcel` (0x10118) and `readFromParcel`
  (0x10014). The shim's construction matches EXACTLY.
- NativeHandle layout `{vector<ScopedFileDescriptor> fds @ +0,
  vector<int32> ints @ +24}`, 48 bytes: proven from
  `NativeHandle::writeToParcel` (0x10604) and `readFromParcel`
  (0x104f8) in common-V2-ndk.so. The shim matches EXACTLY.
- Missing status header: REFUTED. write_result DOES call
  `AParcel_writeStatusHeader` with `AStatus_newOk` (0x11eac-0x11ec4).
- Bp transact mechanics (`BpAllocator::allocate2`, 0x11834): transact
  code 2; a nonzero transact status (except -74) goes to
  `AStatus_fromStatus`, so -129 is a transact-level failure; the reply
  reads run only when transact returns 0.

### 3.3 The -129 mechanism: recorded honestly

A hand byte-trace of the Bp misparse (flag read consumes the
parcelable size 44; readFromParcel reads field0 1280 as a size; the
NativeHandle size check fails) predicts error -22, but T717 smoke #4
observed -129. The flag omission is a proven protocol violation
regardless; whether the misparse yields -129 through parcel internals
the hand trace cannot simulate was decided empirically by the smoke,
not by further armchair reasoning. The smoke PASSED with the flag fix
and no other change (section 5), which establishes empirically that
the flag omission was the cause of the -129 failure. The exact
internal path from misparse to EX_TRANSACTION_FAILED remains
unmodelled by hand; the empirical link is what the evidence supports.

### 3.4 Patch design: the epilogue has no room, the PLT stub does

The write_result success epilogue (0x11eac-0x11eec) is 17 instruction
slots; a flag-inclusive rewrite needs 19. Dropping the header check
is reasoned SAFE (a failed header write implies writeToParcel fails
on the same parcel) but still leaves 18 vs 17. Masking writeToParcel's
error or leaking the AStatus is NOT acceptable. With the flag write
outlined via `bl` to a trampoline, the epilogue fits in exactly 17
slots. The trampoline needs 24 bytes as a save-LR sequence, but only
16 bytes as a tail call: `mov x0, x19; mov w1, #1; b
AParcel_writeInt32@plt; nop` — the `b` preserves LR, so writeInt32
returns directly to the epilogue.

The 11f24 dead-code hypothesis was REFUTED on re-examination (11f24
is live, reached via the temp-vector destroy loop; overwriting it
would have been an R2-class defect — see section 10). A NOP-run scan
found ZERO runs >= 16 bytes anywhere in the binary. A full-
disassembly PLT call-site probe found exactly one uncalled stub:
`AParcel_readInt64@plt` (0x34ab0, 16 bytes) — dead in the v4 binary
because only the original parser called readInt64. Adjacent stubs
(0x34aa0 getDataPosition, 0x34ac0 setDataPosition) are live. The
trampoline fits the dead stub exactly. The stub is REPURPOSED: any
future code calling AParcel_readInt64 must restore the original 16
stub bytes first (recorded in splice_t718.py). PLT0 (0x348b0) and
`__lcxx_override` (live `operator new`) are off limits. The v4 parser
footprint's 24-byte NOP pad stays byte-identical per the brief.

## 4. The fix and the ship — build, review, splice, images

### 4.1 Build and disassembly review

Two assembly sources (epilogue.s, trampoline.s), assembled with
`aarch64-linux-gnu-as`, linked at fixed vaddrs with `--defsym` PLT
targets (T717 build pattern), extracted with objcopy. Sizes asserted
exact: epilogue.bin 68 bytes, trampoline.bin 16 bytes. The
disassembly review verified instruction-by-instruction: all 5 `bl`
targets (newOk 0x349c0, writeStatusHeader 0x349d0, delete 0x349e0,
trampoline 0x34ab0, writeToParcel 0x34a90), both `cbz`/`cbnz`
targets (exactly 0x11ef0, the original destroy entry), and the
trampoline's tail-`b` (0x34a00). Register contract verified: the
epilogue touches only w21/x0/x1/x22/x19 (as the original did, plus
x1 as scratch, dead at epilogue entry); the trampoline touches only
x0/w1 (reloaded by the epilogue afterwards); x19/x25/w21/x29 are
preserved per the destroy/epilogue contract. (Review note: objdump
`-d` prints nothing for raw binaries — the review used `-D` and the
linked ELFs; the empty first dump was noticed, not assumed.)

### 4.2 Splice

splice_t718.py asserted the input SHA (bf8661d0, full), the input
size (1,711,816), the original 68 epilogue bytes, and the original
16 stub bytes before writing. Output: fixed binary
3daa5a491e9d04ff56bbb7998be1eb91a1b3ed625d66b48f264a04b04ca70932
(1,711,816 B), 73 bytes differ, ALL inside the two patch regions;
the v4 parser footprint [0xDF3C, 0xE0F0) is byte-identical to the v4
input. In-context disassembly of the spliced binary confirmed the
epilogue inside write_result, the trampoline in the stub, and both
neighbour stubs intact.

### 4.3 Images (box Linux debugfs; NEVER macOS debugfs)

Vendor patch (patch_vendor_t718.py, adapted from patch_vendor_t717.py;
base = t717-vendor.img 5d2d82b0, expected orig bin = bf8661d0):
rm-first-then-write, inode 731 reused, mode 0100755, uid 0, gid 2000,
xattr `u:object_r:hal_graphics_allocator_default_exec:s0`, e2fsck -f
-y code 0 then -f -n code 0, dumped binary byte-identical, all 8
neighbours byte-identical, composer ELF 8d62a642 + HIDL allocator
91d84fb9 byte-identical, RC1 47a9bd4a + RC2 468f2a85 prefixes with
SYS_RAWIO lines intact (RC1 one `capabilities SYS_NICE SYS_RAWIO`
line; RC2 one SYS_RAWIO hwc line + one plain SYS_NICE allocator
line), base image untouched. New vendor.img:
bac3cdc112dc6c3257c016fb4f600c46f43b9372f3d2f2ebb9b7d79fd05daeca
(134,217,728 B).

CPIO member replacement (cpio_replace_t718.py, T699/T716/T717
procedure): parsed the uncompressed newc stream (725,742,732 B, 237
real members + TRAILER!!!), located `ramdisk-images/vendor.img` at
data offset 2972756 (the offset T716 recorded), verified the old
member SHA (5d2d82b0), replaced the member bytes in place (same
length asserted), re-walked and proved by member-by-member comparison
that ONLY the vendor.img member differs, re-gzipped `gzip -9 -n`.
Shadow-copy check PASS: no member name contains t628. New CPIO:
1150c8410657fe40c99bc96c0f5e61af2dd0938087011076dc7015a85f127d9e
(53,379,397 B).

DTB re-align (dtb_patch_t718.py, minimal Python FDT parser; dtc is
not installed on the box): initrd-start 0x110000000 unchanged, old
initrd-end 0x1132e7eef verified equal to start + 53,378,799, new
initrd-end 0x1132e8145 = start + 53,379,397, 2 bytes differ, all
inside the 8-byte value, size unchanged 2,049 B. New DTB:
d298f5adc8caa8426eb4cfdc72f88bdeb070f90210db6bc1a10622303a2f53d9.
(One script bug found and fixed in-lane: the FDT walk keys the node
path `/chosen`, not `chosen` — the script's own assertion caught it
before any write.)

Pre-flight proof: all three artifact SHAs verified on the box, in
the sandbox, and on the Mac (guest-inputs/) before flight; the
supervisor re-verified every input SHA at each launch (preflight.json
in each flight dir). installd.rc is unchanged in the shipped image
(system.img 1283cbf2 untouched). The 8 T718 supervisor/verify/launch
scripts were py_compile + import-tested and runtime-verified against
fixtures before flight: verify_smoke reproduced the T717 smoke #4
signature exactly (REJECT 0, CALL 1, GBA 1, allocbuf 1); the analyzer
reproduced the T716 flight-01 counts exactly (REJECT 71, GBA 71,
allocbuf 42, EPERM 0, mapped 7, kills 0, boot_completed 29);
verify_flight_ab reproduced the T716 control pattern exactly (576
lines, 20.36 s). Fixture UART copies were removed after testing.

## 5. Step 3a smoke — PASS (240 s guest target, 300 s host cap)

Launched 18:00:49Z (supervisor PID 4268, QEMU PID 4272). Ended
18:05:49Z on the host cap (guest kernel time 191.7 s — the
supervisor's last-1MB kernel-timestamp scan plateaus while the guest
is in logcat-only userspace; the UART grew throughout and QEMU ran
at 100% CPU, so the guest was alive, not wedged). QEMU exit 0.
13,816 lines, 1,469,076 B, UART SHA
137fb562e364f1d4bab62da849ef6f4cfd7e411aff0109b0184da879160d230d.

- REJECT method=allocate2: 0.
- CALL method=allocate2 count=1 width=1280 height=720: 2 (L10444,
  L10451, guest ~102 s).
- GraphicBufferAllocator failures: 0. allocateBuffers failures: 0.
- T637_MAPPER CALL method=importBuffer: 2 (L10446, L10453) — each
  CALL was followed by the mapper importing the buffer, proving the
  Bp received a valid AllocationResult. This is the nonzero-
  allocation evidence.
- Watchdog kills 0, system_server starts 1, EPERM 0, FATAL EXCEPTION
  0, installd-not-found 0, sensorservice avc denials 0, REGISTERED 1
  (no shim restart), getDisplayAttribute failed 1 (tolerated, T717
  Step 2), DisplayDeviceInfo 1280x720 present (L11962), queueBuffer
  0 / presentDisplay 0 (expected in the smoke window; adjudicated
  from flight-01), boot_completed 0 (expected; boot completes at
  guest ~505 s).

Smoke criterion met: REJECT 0 AND GBA-fail 0 AND >=1 nonzero
allocation. The -129 wall is BROKEN. Proceeded to Step 3b.

## 6. Step 3b flight-01 — allocation works, present does not

Launched 18:08:30Z (supervisor PID 4668, QEMU PID 4672). Ended
18:48:34Z on the guest target (2403.8 s >= 2400 s, host 2404.42 s).
QEMU exit 0. 323,607 lines, 42,885,318 B, UART SHA
081a9381aba9de5d6e946fd7df11e231585acf1e098a97673dcef5be0aed03db.
Structural holds: Freeing unused kernel memory, init first stage,
fb0 simplefb registered, /dev/block/vda mounted as /system Success;
no hang, no Oops/panic (0), no RAMDISK incomplete write (0), subHal
load failures 0, prime-cache errors 0.

Allocator evidence (the T718 fix works at scale):

- T628 REJECT allocate2: 0. GraphicBufferAllocator failures: 0.
  allocateBuffers failures: 0.
- CALL method=allocate2: 604. T637_MAPPER importBuffer: 695.
  freeBuffer: 218. getIMapperLibrarySuffix CALLs: 103. T637_MAPPER
  lines: 1164. REGISTERED: 1 (shim alive all flight, no restart).
- The guest consumed the patched shim (GUEST-CONSUMES-ARTIFACT):
  604 parsed calls with correct values, 695 imports, zero failures.

Holds intact (T664-T717 fixes not regressed):

- EPERM (simplefb open /dev/mem failed): 0. simplefb mapped: 1.
- Watchdog kills: 0. SensorService onBootPhase RETURN: 2.
- installd not found: 0. installd STARTED: 1.
- Dexopt result for com.android.launcher3: status=PERFORMED (L13479);
  Launcher3 dexopt SKIPPED: 0.
- getDisplayAttribute failed: 1 (tolerated; display formed 1280x720,
  DisplayDeviceInfo 4 lines, state ON twice, L19043/L32328).
- system_server: 2 starts, 2 entered (2 lifetimes; see GateKeeper
  below). sys.boot_completed: 11 lines (boot_completed lines: 11,
  sys.boot_completed lines: 11 — every boot_completed line carries
  the sys.boot_completed literal). The system_server fork state is:
  forked twice, second lifetime survived to the end of the flight.

Display criteria NOT met (the FAIL):

- queueBuffer: 0. presentDisplay: 0. Present-dispatch lines: 0.
- `E/Scheduler(2488): trackPendingFrame: Invalid present fence`:
  2241 lines, first L23250 (23:24:45), last L323495 (23:56:29),
  spanning the entire flight — every frame the Scheduler tried to
  track carried an invalid present fence. onFrameAvailable 0,
  validateDisplay 0, presentOrValidate 0, doComposition 0 (verbose
  SF/HWC logs absent at this log level; their absence alone proves
  nothing — the fence errors are the observed symptom).
- Composer service received 704 command batches from the driver and
  logged zero E-level lines; SurfaceFlinger logged zero E-level
  lines except the deprecated-IDisplayService notice. The composer
  path is live; frames die at the Scheduler's fence check before
  any present dispatch is observable in the UART.
- FallbackHome was Displayed twice (L19293 lifetime 1, L32645
  lifetime 2) and the display reached state ON twice, but no
  guest-origin frame commit (SurfaceFlinger present success /
  composer dispatch / guest fb0 writes) is evidenced in the UART,
  and snapshot byte differences are never proof (T673). No 10/10
  claim is made (TEN-PASSES not run; one flight + control only).

GateKeeper NPE parallel wall (persists, separate named lane):

- L23129: `java.lang.NullPointerException: Attempt to invoke
  interface method 'void android.service.gatekeeper.
  IGateKeeperService.reportDeviceSetupComplete()' on a null object
  reference` (lifetime 1). LockSettingsService "Unable to acquire
  GateKeeperService" 4x (both lifetimes). Crash-file records name
  `java.lang.NullPointerException` in system_server (L39567,
  L50339). Lifetime 1 (PID 2577) died (signal 9, L24634); lifetime 2
  (PID 4021) started L25930 and survived.
- FATAL EXCEPTION: 6, ALL DeadSystemException collateral of the
  lifetime-1 death at 23:25:06-07 (systemui x2, acore,
  permissioncontroller, launcher3 x1 PID 3499, documentsui).
  removeRegionSamplingListener: 0. The single Launcher3 FATAL is
  DeadSystemException collateral, not a composer-path FATAL.
- Launcher3 restarted repeatedly after the death (PIDs 3499, 4512,
  5154, 5584, each dying as TOP); the L58683 debuggerd dump of PID
  5154 shows a PARKED worker thread (art::Thread::Park), not a
  fatal signal — the Launcher3 churn is guest userspace behaviour,
  not the display wall (T673/T701 standing note).

## 7. A/B control — flight-ab (canonical kernel)

Launched 18:49:56Z (supervisor PID 6353, QEMU PID 6356) with
canonical-Image 05c43917. Ended 18:50:21Z: QEMU self-exited 0 after
25.04 s host, guest 20.19 s. 576 lines, 48,005 B, UART SHA
1a9a58fa6389379efcaff2e6752161eff8ffcef9efdf5824715bfb82444fbec0.
system_server starts 0, installd 0, queueBuffer 0, presentDisplay 0,
EPERM 0, mapped 0, T628 0, REJECT 0, GBA 0, allocbuf 0,
boot_completed 0. The pattern matches the T716 control exactly (576
lines, ~20 s, early exit before system_server), proving the T718
flight-01 results depend on the T705 kernel + fix stack, not on the
harness. Futex regression vector: absent (0 matches for ESR
0x96000005 / Data Abort) in the control AND in flight-01 — the
control exits at 20.19 s, before the probe point; recorded honestly,
not claimed.

## 8. First failing link and the precise next fix

First failing link: the present path between SurfaceFlinger's
Scheduler and the composer/display. Allocation is proven working
(604 calls, 695 imports, 0 failures); the display is formed and ON;
the composer receives commands; but no buffer is ever queued or
presented in the observable UART, and all 2241 Scheduler frame-track
attempts report an invalid present fence.

Next fix (for the successor lane): determine why every present fence
is invalid, in this order: (a) disassemble the Scheduler's
trackPendingFrame fence-validity check and the producer fence path
(BufferQueueProducer queueBuffer acquire-fence origin) in the
guest's own libraries, framework-first, before any build; (b)
establish whether an invalid fence drops the frame or only skips
frame tracking (if it only skips tracking, the present blockage is
elsewhere and the fence errors are noise — discriminate with
guest-origin evidence, e.g. SF dumpsys/fence state or a composer
present-call trace, never with snapshot byte diffs); (c) apply the
smallest fix at the proven link and fly the same criteria
(queueBuffer > 0 + first present dispatch + holds). Do NOT rework
the v4 parser or the T718 flag fix (both proven). The GateKeeper NPE
wall is a separate named lane (stub/guard the GateKeeper service so
LockSettingsService acquires it); it is NOT this lane's failing link
(the second system_server lifetime survived with boot completed and
the allocation path fully working).

## 9. Non-regressions and lane hygiene

- No macOS debugfs anywhere in this lane. All image work on the box
  via Linux debugfs (vendor) and CPIO member replacement (initramfs).
- No flight folder overwritten in place. The smoke, flight-01, and
  flight-ab each ran in their own flights/ dir; inherited JSONs were
  superseded by the supervisor's own writes (preflight/flight-end),
  and no UART was overwritten (T716/T717 UARTs live in their own
  worktrees and the vault).
- T664-T717 fixes not regressed: SYS_RAWIO lines intact in the flown
  vendor image; v4 parser bytes byte-identical in the flown binary;
  installd trigger untouched (system.img 1283cbf2, installd.rc
  unchanged); composer ELF 8d62a642 intact; all T716 holds verified
  in flight-01 (section 6).
- No commit made by the lane agent. No agyx. Route: sandbox subagent
  driving the Mac only via macssh.sh, box via boxssh.sh.
- Variances from the brief: V1 — the smoke stopped on the 300 s host
  cap at 191.7 s guest time (supervisor kernel-timestamp scan
  plateau in logcat-only userspace); it ran ~90 s past the ~102 s
  allocation onset and captured the decisive lines, so the smoke
  criterion was adjudicated on the evidence, not the clock. V2 —
  the futex regression vector is absent from both flights (section
  7); the control's early exit precedes the probe point. V3 — the
  brief's anticipated "second footprint patch in write_result's own
  function body" was NOT needed: the fix fits the 17-slot epilogue
  plus the dead PLT stub, a smaller change than a full rewrite.

## 10. Reflection (REFLECT-ON-EVERY-PROBLEM)

### R1. The 11f24 near-miss: a dead-code hypothesis, refuted before it hurt

(1) What happened, with evidence? During patch design I hypothesised
that 4 instructions at 0x11f24-0x11f30 were dead code and could host
part of the trampoline. On re-examination of the destroy-loop
disassembly I proved 11f24 is LIVE: it is reached via the temp-vector
destroy loop when the fds vector is non-null at 11bd0, with w19
holding a pointer whose bit 0 is 0. No bytes were ever written there.
(2) Root cause: I inferred deadness from local appearance (the code
looked unreachable from the path I had traced) instead of proving
unreachability over the whole function. (3) Missed signal: the
destroy loop at 11bd0 branches backwards into the region — a
whole-function control-flow read, not a local read, was the check
that caught it. (4) What changes: no dead-space claim is accepted
without a whole-binary call-site/reachability probe (the PLT probe
that found the truly dead 0x34ab0 stub is the template: every
candidate address must show zero inbound branches in the full
disassembly). (5) Where it lives: this section; the rule is
restated for the successor lane in section 8's "framework-first"
instruction, and the splice script's assertions (original bytes +
diff confinement) are the mechanical backstop for any future
footprint patch.

### R2. The -129 mechanism gap: a hand trace that predicted the wrong code

(1) What happened? My hand byte-trace of the Bp misparse predicted
-22; the guest observed -129. I recorded the gap honestly instead of
forcing the trace to fit, and let the smoke discriminate. The flag
fix cleared -129 with no other change, establishing the empirical
link. (2) Root cause: a client-side parse trace cannot simulate
parcel-driver internals; transact-level failures have causes
invisible to a reply-byte trace. (3) Missed signal: none available
in-lane — the -129 mechanism was genuinely underdetermined by
disassembly alone. (4) What changes: when disassembly proves a
protocol violation but cannot model the failure code, the fix ships
as a discriminating experiment with the gap stated in the findings,
not as a fully-modelled root cause. (5) Where it lives: section 3.3
of these findings; the pattern (proven violation + stated gap +
empirical discriminator) is the template for the successor lane's
fence investigation (section 8b).

### R3. Tooling traps caught by their own checks (compact)

The DTB script's `/chosen` path bug was caught by its own assertion
before any write (assert-first scripts work — keep them). The
objdump `-d` empty output on raw binaries was caught by noticing the
empty dump, not by assuming success (empty output is never a pass).
A macssh payload broke on inner single quotes; multi-line Mac
payloads now go via `bash -s` + stdin (the pattern used for all
later progress/heartbeat writes in this lane). The smoke's guest-time
plateau looked like a wedge; UART growth + QEMU CPU proved life
(the supervisor's last-1MB timestamp scan is a known limitation —
poll UART size and process state before declaring a stall).

## 11. Artifacts

Fixed binary (T718): 3daa5a491e9d04ff56bbb7998be1eb91a1b3ed625d66b48f264a04b04ca70932 (1,711,816 B, 73 bytes differ from v4, parser footprint identical).
Vendor image: bac3cdc112dc6c3257c016fb4f600c46f43b9372f3d2f2ebb9b7d79fd05daeca (134,217,728 B, inode 731).
Fix CPIO: 1150c8410657fe40c99bc96c0f5e61af2dd0938087011076dc7015a85f127d9e (53,379,397 B).
Modified DTB: d298f5adc8caa8426eb4cfdc72f88bdeb070f90210db6bc1a10622303a2f53d9 (2,049 B, initrd-end 0x1132e8145).
Smoke UART: 137fb562e364f1d4bab62da849ef6f4cfd7e411aff0109b0184da879160d230d (13,816 lines).
Flight-01 UART: 081a9381aba9de5d6e946fd7df11e231585acf1e098a97673dcef5be0aed03db (323,607 lines).
Flight-ab UART: 1a9a58fa6389379efcaff2e6752161eff8ffcef9efdf5824715bfb82444fbec0 (576 lines).
Patch sources + scripts: epilogue.s, trampoline.s, build_t718.sh,
splice_t718.py, patch_vendor_t718.py, cpio_replace_t718.py,
dtb_patch_t718.py (sandbox ~/workspace/t718-shim-patch/; box
/home/muse/t718-work/; box patch dir /home/muse/t718-work/patch/).
Flight scripts: supervisor_t718.py, verify_smoke_t718.py,
verify_flight_ab_t718.py, analyze_t718_flight01.py,
launch_smoke_t718.py, launch_flight_01_t718.py,
launch_flight_ab_t718.py, launch_smoke_ext_t718.py (worktree
scripts/ + sandbox ~/workspace/t718-scripts/).
Sandbox image copies: ~/workspace/t718-images/ (fixed binary,
vendor, CPIO, DTB, patch report).
Box work dir: /home/muse/t718-work/ (framework .so files, fixed
binary, images, patch report, unpacked CPIO t718-work.cpio).
Guest inputs consumed (shared contract, unchanged): kernel
53d9563e, system.img 1283cbf2, canonical Image 05c43917, DTB base
1a961795, composer ELF 8d62a642, extracted_kernel.config d840310a.
