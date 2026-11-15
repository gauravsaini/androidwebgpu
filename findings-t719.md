# Findings — Track T719: invalid present fence

Verdict: FAIL

## Summary

T719 set out to break the "Invalid present fence" wall: 2,241
`trackPendingFrame: Invalid present fence` errors in the T718 flight-01
UART, with `queueBuffer` 0 and `present` 0. Step 2 disassembly PROVED the
chain end-to-end from the guest's own binaries: the composer backend
hardcodes `setPresentFence(-1)` on every present path, and
SurfaceFlinger's Scheduler logs the error exactly when the present
fence's cached signal time is -1 — which is exactly when the fence fd
is -1. The guest kernel has no `CONFIG_SW_SYNC`, so no fence-creation
mechanism exists at all; the composer's statically linked `sw_sync_*`
functions have zero call sites (dead code).

The smallest fix at the proven link — a 4-byte patch to the guest
surfaceflinger binary that skips the log line — was built, verified,
and flown as a SCAFFOLD (recorded honestly below: the underlying -1
retire fence remains). Flight-01 CONFIRMED the scaffold's prediction
exactly: fence errors 2,241 → 0, with allocation intact (REJECT 0,
571 allocate2 CALLs, 702 imports, 0 failures), all T716 holds intact
(EPERM 0, simplefb mapped 1), 0 Watchdog kills, and `sys.boot_completed`
present 11 times. The Scheduler was reduced to its 2 pacesetter lines.

The verdict is FAIL as written: the brief's literal PASS criteria
(queueBuffer > 0 and a first present dispatch) remain unachieved, and
the fence wall is silenced by scaffold only, not fixed. The REAL fix —
a kernel fence source plus a real retire fence in the composer backend
plus EGL native-fence support — is named in §6 as a multi-lane program.

## WALL-TYPE-TAG: NO-FENCE-SOURCE

Rust par bhi lagu: the Rust emulator must provide a fence source
equivalent to /dev/sw_sync and a real retire fence, or SurfaceFlinger
will hit the same -1 retire-fence path there; the scaffold patch is
guest-binary-specific and does not transfer to the Rust emulator.

## 1. Step 0 — Mac disk gate: PASS

Recorded 2026-11-16 06:10:50 AEDT: Mac data volume 228 Gi, 12 Gi
available, 95% used — above the 8 GiB gate.

## 2. Step 1 — Input verification: PASS

Recorded 2026-11-16 06:10:50 AEDT. Every input SHA re-hashed and
matched; no network fetch. Findings-t718 `da5fc919`, death note
`62d00876`, smoke UART `137fb562`, flight-01 UART `081a9381`,
flight-ab UART `1a9a58fa`, findings-t717 `2e2381cd`, death note
`b126e186`, T716 flight-01 `b178e4c6`; guest-inputs 51 files
byte-identical T718 vs T719 including the t718 stack
(`bac3cdc1`/`1150c841`/`d298f5ad`), system.img `1283cbf2`, Image
`53d9563e`, config `d840310a`, composer ELF `8d62a642`; box artifacts
(fixed binary `3daa5a49`, v4 `bf8661d0` x2, orig `745d19d0` x2, vault
images match); sandbox drafts/images match.

## 3. Step 2 — Disassembly: the fence chain, proven

All disassembly is from the guest's own binaries, extracted read-only
from the vault images. Framework-first: the check was found in
SurfaceFlinger/Scheduler, the origin in the composer backend.

### 3.1 The binaries

Extracted via debugfs from the vault system.img (recorded 2026-11-16
06:11:53 AEDT):

- surfaceflinger: SHA
  `259d39f54ef6bbac9cc90991f913ef0bd8cdcde5e9cbbd6a828b36fc6298061b`,
  8,336,320 B, stripped.
- libgui.so: SHA `5082485c…`, 1,690,656 B, dynamic symbols present.
- libui.so: SHA `e14f836c…`, 412,256 B.
- libsync.so: SHA `e79e3721…`, 133,512 B.
- allocator-V2-ndk.so: SHA `a941d842…`, byte-identical to T718.
- Composer ELF (from flown vendor.img): SHA `8d62a642…`, 19,942,712 B,
  FULL symbol table, 7,090 symbols.

Image paths are `/system/bin/…` and `/system/lib64/…` (`/bin` is a
symlink; `/lib64` does not exist at the image root).

### 3.2 The fence-invalidity check (PROVEN)

- String `"%s: Invalid present fence"` at surfaceflinger file
  offset/vaddr `0x49f7a` (identity LOAD map); `"trackPendingFrame"` at
  `0x45695`; tag `"Scheduler"` at `0x3b34d`.
- Single log site at `0x2040d8–0x2040f4` (`__android_log_print`,
  priority 6). The check at `0x204054–0x204060`:
  `FenceTime::getCachedSignalTime()` then `cmn x0, #1; b.eq 0x2040d8`
  — cached signal time == -1 triggers the log.
- `FenceTime::getCachedSignalTime` (libui `0x3d120`): `ldar` of the
  int64 at FenceTime+56.
- `FenceTime` ctor from `sp<Fence>` (libui `0x3cd10`): field[0] =
  (fd == -1), fd read from Fence+12; cached = INT64_MAX if the fence
  is valid, -1 if invalid (CSINV semantics verified: the
  condition-true path selects Xn; an earlier inverted reading was
  caught and corrected in-lane).
- `FenceTime::getSignalTime` (`0x3cf74`): computes (waits) only when
  cached == INT64_MAX; returns cached as-is otherwise.
- `Fence::wait` (libui `0x33e04`): fd at +12 confirmed; fd == -1
  returns 0 immediately (already signaled).
- `Fence::getSignalTime` (`0x3ca58`): fd == -1 takes the invalid path,
  else `sync_file_info`.
- `FenceTime` ctor (fence,bool) at `0x3d1ac` confirms the same 3-state
  field[0] (0 valid / 1 invalid / 2 trusted).

Chain proven end-to-end: present fence fd == -1 ⟺ cached signal time
-1 ⟺ "Invalid present fence" log.

### 3.3 The fence origin (PROVEN — smoking gun)

`ComposerClientImpl::present()` (composer ELF `0x5bf1c`): blits
client-target handles via `SimpleFb::blit` (`0x5c114`, mmap+memcpy to
fb0) and/or fills the mapped framebuffer with a counter-derived color,
then on ALL paths converges at `0x5bff0` and tail-calls
`setPresentFence(-1)` — `mov w1, #-1` hardcoded at `0x5bff4`.

`sw_sync_*` functions are statically linked but have ZERO call sites
(whole-disassembly count, verified) — dead code. Kernel config
`extracted_kernel.config`: NO `CONFIG_SW_SYNC` (`CONFIG_SYNC_FILE=y`,
`CONFIG_DMA_SHARED_BUFFER=y` only) → `/dev/sw_sync` absent → no
fence-creation mechanism exists in the guest.

### 3.4 §8b discrimination — SKIP, not DROP (PROVEN at both ends)

(a) The Scheduler error path does NOT return: after logging it
branches back into the loop (`b 0x204078` at `0x2040f8`), skipping
only that entry's timing math; the INT64_MAX (pending) path at
`0x204144` re-inserts the entry for later processing. (b) The composer
writes fb0 (blit/fill) BEFORE returning the -1 fence. The log block at
`0x2040d8` has exactly ONE inbound branch in the whole binary (the
`0x204060 b.eq`) — patch-site uniqueness proven by a full-disassembly
scan.

### 3.5 UART correlation (T718 flight-01)

2,241 fence errors vs only 164 RenderEngine `failed to dup EGL native
fence sync: 0x3000` lines (first L23309, 23:24:46.517) — the EGL dup
failure is a secondary contributor, not the main source. RenderEngine
= SkiaGL backend + "SimpleFB CPU GLES Rasterizer" (vendor "OpenAI").
Composer service: 22,838 lines, 0 E-lines. BLASTBufferQueue activity
present (FallbackHome queues churning). `queueBuffer`/`presentDisplay`
literals = 0 — PROVEN log-visibility artifact: the surfaceflinger
binary contains `"ANativeWindow::queueBuffer failed with error: %d"`
(offset `0x50102`, error-only) and `", presentDisplay: "` (offset
`0x5368d`, stats fragment); libgui has NO queueBuffer log-message
strings (mangled symbols only). Successful queueBuffer/present calls
log nothing at this level. "Unknown command" composer lines are
binder-dump misreads (garbage values), not HWC opcodes. PID 2488 is
SurfaceFlinger.

## 4. The fix: 4-byte scaffold patch (built and verified)

### 4.1 Patch

File offset `0x204060` (== vaddr) in the guest surfaceflinger binary:

- Before: `c0 03 00 54` — `b.eq 0x2040d8` (log block).
- After: `06 00 00 14` — `b 0x204078` (skip silently, rejoin the
  loop-continue).

Exactly 3 bytes differ (offsets `0x204060/61/63`; `0x204062` is `0x00`
in both). Changes NO behaviour except the log line: the patched branch
lands on the exact instruction the log block itself branches to
(`0x2040f8: b 0x204078`).

### 4.2 Scaffold admission (honest)

This is a SCAFFOLD, not a fix. The underlying -1 retire fence remains;
the Scheduler still skips the timing math for every frame. The patch
exists as the empirical discriminator (T718-R2 pattern): if the errors
go to 0 with everything else identical, the noise hypothesis is
confirmed in the guest. Flight-01 confirmed it: 2,241 → 0. Rejected
alternatives: patching the composer `-1` (no valid fd exists to
return); patching the libui ctor (only delays the -1 by one
`getSignalTime` call); a diagnostic-logging flight (the disassembly is
conclusive, not inconclusive).

### 4.3 Build record

- Splice script `splice_t719.py`: asserted orig SHA `259d39f5…`, size
  8,336,320, orig bytes at `0x204060`; wrote the patched binary;
  asserted exactly the 3 diff bytes. Patched binary SHA:
  `fadee423d50c8da59918cc568e932c5e6dfe11553817625cdc82b77c6bc59312`
  (8,336,320 B). In-context disassembly confirmed `b 204078` at
  `0x204060` with neighbours intact.
- Image patch script `patch_system_t719.py` (T706/T718 Linux debugfs
  procedure): copied the vault base image (never modified), rm-first-
  then-write of `/system/bin/surfaceflinger`, mode `0100755`, uid 0,
  gid 2000, xattr `u:object_r:surfaceflinger_exec:s0`, `e2fsck -f -y`
  then `-f -n` (code 0), dumped-binary byte-compare, metadata/xattr
  verify, 9 neighbours byte-identical, base untouched.
- New system.img SHA:
  `ac15a68ac1fb95c51eb3476f76692dfdda7e413e3d9e52f8cf8312b02a2ad5dd`
  (2,059,816,960 B). Inode 556 reused. installd.rc byte-identical
  (`3ea3466b`, 4,958 B) — the T715 installd trigger is preserved.
  SHA-verified at every hop: box → sandbox → Mac guest-inputs.
- T718 stack unchanged: vendor `bac3cdc1`, CPIO `1150c841`, DTB
  `d298f5ad`, Image `53d9563e`, shim binary `3daa5a49` (the system.img
  patch does not touch vendor.img, so the parser + flag-fix bytes are
  intact).

### 4.4 Flight scripts

Seven T719 scripts adapted from the proven T718 versions, import-tested
and fixture-verified against the T718 UARTs before flight (the analyzer
reproduces flight-01 exactly: REJECT 0, fence errors 2,241, dup-fail
164, EPERM 0, mapped 1, kills 0, boot_completed 11; verify_smoke
reproduces the smoke: REJECT 0, CALL 2; verify_flight_ab reproduces
the control: 576 lines, 20.19 s). Supervisor pinned to the flown
artifact SHAs including the new system.img SHA. One fixture catch: the
first FallbackHome regex (`FallbackHome.*Displayed`) matched 0 lines;
the real format is `Displayed.*FallbackHome` (2 lines, L19293/L32645
in T718) — fixed before flight.

## 5. Flight evidence

All flights ran on the Mac (QEMU 11.1.2) from the T719 worktree with
the pinned stack. UARTs are preserved in `flights/`.

### 5.1 Smoke (Step 3a) — PASS, regression gate clear

Launched 2026-11-16 07:28:19 AEDT. Guest target reached (256.0 s),
QEMU exit 0, 13,509 lines, 1,403,041 B, UART SHA
`bfd345b812ec1393f0cfc88bbb79efc6b6f9d77e3dd8d4fb8e51078de21ef691`.

- T628 REJECT 0, CALL allocate2 2, GraphicBufferAllocator failures 0,
  allocateBuffers failures 0 — hard smoke criterion met with the shim
  exercised (T628 lines 4, system_server started 1, guest 256 s).
- EPERM 0, installd-not-found 0, Watchdog kills 0.
- Fence errors 0, RenderEngine dup-fail 0 (informational: fence
  errors appear post-boot, past the smoke window).
- simplefb mapped 0 — adjudicated from flight-01, not this smoke
  (first open attempt at guest ~522 s in T715).

### 5.2 Flight-01 (Step 3b) — scaffold confirmed, criteria unmet

Launched 2026-11-16 07:34:52 AEDT. Guest target reached (2,403.7 s),
QEMU exit 0, 309,407 lines, 40,769,960 B, UART SHA
`65c4cd9a1489d834c5c4527838c7961671f55f721d6c6843b5821b155dc53a33`.

Scaffold effect (decisive):

- Invalid present fence lines: 0 (T718 flight-01: 2,241). The
  scaffold prediction is confirmed in the guest.
- Scheduler(2488) lines: 2, both `Display 0 is the pacesetter`
  (L10636, L10641) — the Scheduler is otherwise silent.

Allocation holds intact:

- T628 REJECT 0; CALL allocate2 571 (first L10387); T628 lines 667;
  importBuffer 702; T637_MAPPER 1,166; GraphicBufferAllocator
  failures 0; allocateBuffers failures 0.

T716/T715 holds intact:

- EPERM 0; simplefb mapped 1 (L22871); Watchdog kills 0.
- installd firing up L9656; installd-not-found 0.
- Dexopt Launcher3 status=PERFORMED (L13472); SKIPPED 0.
- SensorService onBootPhase RETURN 2 (both system_server lifetimes).
- boot_completed 11; sys.boot_completed 11 (first L20636).

Parallel GateKeeper NPE wall (unchanged pattern):

- system_server starts 2 (L10931, L25665): 1 death, lifetime 2
  survived. GateKeeper NPE at L22909
  (`reportDeviceSetupComplete()` on a null object reference).
- FATAL EXCEPTION 7 (L24358, L24364, L24370, L24395, L24406, L24411,
  L24414), all DeadSystemException collateral (7 DeadSystemException
  lines); removeRegionSamplingListener 0.

Tolerated / secondary:

- getDisplayAttribute failed 1 (L10624, Unsupported (8) — tolerated
  per T717 Step 2).
- RenderEngine EGL dup-fail 178 (first L23085; T718: 164) — the EGL
  side persists, as expected: the scaffold does not touch EGL.
- FallbackHome Displayed 2; composer-service E-lines 0; Kernel
  Oops/panic 0; RAMDISK incomplete write 0.

Unachieved (literal PASS criteria):

- queueBuffer lines 0; presentDisplay lines 0 — as proven in §3.5,
  these literals are unobservable on success paths at this log level,
  so the literal criteria cannot be met by a successful flight. No
  first present dispatch is evidenced in the UART.

### 5.3 Flight-ab control (canonical kernel)

Launched 2026-11-16 08:16:03 AEDT. Canonical kernel self-exited
(returncode 0) at guest 19.4 s, 575 lines, 47,928 B, UART SHA
`fbbc1b097e295655cd9e64a563ae432362f7c5a6b2d9d516c183ce817f810814`.
system_server starts 0, installd 0, queueBuffer 0, presentDisplay 0,
EPERM 0, mapped 0, T628 0, boot_completed 0, fence errors 0. The
control pattern matches T716/T718 (576 lines, 20.19 s, QEMU self-exit
0): the canonical kernel exits before system_server, confirming the
flight-01 results come from the T719 stack, not the harness.

### 5.4 Verdict reasoning

FAIL as written. The scaffold did exactly what the disassembly
predicted (fence errors 2,241 → 0, everything else intact), which
confirms the noise hypothesis and the proven chain — but silencing a
log line is not breaking the wall. The wall is the missing fence
source: no `CONFIG_SW_SYNC`, no `/dev/sw_sync`, composer hardcoded
to -1. The literal PASS criteria (queueBuffer > 0, first present
dispatch) are unachieved, and the first is unobservable on success
paths, so no flight of this stack can meet it as written.

## 6. The real fix (named, out of scope)

A kernel fence source (`CONFIG_SW_SYNC` kernel rebuild) + an sw_sync
retire fence in the composer backend + EGL native-fence support — a
multi-lane program. The scaffold in this lane must be reverted when
the real fix lands; it must never be mistaken for the fix. The next
lane starts from the kernel config, not from another binary patch.

## 7. Reflection

1. What happened, with evidence? The lane proved the fence chain
   end-to-end from disassembly (§3), built a 4-byte scaffold patch,
   and flight-01 confirmed the prediction exactly (fence errors
   2,241 → 0, allocation and boot holds intact). The verdict is
   nonetheless FAIL as written: the literal PASS criteria are
   unachieved and the underlying -1 retire fence remains.
2. Why — root cause? The guest has no fence-creation mechanism at
   all: the kernel lacks `CONFIG_SW_SYNC` and the composer backend
   hardcodes `setPresentFence(-1)`. A binary patch at the log site
   can only silence the symptom; the fix requires a kernel rebuild
   plus a composer-backend change, which is a multi-lane program
   outside this lane's smallest-fix scope.
3. Missed signal? None material in-lane. The disassembly identified
   the missing fence source BEFORE the flight, and the
   scaffold-vs-fix distinction was recorded before flying — the
   flight confirmed a stated prediction rather than revealing a
   surprise. Two in-lane catches worked as designed: the CSINV
   inversion misread was caught and corrected against both branch
   outcomes, and the FallbackHome regex miss was caught by fixture
   verification before flight.
4. What changes? (a) The successor brief must carry the
   NO-FENCE-SOURCE wall and start from the kernel config
   (`CONFIG_SW_SYNC` rebuild), with the scaffold-revert requirement
   stated. (b) The literal queueBuffer>0 criterion must be replaced
   by an observable proxy in future briefs, since §3.5 proves the
   literal is unobservable on success paths. (c) Multi-GB transfers
   keep `--partial` always: a sandbox runtime restart interrupted
   hop 1 at 61%, and the resume completed without re-transferring
   the prefix or duplicating any effect. (d) Lane heartbeat writes
   during flights are overwritten every 15 s by the supervisor's own
   progress JSON in `heartbeat.json`; the CLOSED heartbeat must be
   written only after all supervisors have exited.
5. Where it lives? This section + §6 + the death note. The
   successor brief and the driver record carry (a) and (b); (c) and
   (d) are lane-mechanics notes for the driver.

## 8. Artifacts

- Patched surfaceflinger binary: SHA
  `fadee423d50c8da59918cc568e932c5e6dfe11553817625cdc82b77c6bc59312`,
  8,336,320 B (box `/home/muse/t719-work/`).
- Patched system.img: SHA
  `ac15a68ac1fb95c51eb3476f76692dfdda7e413e3d9e52f8cf8312b02a2ad5dd`,
  2,059,816,960 B (box, sandbox staging, Mac guest-inputs/).
- Patch report: `/home/muse/t719-work/t719_patch_report.json` (box).
- Smoke UART: SHA
  `bfd345b812ec1393f0cfc88bbb79efc6b6f9d77e3dd8d4fb8e51078de21ef691`
  (13,509 lines).
- Flight-01 UART: SHA
  `65c4cd9a1489d834c5c4527838c7961671f55f721d6c6843b5821b155dc53a33`
  (309,407 lines).
- Flight-ab UART: SHA
  `fbbc1b097e295655cd9e64a563ae432362f7c5a6b2d9d516c183ce817f810814`
  (575 lines).
- Flight evidence: `flights/smoke/`, `flights/flight-01/`,
  `flights/flight-ab/` in the worktree.
- Scripts: `scripts/*t719*.py` in the worktree.
