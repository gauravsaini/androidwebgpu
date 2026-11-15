# Findings — Track T716: composer /dev/mem EPERM diagnosis + CAP_SYS_RAWIO fix + first present dispatch

Verdict: FAIL

WALL-TYPE-TAG: guest

`Rust par bhi lagu`: haan — the fix is guest-image init configuration (the
composer service's `capabilities` line in vendor.img). Any emulator that
runs this Android guest — QEMU scaffold or the Rust emulator — boots the
same init, which applies the same capability set to the same service. The
Rust emulator's init/capability handling must honour the `.rc`
`capabilities` field exactly as Android init does, or the same EPERM
recurs under Rust. The guest image fix carries over unchanged; no
Rust-side workaround is needed or wanted.

## 0. Lane identity

- Track: T716 (composer /dev/mem). Worktree:
  `/Users/Shared/wt-track716-composer-devmem`, branch `t716`, base `da6452d`
  (T715). Lane run by a sandbox subagent driving the Mac via `macssh.sh`
  ONLY — no agent process launched on the Mac, no agyx/codex/opencode
  (T715 V6 route). No commit by the lane agent.
- Launch: 00:49:56 AEDT 16 Nov 2026 (spawn receipt). Wedge trigger:
  launch + 3h ≈ 03:50 AEDT. The 00:56 time in the first progress line was
  an estimate and is superseded (clock correction recorded in
  progress-t716.txt; authoritative times are UTC stamps in flight-end.json
  and supervisor logs).
- QEMU: 11.1.2, `/opt/homebrew/bin/qemu-system-aarch64`, SHA
  `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869`
  (31,498,864 B, unchanged from T715).

## 1. Mission outcome

T715 proved the installd trigger works and isolated the display wall to
one line: `simplefb: open /dev/mem failed: Operation not permitted`
(5 occurrences in flight-01, first L23338), leaving queueBuffer 0 and
presentDisplay 0. This lane diagnosed the EPERM from evidence, granted
the mapping by the smallest route (service capabilities), proved the fix
took effect in the guest UART, and flew the full 2400 s window.

The EPERM wall is BROKEN: 0 EPERM lines in every T716 flight, and the
composer mapped the framebuffer 7 times in flight-01 (first L22650).
The extended smoke PASSED on the brief's hard criterion. But the lane
verdict is FAIL: the display still never presents. queueBuffer 0,
presentDisplay 0, 742 `Invalid present fence` lines, and 1 genuine
composer-path Launcher3 FATAL (`Couldn't removeRegionSamplingListener`,
L81124). The wall moved downstream to a PRE-EXISTING, parallel wall this
lane did not create and did not fix: every buffer allocation in the
flight fails with a 0x0 size (42 `allocateBuffers` failures, all 0x0),
starting with HWComposer `getDisplayAttribute failed: Unsupported (8)`
at L10878 — the same failure shape T715 already showed (54 allocation
failures, 5 attribute failures). No buffer is ever allocated, so no
frame can ever be queued or presented, even though the framebuffer is
mapped and waiting. The exact first failing link and next fix are in §5
and §9.

## 2. Diagnosis (Step 2, from evidence)

### 2.1 Root cause: the composer service lacks CAP_SYS_RAWIO

The failure is a missing Linux capability, not DAC, not SELinux, and not
STRICT_DEVMEM. `open("/dev/mem")` returns EPERM because the kernel's
`mem_open()` requires `CAP_SYS_RAWIO`, and the composer service's
capability set is `SYS_NICE` only.

Three definitions of `vendor.hwc-2-1` exist. The effective one is proven
by the T715 flight-01 UART:

1. Ramdisk `init.rc` L167 (in `t713-fix.cpio.gz`): `user system`, `group
   graphics drmrpc`, `capabilities SYS_NICE`. First-stage only; dead after
   switch_root (guest 8.93 s). The service starts at guest 87 s in second
   stage. NOT patched: zero effect, adds CPIO/DTB risk for nothing.
2. Vendor `/etc/init/android.hardware.graphics.composer@2.1-service.rc`
   (inode 305, 249 B, mtime 18-Oct-2026, `user root`): `group graphics
   drmrpc`, `capabilities SYS_NICE`. PATCHED (see §3).
3. Vendor `/etc/init/hw/init.virt.rc` L36 (inode 727, 1,683 B, mtime
   9-Nov-2026): `user root`, `group root graphics drmrpc`, `capabilities
   SYS_NICE`, `seclabel u:r:hal_graphics_composer_default:s0`,
   `stdio_to_kmsg`. PROVEN EFFECTIVE: T715 flight-01 UART L9254
   `[87.005906] init: Command 'start vendor.hwc-2-1'
   action=early-boot (/vendor/etc/init/hw/init.virt.rc:47) took 1180ms
   and succeeded`, pid 2417. PATCHED (see §3).

No composer definition exists in system.img (rdump + grep empty). No init
duplicate-definition warnings appear in the UART. Both vendor definitions
were patched to kill all ambiguity about which wins.

CAUTION (recorded for the next lane): `init.virt.rc` has TWO
`capabilities SYS_NICE` lines (allocator stanza + hwc stanza). Only the
hwc stanza was patched, using unique context (`group root graphics
drmrpc` + `seclabel u:r:hal_graphics_composer_default:s0`). The allocator
stanza is byte-identical after the patch (verified by dump comparison).

### 2.2 Ruled out: DAC, ueventd, SELinux-as-blocker, STRICT_DEVMEM

- DAC: `/dev/mem` gets the devtmpfs default node (no ueventd rule in
  vendor `ueventd.rc` or ramdisk `ueventd.rc` — the only "mem" match in
  the ramdisk file is `/dev/ashmem*`). The service runs as root, so DAC
  passes via root ownership. Route (b) ueventd cannot fix a capability
  failure.
- SELinux — PREMISE CONTRADICTION, RECORDED AND RE-DERIVED (brief §5):
  the brief's premise was that no avc denial exists for the composer.
  That premise is WRONG in letter: T715 flight-01 has 8,786 `avc:
  denied` (TWO spaces) lines, 15 of them for `/dev/mem` (3 per attempt ×
  5 attempts). An early lane grep for single-space `avc: denied` matched
  0 lines and produced a wrong "0 avc denials" claim in the first
  diagnosis progress line; corrected 01:20 AEDT with a written
  reflection (root cause: grepped from memory instead of the proven
  two-space pattern already in the T715 verify scripts). The premise
  HOLDS in substance: every denial carries `permissive=1` (logged, NOT
  enforced) and precedes each EPERM line by ~3 ms. SELinux is ruled out
  AS THE BLOCKER; the EPERM is the `mem_open()` capability check. The
  T716 extended smoke CONFIRMED this re-derivation: `/dev/mem` avc
  denials persist (6 lines, `{open}` + `{map}`, all `permissive=1`)
  while the open succeeds and the mapping lands (§4).
- STRICT_DEVMEM: the flown kernel's config was extracted with
  `extract-ikconfig` from the tree Image itself (SHA `53d9563e…`,
  identical to the flown T705 Image): `CONFIG_DEVMEM=y`, `CONFIG_FB=y`,
  `CONFIG_FB_SIMPLE=y`, `CONFIG_STRICT_DEVMEM=y`. `devmem_is_allowed()`
  in `arch/arm64/mm/mmap.c:148` returns 1 when the page is not System RAM
  and not iomem-exclusive; the simplefb framebuffer at `0x5F000000` is
  device memory (kernel UART L97: `simple-framebuffer 5f000000.framebuffer:
  fb0: simplefb registered!`). Route (d) kernel rebuild NOT needed. The
  extended smoke proved this empirically: the mmap succeeded (§4).
  WARNING: do NOT cite `extracted_kernel.config` (`d840310a…`) for the
  flown kernel — it is the CANONICAL config (`# CONFIG_DEVMEM is not
  set`, `# CONFIG_FB is not set`, the T685 finding).
- Ramdisk overlay shadowed (T713 lesson applied): the initramfs carries
  its own `vendor/etc/init/*.rc` overlay (composer .rc 229 B, `user
  system`, `SYS_NICE` only, two copies) and a stock composer binary
  (2,223,240 B). These are SHADOWED by the vendor.img mount: the overlay
  has NO `init.virt.rc`, while the UART start action cites
  `init.virt.rc:47`, which exists ONLY in vendor.img. The guest runs the
  T708 rebuilt ELF from vendor.img (dumped from vendor.img on the box:
  SHA `8d62a642…`, 19,942,712 B). The overlay was NOT patched.
- Composer source tree `/Users/Shared/composer-2-1-tree` (HEAD
  `40e9f153…`) has NO simplefb backend (HIDL interfaces + utils only;
  greps for `simplefb` and `dev/mem` empty). The backend is the T681
  custom simplefb vendor service (banner `T681 Composer 2.1 simplefb
  vendor service` in the ELF; `kSimpleFbPhys` symbol). The ELF contains
  NO `/dev/fb0` string — route (c) fb0-based mapping would need code
  changes; route (a) is smallest.

### 2.3 Success line (identified before flying)

From ELF strings: `simplefb: mapped 0x%lx %ux%u stride=%u bpp=%u`.
Count in T715 flight-01 UART: 0. Smoke criterion: 0 EPERM lines AND this
line present.

## 3. The fix (Step 2, shipped)

Route (a): add `SYS_RAWIO` to the composer service `capabilities` in
BOTH vendor.img definitions (`SYS_NICE` → `SYS_NICE SYS_RAWIO`, +10 bytes
each). Nothing else changed.

### 3.1 Shipping route: CPIO member replacement (mandatory)

The guest mounts the EMBEDDED vendor.img, not the standalone file: T715
flight-01 UART L310 `[T705] guest attached embedded vendor.img to loop1`;
CPIO member `ramdisk-images/vendor.img` SHA = `095386ab…` =
`t712-vendor.img`; the QEMU command line has NO vendor drive (only the
system.img virtio-blk drive). Patching the standalone vendor.img alone
would do nothing.

### 3.2 Vendor.img patch (box, Linux debugfs, rm-first-then-write)

Vendor.img has 0 free inodes (Inode count 736, Free inodes 0, Block count
32768, Free blocks 3603, 4K blocks), so the T715 write-first link-swap
cannot allocate. Adapted procedure (same safety properties, one
reordering): rm the old file (frees exactly one inode) → write the new
file (reuses it) → set mode/uid/gid/xattr explicitly → e2fsck. The image
is an offline copy; no guest runs during the patch. Script:
`t716-work/patch_vendor.py` on the box (adapted from
`patch_t715_system_img.py`).

Verification (all PASS, exit 0):
- RC1 249 → 259 B, SHA `47a9bd4a…` (inode 305, reused); RC2 1683 → 1693 B,
  SHA `468f2a85…` (inode 727, reused). Allocator stanza untouched.
- Mode 0644, uid/gid 0, xattr `u:object_r:vendor_configs_file:s0` on both.
- `e2fsck -f -y` exit 0 (no repairs needed), `e2fsck -f -n` exit 0.
- Both dumped files byte-identical to intended content.
- 6 neighbour files byte-identical (ranchu rc, ueventd rc, allocator rc,
  swiftshader rc, t628 rc, t637 rc); composer ELF byte-identical
  (`8d62a642…`); `hals.conf` unchanged (44 B).
- Vault original untouched (SHA `095386ab…` re-verified after).
- NEW `t716-vendor.img` SHA
  `9a2519b17ff8145f835ec6320e3c1b8f11e711b0c3ac70c868fef0c91e22a6af`
  (134,217,728 B, same size).

### 3.3 CPIO surgery + DTB re-align

- Parsed the uncompressed newc stream (725,742,732 B, 238 members):
  `ramdisk-images/vendor.img` at data offset 2972756, SHA `095386ab…`.
  Replaced the member bytes in place (same length asserted), re-gzipped
  at compresslevel 9 (`gzip -9 -n`). Member-by-member comparison: ONLY
  `ramdisk-images/vendor.img` differs; all other members byte-identical;
  archive size unchanged; gzip round-trip SHA matches.
- NEW `t716-fix.cpio.gz` SHA
  `08eb665bb5860a5cd56ca92e0cfdddb0d4ee351f4265e6dcf755eb0bfb4414e3`
  (53,380,254 B; old was 53,502,851 B).
- DTB: old `linux,initrd-end` 0x113306383 = start + 53,502,851 exactly
  (T699 lesson confirmed). New `linux,initrd-end` 0x1132e849e = start +
  53,380,254; only 3 value bytes differ; size unchanged 2,049 B
  (binary-patched with a Python FDT parser — `fdtget`/`dtc` are not
  installed on the box; byte-diff confinement verified).
- NEW `t716-modified.dtb` SHA
  `cbb25094fffbb9f1bd010fde21868a8e2938a10d214a60051181ab123fa10e97`
  (2,049 B).

### 3.4 Transfers + installd trigger

All three artifacts transferred box → sandbox → Mac with SHA re-verified
at every hop; staged in worktree `guest-inputs/` under NEW names (T715
inputs never overwritten). `verify_inputs_t716.py` PASS on the Mac:
all 6 SHAs + sizes, DTB delta == CPIO size, embedded vendor member SHA ==
`9a2519b1…` (streaming parse, 238 members).

installd trigger UNTOUCHED and verified twice: system.img SHA
`1283cbf2…` re-verified at every hop, AND `installd.rc` dumped from the
staged image via Mac read-only debugfs = 4,958 B, SHA
`3ea3466bae5a3ff0ecabf4de117d9cc21388e6a813d205da357b473145b1ac94`,
`start installd` at line 109 (cited in smoke-ext L9319).

## 4. Smoke (Step 3a)

### 4.1 240 s smoke: INCONCLUSIVE (by design)

UART SHA `d224de9526f82c811fb624b19388aae150ca4d35ffc7b3d80f8b8720cd5ff161`
(13,664 lines, 1,433,470 B, guest max 217.0 s, host-cap stop 300.6 s,
exit 0). Structural all PASS; holds intact (subHal 0, sensorservice avc
0, kills 0, primeCache 0, installd-not-found 0, system_server 1,
SensorService onBootPhase return L13204, installd firing up L9685,
composer start cites `init.virt.rc:47` L9275, RAMDISK/panic 0). Composer:
EPERM 0, mapped 0, NO open attempt observed (0 `/dev/mem` lines) — the
first T715 attempt was at guest ~522 s, past this window. Per the brief:
ONE extended smoke.

### 4.2 Extended 600 s smoke: PASS

Evidence in `flights/smoke-ext/` (separate dir — the 240 s smoke evidence
is preserved, never overwritten; variance V1 below). UART SHA
`2246dca50f41c7d995b0ac04b86947336626adb1d15becd87a6a22e0c2264725`
(24,291 lines, 2,853,885 B, guest 605.3 s, guest-target stop, exit 0).

- EPERM lines: 0.
- L22814:
  `I/android.hardware.graphics.composer@2.1-service( 2415): simplefb:
  mapped 0x5f000000 1280x720 stride=1280 bpp=4` — the composer opened
  `/dev/mem` and mapped the framebuffer at the DTB address, guest ~573 s.
- avc `/dev/mem`: 6 lines (`{open}` + `{map}`, all `permissive=1`) — the
  `{map}` denial is new vs T715 (the open now succeeds, so execution
  reaches mmap). Persistent avc logging with a successful open is
  exactly the re-derived SELinux expectation (§2.2).
- Holds: subHal 0, sensorservice avc 0, kills 0, primeCache 0,
  installd-not-found 0, system_server 1, boot_completed 4 lines,
  installd trigger cited L9319 (`installd.rc:109`), SensorService
  onBootPhase return L13188 (7278 ms), Oops/panic/BUG/RAMDISK 0.
- queueBuffer 0 in this window (expected: mapped at 573 s, ~30 s of
  window left; present dispatch is flight-01's criterion).

The fix is PROVEN in the guest UART (GUEST-CONSUMES-ARTIFACT): the guest
consumed the patched vendor.img and the mapping landed.

## 5. Flight-01 + control (Step 3b)

### 5.1 Flight-01

UART SHA `b178e4c67ebeb335ae902363a97895366abd024cbc439660dcf60265a8a9c0c9`
(219,931 lines, 28,842,665 B; started 2026-11-15T14:28:56Z, ended
2026-11-15T15:08:59Z; host 2403.52 s, guest 2403.0 s, guest-target stop,
qemu exit 0). All counts below are grep numbers on the final file,
produced by `scripts/analyze_t716_flight01.py` (byte reads, split on
`\n` only) and cross-checked with direct grep.

PASS-criterion counts:
- queueBuffer: 0 (criterion > 0 — FAILED).
- presentDisplay: 0; present dispatch lines: 0 (criterion: first present
  dispatch — FAILED). No guest-origin present evidence of any kind.
- Dexopt for com.android.launcher3: L13593 status=PERFORMED; Launcher3
  dexopt SKIPPED lines: 0 (criterion PASSED).
- Launcher3 FATAL EXCEPTION: 2. L24729 DeadSystemException collateral
  (see below); L81124 genuine `Couldn't removeRegionSamplingListener`
  (see below). Criterion (0, or all proven GateKeeper-collateral with
  genuine composer-path FATALs 0) — FAILED on L81124.
- EPERM lines: 0. `simplefb: mapped` lines: 7 (L22650, 71717, 103247,
  131284, 160506, 183149, 202127 — all `0x5f000000 1280x720
  stride=1280 bpp=4`, pid 2417). The composer fix holds for the full
  window.

Holds (all intact):
- Watchdog KILLING: 0. `installd not found`: 0. fixAppsData lines: 0
  (healthy per T715). FutureTask.get: 0. Failed to load subHal config
  file: 0. `output buffer not gpu writeable`: 0. sensorservice avc
  denials: 0. Kernel Oops/panic: 0. RAMDISK incomplete write: 0.
- installd STARTED lines: 4 (L9368, 9390, 9391, 9718); 707 installd
  lines total.
- SensorService onBootPhase RETURN lines: 8 (L13365, 27442, 58881,
  95403, 125529, 150380, 175965, 196300). onBootPhase lines total: 355.
- `sys.boot_completed` lines: 29 (boot_completed reached in every
  system_server lifetime that lived long enough).
- avc `denied` (two spaces) total: 8,271; for `/dev/mem`: 33 (all
  `permissive=1`, `{open}` + `{map}` — expected logging, §2.2).
- SurfaceFlinger ready L10578; display device added 1280x720 L12061;
  display state ON L14846/L14851. Launcher3 lines: 945 (T715: 3,143 —
  observation, no inference drawn).

system_server fork state (reported explicitly either way, per the
brief): 9 `system_server_start(` lines (L11082, 25382, 56258, 92784,
122990, 147736, 173312, 193619, 218073) and 9 `Entered the Android
system server!` lines — 9 lifetimes (T715: 6), 0 by the Watchdog. Fork
sequence numbers in the start lines run [1..9]. The 9th lifetime started
at L218073, near the end of the window; 8 onBootPhase returns cover
lifetimes 1–8.

FATAL classification (7 total):
- 6 × DeadSystemException collateral, all in the first death cluster
  23:25:24–25 (L24718 permissioncontroller, L24725 documentsui, L24729
  launcher3, L24761 systemui, L24765 pid 3904, L24775 systemui/SysUiBg).
  Each names `DeadSystemException: The system died`. The system_server
  death they follow is the GateKeeper NPE death (crash lines L23309–
  23310, see below) — collateral per the brief's rule.
- 1 × GENUINE composer-path: L81124, Launcher3 PID 5970,
  `java.lang.RuntimeException: Couldn't removeRegionSamplingListener`
  at `CompositionSamplingListener.nativeUnregister` →
  `RegionSamplingHelper.unregisterSamplingListener` — the exact T715
  genuine shape (T715: L46864/L72770). `removeRegionSamplingListener`
  lines total: 15 (T715: 5).

Parallel wall (NOT this lane's work): GateKeeper NPE persists —
L23309 `NullPointerException: Attempt to invoke interface method 'void
android.service.gatekeeper.IGateKeeperService.reportDeviceSetupComplete()'
on a null object reference` at L23310
`LockSettingsService.java:3394`. NullPointerException lines total: 6.
GateKeeper/LockSettingsService lines total: 125. Each system_server
death re-hitting setup completion is INFERENCE for deaths after the
first (only the first crash stack was read in full — same honesty rule
as T715).

### 5.2 First failing link (the 0x0 allocation wall)

The composer maps the framebuffer, but no buffer is ever allocated:

1. L10878 (guest ~104 s):
   `E/HWComposer( 2492): getAttribute: getDisplayAttribute failed for
   display 0: Unsupported (8)`. 8 occurrences in the flight (one per
   system_server lifetime that reached display setup). The log line does
   NOT name which attribute was refused — that is the next lane's first
   question.
2. L10891: `E/BufferQueueProducer( 2492): [FramebufferSurface]
   allocateBuffers: failed to allocate buffer (0 x 0, format 1, usage
   0x300)`. 42 `allocateBuffers` lines in the flight, ALL failures, ALL
   0x0 (34 × format 22 usage 0 — FallbackHome BLAST queues; 8 × format 1
   usage 0x300 — FramebufferSurface). No successful allocation is ever
   logged. The 0x0 failures persist across the whole window (first
   guest ~104 s, FallbackHome failures from guest ~513 s onward), so
   this is not a startup-ordering transient.
3. Consequence: queueBuffer 0, onFrameAvailable 0, acquireBuffer 0,
   validateDisplay 0, setClientTarget 0, createLayer 0 (all literal
   counts; the HAL-method strings' absence is consistent with the
   composer backend not logging its methods, so the allocation
   failures + queueBuffer 0 are the load-bearing evidence, not the
   method-name counts).
4. Downstream: 742 `E/Scheduler: trackPendingFrame: Invalid present
   fence` lines (first L22854, 0.2 s after the first mapping at L22650)
   — SurfaceFlinger tracks presents that never carry a valid fence —
   and the L81124 sampling FATAL.

INFERENCE (labelled): the 0x0 size suggests the display/layer size
never propagates to the allocator on the HWC path — plausibly because
the composer's getDisplayAttribute refusal leaves SurfaceFlinger
without display dimensions via HWComposer, even though the framework
DisplayDeviceRepository reports 1280x720 via display-config
enumeration (L12061). The alternative — the allocator (gralloc)
failing independently with 0x0 as a symptom of an unset layer size —
is NOT excluded by this UART. The next lane must distinguish these
from the T681 backend source + the allocator path. What is FACT:
getDisplayAttribute fails, every allocation fails at 0x0, no frame
ever flows, and the fence errors + sampling FATAL follow.

Pre-existing, not a T716 regression: T715 flight-01 shows the SAME
shapes — 54 `allocateBuffers` failures, all 0x0 (49 × format 22, 5 ×
format 1); 5 × `getDisplayAttribute failed: Unsupported (8)`; 929
`Invalid present fence`; onFrameAvailable 0; validateDisplay 0.
T716's fix (capabilities-only, vendor.img only) cannot have caused
them, and T716's mapping (0 → 7) did not cure them. The 0x0 wall and
the EPERM wall are independent, parallel walls; T716 broke the second
one. The first one is the next lane.

Before/after (T715 → T716, flight-01):
- EPERM: 5 → 0. `simplefb: mapped`: 0 → 7.
- queueBuffer: 0 → 0. presentDisplay: 0 → 0.
- `Invalid present fence`: 929 → 742.
- `allocateBuffers` 0x0 failures: 54 → 42.
- getDisplayAttribute Unsupported: 5 → 8.
- Genuine `removeRegionSamplingListener` FATALs: 2 → 1.
- system_server lifetimes: 6 → 9. Watchdog kills: 0 → 0.
- Dexopt Launcher3: PERFORMED → PERFORMED.

### 5.3 Flight-ab control

UART SHA `ee9f6284e1ded3f46488674ee69f7a1b1cead659e9aefee7decf3a7c9226a74a`
(576 lines, 47,996 B; started 2026-11-15T15:10:07Z, ended
2026-11-15T15:10:32Z; host 25.03 s, guest 20.36 s; QEMU exited on its
own, returncode 0). Canonical kernel `05c43917…`. 0 system_server
starts, 0 installd, 0 queueBuffer, 0 EPERM, 0 mapped, 0 `/dev/mem`,
guest max 20.36 s, ends `reboot: Restarting system with command
'bootloader'`. Same early-exit shape as the T715 control (575 lines,
47,928 B, 19.28 s). Control pattern confirmed; no anomaly.

## 6. Variances from the brief

- V1: extended-smoke evidence in `flights/smoke-ext/` (new dir), not
  `flights/smoke/`, so both smokes' UARTs are preserved (evidence
  preservation). The 240 s smoke evidence stays in `flights/smoke/`.
- V2: vendor.img patch used rm-first-then-write instead of the T715
  write-first link-swap (0 free inodes — the T715 procedure cannot
  allocate). Same verification bar (e2fsck clean, dumps byte-identical,
  neighbours + ELF unchanged, vault untouched); inodes 305/727 reused
  exactly.
- V3: DTB patched with a hand-rolled Python FDT parser (byte-exact value
  rewrite, diff confinement verified) because `fdtget`/`fdtput`/`dtc`
  are not installed on the box.
- V4: launchers were `py_compile`-verified + diff-reviewed against the
  proven T715 launchers, NOT import-tested (importing a launcher forks
  and launches QEMU). Supervisor + all analysis scripts were truly
  import/run-tested, including behaviour tests against T715 UART copies
  with known expected counts (test copies deleted before launch).
- V5: `Invalid present fence` counts come from direct `grep -a -c`
  with the command recorded in progress (742 in flight-01; 929 in T715
  by the same command) — the shipped analysis scripts were frozen after
  behaviour testing and the fence count is a secondary diagnostic, not
  a criterion. All criterion counts come from the shipped scripts.
- No further variances.

## 7. Scripts

- `scripts/supervisor_t716.py` (import-tested), `launch_smoke_t716.py`,
  `launch_smoke_ext_t716.py`, `launch_flight_01_t716.py`,
  `launch_flight_ab_t716.py` (py_compile + diff-review),
  `verify_smoke_t716.py`, `analyze_t716_flight01.py`,
  `verify_flight_ab_t716.py` (behaviour-tested on T715 UART copies),
  `verify_inputs_t716.py` (run-tested on staged inputs). All read UARTs
  as bytes and split on `\n` only. Box-side build scripts:
  `patch_vendor.py`, `cpio_scan.py`, `cpio_replace.py`, `dtb_patch.py`
  in `/home/muse/t716-work/` (+ sandbox copies in
  `~/workspace/t716-scripts/`).

## 8. TEN-PASSES

NOT run (one flight + A/B control only, per the brief). No
reproducibility claim is made.

## 9. Next fix (exact) / follow-ups

1. T717 — the 0x0 buffer-allocation wall (this lane's first failing
   link). Diagnose, in order: (a) WHICH display attribute the T681
   simplefb composer backend refuses in getDisplayAttribute — read the
   backend source (the T681 custom vendor service; the ELF is preserved
   in vendor.img, SHA `8d62a642…`) and/or add logging; the UART line
   does not name the attribute. (b) WHY BufferQueueProducer receives
   0x0 dimensions — trace the display/layer size propagation from
   HWComposer through SurfaceFlinger to the allocator (gralloc), and
   distinguish "composer refuses the size query" from "allocator fails
   independently". (c) Smallest fix: implement the refused attribute in
   the composer backend (ship via the T708 ELF route) and/or repair the
   size propagation; prove it with a nonzero allocation + queueBuffer >
   0 + first present dispatch in the guest UART. Base: the T716 stack
   (mapping fix retained — do NOT regress the SYS_RAWIO capabilities).
   Oracle: this flight-01 UART, first failures L10878 (attribute) and
   L10891 (allocation).
2. GateKeeper stub/guard (parallel wall, separate lane — NOT folded
   into T717): system_server survives setup completion; verify with 1
   system_server lifetime past boot_completed. 9 lifetimes in T716 (6
   in T715); the churn is getting worse, not better.
3. Vectors 2 and 3 from T715 are NOT needed (carried: the pool
   unblocked and onBootPhase returns). Do not spend a lane on them.
