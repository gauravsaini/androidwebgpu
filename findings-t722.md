# T722: EGL native-fence support — findings

Track T722. Worktree /Users/Shared/wt-track722-egl-native-fence, branch t722,
base d8e90b0. Second recovery agent 0106e8ed (spawn 13:35:29 AEDT 16 Nov 2026,
wedge 16:35:29 AEDT). Prior agents: 0f3c3c2c (WAIT-DISK at Step 0),
81586262 (Steps 0-2, WAIT-DISK at Step 3). No commits by any lane agent;
certification is the driver's.

Verdict: FAIL

WALL-TYPE-TAG: EGL-DISPATCH-UNREACHABLE

## 1. Question asked

T721 proved the fence source (CONFIG_SW_SYNC kernel) and a real sw_sync retire
fence per present from the composer. T722 asks whether the SimpleFB EGL stub
can be given a working eglDupNativeFenceFDANDROID, so RenderEngine stops
logging `failed to dup EGL native fence sync` and the present path gains real
native-fence syncs at the EGL layer. The scaffold stays REVERTED throughout
(revert system 0ba6d6d3 is the flown config for every flight).

## 2. Pre-stated proof basis (recorded before flying; not moved)

- Normalization: dup-fails per 1000 s of guest kernel time. Window =
  flight-end.json guest_log_max_seconds, fallback = max kernel timestamp in
  the UART. Raw-count comparison across different window lengths is not proof.
- Baselines: T721 flight-01 10/2464.5 s = 4.06 per 1000 s; T721 revert
  4/600.6 s = 6.66 per 1000 s. PASS requires the T722 flight-01 rate BELOW
  BOTH baselines.
- Holds that must stay intact: T721 fence log line present, Invalid present
  fence 0, REJECT 0, EPERM 0, kills 0, sys.boot_completed present, simplefb
  mapped present.
- No new log line from the stub itself is possible: the stub has zero
  undefined symbols and no DT_NEEDED, so it cannot reach logcat. Proof is the
  dup-fail differential plus holds. Count with the literal pattern
  `failed to dup EGL native fence sync`; read UARTs as bytes, split on \n
  only. `avc: denied` (two spaces) is permissive-mode audit noise, not EPERM.

## 3. Characterization (Step 2, first recovery agent; carried, not redone)

- The SimpleFB EGL stub eglDupNativeFenceFDANDROID at 0x5cb0 returns -1
  unconditionally (`mov w0,#-1;ret`); eglGetError always returns 0x3000
  (EGL_SUCCESS). Two RenderEngine call sites in surfaceflinger (0x3f20b0,
  0x400b18) create a native-fence sync (eglCreateSyncKHR 0x3144, stub returns
  fake 0x5001), glFlush, then dup; fd == -1 logs the W line.
- Kernel sw_sync file is mode 0644; open+ioctl need no write mode or caps
  (source-verified at Step 2; re-verified from source post-flight, see S5),
  so uid-1000 surfaceflinger was judged able to mint signaled fences via an
  O_RDONLY open.
- No stub source exists anywhere searched (repo + composer tree + sandbox),
  so the route is a binary patch, T721 pattern. Cave: dead
  soft_egl_destroy_surface at 0x56dc, 244 B (0 refs, not in funcmap, not
  dlsym-reachable). Statics: .bss +16 B at 0x621eda8 (timeline fd + fence
  counter, zero-init). Patch site 0x5cb0: `b 0x56dc` (word 0x17fffe8b);
  restore word 0x12800000.
- The trampoline (235 B) opens /sys/kernel/debug/sync/sw_sync, mints a fence
  per call (CREATE_FENCE ioctl 0xC0285700, INC ioctl 0x40045701), and returns
  the fence fd, or -1 on any failure (old behaviour). Raw syscalls only; no
  logging possible.

Artifacts (SHAs recorded the moment built; box /home/muse/t722-work, flown
copies in guest-inputs/; all re-hashed by the second recovery agent, no
rebuild, all matched):

| Artifact | Size | SHA-256 |
|---|---|---|
| tramp_t722.bin | 235 B | 5299e5b57108ca7b594956cd355ef1d45143f2b82aea035ca8d1a53c6c5f6631 |
| soft_egl_destroy_surface-orig.bin | 244 B | afba2db68c7b392949baf8fcffc0c48ce5d1bf9b3ad0a995c0d6e2f99227a9d9 |
| libEGL_swiftshader-t722-patched.so | 52,416 B | 34b79da2823dbfe4aa168bc4d2e86c49f5efe3c43180ed981c62b95ce4eb21be |
| t722-vendor.img | 134,217,728 B | 5e0607ada51367fbe9f7e5dd7586afbeb9216f478f00a31baa5179eb73f476e0 |
| t722-fix.cpio.gz | 53,382,554 B | 42f1b2ec0127d4242a844b226df4fdcd505c318b923804de1c327875a0aa24cf |
| t722-modified.dtb | 2,049 B | 32eaab25c34fcce9e7c347ad600f215a9955e397c8ac63ef09f73b898ac1f7dc |

Patched .so: 219 diff bytes in 4 regions, disassembly-verified. Vendor image:
4 stubs replaced, e2fsck clean, composer f7600821 + shim 3daa5a49 + .rc
byte-identical, base untouched. CPIO: 238 members, ONLY the vendor member
differs, archive delta +0. DTB: initrd-end 0x1132e8d9a, 2 diff bytes.
EMBEDDED-VENDOR CHECKS PASS (both CPIOs, both machines): T722 CPIO member
ramdisk-images/vendor.img = 5e0607ad; T721 CPIO member = 6bf3e2e0.
Scripts: supervisor_t722.py + analyze_t722.py + 3 launch scripts, present and
import-tested; the analyzer was validated against the T721 flight-01 oracle
UART (166,172 lines, dup-fail 10, rate 4.058, all holds reproduced). QEMU
command line read before the first launch: -kernel Image, -dtb t722 DTB,
loader device for the CPIO, virtio-blk revert system drive only, NO vendor
drive (the vendor file is a SHA-consistency check only).

## 4. Flights (all times AEDT 16 Nov 2026)

### 4.1 Step 3a smoke — PASS

Launched 13:38:18 (supervisor PID 28771, QEMU PID 28776). Stop: host timeout
300 s. Host 300.53 s, guest 196.41 s, QEMU exit 0. UART SHA
2ef4a9184eccbcca68396aa0ca0a7d57fe07591c1a26ed44db31843f09ea9769, 13,732
lines, 1,453,239 B. Supervisor-verified inputs: kernel 70d3859d, DTB
32eaab25, CPIO 42f1b2ec, system 0ba6d6d3, vendor 5e0607ad.

REJECT 0, GBA failures 0, allocateBuffers failures 0, CALL allocate2 x2 (both
width=1280 height=720, L10402/L10409), T720_PROBE DONE PASS 1 / FAIL 0, EPERM
0, kills 0, FATAL 0, Oops 0, Invalid present fence 0, Launcher3 dexopt
PERFORMED (SKIPPED 0), display formed 1280x720, SimpleFB renderer loaded,
EGL_ANDROID_native_fence_sync advertised. T721 fence log 0 — expected (first
present lands ~565 s guest; prong deferred as in the T721 smoke).
sys.boot_completed 0 — expected (boot completes ~505 s guest; window cut at
the host cap). Dup-fail 0 in 196.4 s — vacuous (no presenting activity in the
window), explicitly NOT the EGL proof; flight-01 decides.

### 4.2 Step 3b flight-01 — flown, orphaned, manually stopped; PRIMARY CRITERION FAILED

Launched 13:44:22 (supervisor PID 29101, QEMU PID 29106; 2400 s guest target,
2500 s host cap). At host ~600 s the supervisor hit the disk gate ([FATAL]
Disk space below 8 GiB threshold: 7.49 GiB available, logged after the 585.6 s
tick) and exited(10): supervisor count 0, QEMU orphaned and still running, no
flight-end.json. Disk 7,857,404 KiB (7.49 GiB, 97%) at 13:55:17, after sliding
more than 1 GiB since flight start. Per the T721 V1 precedent the orphaned
QEMU was LEFT RUNNING to the guest target under manual monitoring (guest
684.1 s at 13:55:47, 743.9 s at 13:56:47, 1366.9 s at 14:07:10, 1974.3 s at
14:17:20; UART growing; QEMU CPU 38-100%). Manual SIGTERM at 14:25:25 at
guest_max 2461.281 s; QEMU gone 1 s later, no SIGKILL needed. A manual
flight-end record (flights/flight-01/flight-end-manual.json) carries the UART
SHA, the stop reason, and an honest note that the QEMU exit code was not
capturable; the numbered UART and .gz were deliberately NOT generated
(supervisor dead, disk-constrained). Final UART: 256,896 lines by iteration
(256,895 by wc -l; 1-line trailing-partial difference), 34,519,797 B, SHA
ae28c6a65aee386434b29ec2d926015956d821e3a2dcd1a2ff815787fa98f0ea.

Analyzer (scripts/analyze_t722.py flight-01 T722; full report in
flights/flight-01/analyze-report.txt):

- Dup-fail count 20, window 2461.3 s (max kernel timestamp fallback, no
  flight-end.json), RATE 8.126 per 1000 s. NOT DOWN vs the flight-01 baseline
  4.057 nor the revert baseline 6.661. THE PRIMARY CRITERION FAILED.
- All 20 lines are `W/RenderEngine( 2492): failed to dup EGL native fence
  sync: 0x3000`, all from pid 2492 (surfaceflinger/RenderEngine, one
  lifetime): L23145, 23343, 24010, 24040, 24998, 25085, 25249, 39851, 40570,
  41251, 41444, 41446, 41791, 51412, 59105, 66856, 97915, 99420, 99421, 99675.
  Count independently re-confirmed by grep (20, all pid 2492).
- The guest loaded the PATCHED file: `D/libEGL ( 2492): loaded
  /vendor/lib64/egl/libGLES_swiftshader.so` — one of the 4 stubs replaced in
  the vendor image (see S5).

Holds — ALL INTACT:

| Hold | flight-01 |
|---|---|
| T721 fence log present | 1 (L22863, `T721 fd=10 tl=9`, pid 2417) |
| T720_PROBE DONE PASS / FAIL | 1 / 0 |
| REJECT | 0 |
| CALL allocate2 | 205 |
| T637_MAPPER | 335 |
| GBA failures / allocateBuffers failures | 0 / 0 |
| EPERM | 0 |
| simplefb mapped | 1 |
| Watchdog kills | 0 |
| Invalid present fence | 0 (without the scaffold) |
| sys.boot_completed | 11 |
| Launcher3 dexopt | PERFORMED (SKIPPED 0) |
| Kernel Oops / panic | 0 |
| system_server starts / entered | 6 / 6 |
| onBootPhase RETURN | 4 |
| FallbackHome Displayed | 4 |
| composer-service E-lines | 0 |
| EGL information blocks / SimpleFB renderer | 1 (L10450) / 1 (L10462) |
| queueBuffer / presentDisplay / present dispatch | 0 (unobservable per T719 S3.5, not criteria) |

FATAL EXCEPTION 11 / DeadSystemException 10 / GateKeeper-LockSettingsService
88: the persistent GateKeeper NPE signature, same shape as T715-T721, a
separate named lane, NOT a T722 regression. All 11 FATAL contexts are
DeadSystem/DeadObject collateral in app processes (sdksetup, launcher3,
permissioncontroller, media, devicelockcontroller, systemui) after a
system_server death; no new crash class, no surfaceflinger crash. `avc:
denied` total 10,494 with 3 for /dev/mem, all permissive=1: permissive-mode
audit noise, not EPERM (EPERM 0).

### 4.3 Step 3c A/B control — NOT FLOWN (disk gate)

The control (T721 stack unchanged, 700 s guest / 800 s host) could not launch:
the supervisor exits(10) at launch below 8 GiB, and disk read 7,837,208 KiB
(7.47 GiB) at 14:25:26 AEDT. This control gap is named honestly. The verdict
stands without it: the T721 flights themselves supply the control baselines
(same stack, same QEMU, recorded rates), and the same-day control would only
have added variance control. The supervisor's disk gate was not bypassed, and
nothing was pruned (pruning is driver-only authority).

## 5. Investigation: why the fix did not take effect

Post-flight, evidence only, no rebuild. The guest consumed the patched vendor
bytes (embedded-member hash verified pre-flight, supervisor-verified CPIO
SHA, cmdline carrying the CPIO SHA), so the investigation tested every
mechanism that could leave the dup-fail rate unchanged.

### 5.1 Ruled out (each with evidence)

1. Wrong image / wrong vendor bytes. Embedded-member hash (T722 CPIO member
   = 5e0607ad), supervisor-verified input SHAs at launch, and the QEMU cmdline
   (loader device for the CPIO, no vendor drive) all confirm the guest flew
   the patched vendor. The T721 embedded-vendor lesson was applied and held.
2. Shadow stub copy. The UART names the loaded file:
   `D/libEGL ( 2492): loaded /vendor/lib64/egl/libGLES_swiftshader.so`, and
   the patch report (box t722_patch_report.json) lists
   /lib64/egl/libGLES_swiftshader.so among the 4 replaced stubs. The loaded
   file IS a patched copy. No fifth copy is in the load path.
3. Trampoline crash or fault. Surfaceflinger (pid 2492) stayed alive for the
   whole flight, logged 20 clean dup-fails, formed the display, and produced
   the EGL information block. A faulting trampoline (bad .bss address, bad
   branch target) would have killed RenderEngine on the first dup call.
4. Trampoline addressing or assembly defect. The assembled bytes were decoded
   directly from tramp_t722.bin on the box: adrp x9 word 0xb00310c9 targets
   page 0x621e000 with lo12 0xda8 (statics 0x621eda8, exact); adrp x1 word
   0x90000001 targets page 0x5000 with lo12 0x7a8 (path string 0x57a8,
   exact); the branch word 0x17fffe8b at 0x5cb0 lands exactly at 0x56dc; the
   path string bytes are present at the computed address; the .bss tail
   placement is exact (RW LOAD segment memsz end 0x621edb8 = statics + 16,
   NOBITS zero-init). All addressing is PC-relative, ASLR-safe. The T721
   base-0 adrp bug class is RULED OUT by the decoded bytes.
5. Kernel mode, capability, or uid barrier. Kernel source on the box
   (drivers/dma-buf/sw_sync.c, sync_debug.c, tree HEAD 4da740c10dc0):
   sw_sync_debugfs_open creates a timeline with NO mode, capability, or uid
   check; the CREATE_FENCE and INC ioctls have NO checks; the debugfs file is
   mode 0644. An O_RDONLY open passes the VFS permission check for uid 1000.
   The O_RDONLY-vs-O_RDWR difference is NOT the cause: the T720 probe and the
   T721 libsync helper open O_RDWR because they run as root; O_RDWR on a
   root-owned 0644 file would FAIL the VFS check for uid 1000. T722's
   O_RDONLY choice was the right one for surfaceflinger.
6. SELinux enforcement. The guest runs permissive (cmdline
   androidboot.selinux=permissive). The composer's ioctls on the same file
   generate permissive=1 audit lines AND succeed concurrently (T721 fence log
   L22863, fd=10 tl=9). Permissive denials do not block.
7. Mount-namespace invisibility. The surfaceflinger service block in the
   revert system image (0ba6d6d3, read-only Linux debugfs) runs `user
   system` with NO mount_namespace directive, and a sweep of ALL
   /etc/init/*.rc files found no mount_namespace anywhere in this build.
   Surfaceflinger shares init's mount namespace; the debugfs mount is visible
   to it in principle.
8. Mount absent. The .rc `on post-fs-data` trigger mounts debugfs at
   /sys/kernel/debug; the composer uses that mount concurrently (ioctl audit
   lines 0x5700/0x5701 at the same wall time as the dup-fails), and the
   wifi.rc mkdir under /sys/kernel/debug succeeded at guest ~497 s.

### 5.2 Surviving explanation: the patched stub is never invoked

By elimination, the remaining mechanism is dispatch: RenderEngine's
eglDupNativeFenceFDANDROID call, routed through the system libEGL.so (the EGL
information block reports "1.4 Android META-EGL"), returns -1 WITHOUT reaching
the patched driver stub at 0x5cb0 — either META-EGL does not forward the call
to the driver function, or the call resolves to a different implementation.
This fits every observation: zero audit traces from surfaceflinger on sw_sync
(2,242 sw_sync lines in the UART, 0 containing pid 2492 — no open and no
ioctl was ever attempted by surfaceflinger), no crash, behaviour identical to
the unpatched stub, and all holds intact. The characterization verified the
stub FUNCTION (returns -1) and the call SITES (log on -1) but never verified
the DISPATCH LINK between them. That unverified link is the wall.

The successor's discriminating probes, in order: (a) disassemble the system
libEGL.so eglDupNativeFenceFDANDROID wrapper and its driver function-table
population to see whether the driver stub is reachable; (b) a guest probe
that dlopens the driver .so directly and calls eglDupNativeFenceFDANDROID as
uid 1000, proving the patched function works when called; (c) a syscall trace
of surfaceflinger across a present, proving whether any sw_sync open is
attempted. The dispatch question must be answered BEFORE any rebuild: do not
rebuild the same patch expecting a different result.

### 5.3 The rate increase is non-effect, not regression

8.126 per 1000 s is above both baselines, so the pre-stated criterion failed
and the verdict is FAIL. But nothing regressed: no new failure class, no
crash, all holds intact, the fence path repeated (T721 fd=10 tl=9). The
higher rate is consistent with zero fix effect plus workload variance in
present activity (the present-activity denominator is unobservable per T719
S3.5). The patch is INEFFECTIVE, not harmful.

## 6. Successor direction, fix disposition, Rust line

- Successor first step: answer the dispatch question (S5.2 probes). If
  dispatch is the wall, patch the DISPATCH point (META-EGL wrapper / function
  table) or the RenderEngine call sites in surfaceflinger — not the stub
  again. Chain direction after the EGL wall is understood remains display
  completion / frame commit. The GateKeeper NPE stays a separate named lane.
- Fix disposition: the binary patch is verified-correct to the byte and safe
  (no crash, no regression), but ineffective in the guest. The patched
  vendor/CPIO/DTB artifacts are preserved with the SHAs in S3 for the
  successor to reuse or supersede. The restore word for the patch site
  (0x12800000) and the original cave bytes (244 B, afba2db6) are recorded.
- Rust par bhi lagu: the Rust emulator must verify the EGL dispatch path end
  to end — a correct fence primitive behind an entry point the loader never
  calls changes nothing; the Rust EGL layer needs a positive-control probe
  proving each fence entry point is actually reached, not only that the
  primitive works in isolation.

## 7. Reflection (REFLECT-ON-EVERY-PROBLEM)

- What happened (evidence): the lane built a byte-correct patch against a
  characterization that assumed META-EGL forwards eglDup to the driver stub.
  Flight-01 proved zero effect (rate 8.126 vs baselines 4.057 and 6.661, all
  holds intact). Post-flight investigation eliminated every
  implementation-level cause (image, shadow copy, addressing, kernel
  barriers, SELinux, namespace, mount) and left dispatch non-forwarding as
  the surviving explanation.
- Root cause: the characterization verified the two ENDS (stub function,
  call sites) but never the LINK (dispatch). The proof basis (dup-fail
  differential) assumed the link, so a correct patch against an unreached
  function produced a clean, confident, wrong-shaped lane: everything
  verified except the one thing that mattered.
- Missed signal: the EGL information block in every flight's UART reports
  "1.4 Android META-EGL" — the loader layer sat in plain sight from T721 on,
  and the stub's zero-undefined-symbols property (recorded as "no new log
  line is possible") was treated as a proof-basis caveat rather than a prompt
  to verify dispatch. The T721 adrp lesson WAS checked (and passed); the
  analogous "is the patched code even CALLED" check had no gate.
- What changes: future binary-patch lanes must verify CALL-REACHABILITY
  before building — prove the patched function is invoked in the guest
  (dispatch trace, direct-call probe, or a guest-visible marker), with a
  positive control distinct from the absence-of-failure signal. A patch
  whose target is never called is not a fix, however correct its bytes.
- Where it lives: this section; the successor brief must carry the
  reachability gate as a Step 2 exit criterion; the WALLS.md T722 section
  (driver) records the wall; the standing evidentiary rules should gain a
  reachability rule (proposed here, driver disposes).

## 8. Evidence index

- flights/smoke/uart.log — SHA 2ef4a9184eccbcca68396aa0ca0a7d57fe07591c1a26ed44db31843f09ea9769, 13,732 lines, 1,453,239 B.
- flights/smoke/flight-end.json — supervisor record (host 300.53 s, guest 196.41 s, QEMU exit 0).
- flights/flight-01/uart.log — SHA ae28c6a65aee386434b29ec2d926015956d821e3a2dcd1a2ff815787fa98f0ea, 256,896 lines (iteration), 34,519,797 B.
- flights/flight-01/flight-end-manual.json — manual record (started 2026-11-16T02:44:22Z, SIGTERM at guest 2461.281 s, exit code not capturable).
- flights/flight-01/analyze-report.txt — full analyzer output (all counts in S4.2; binary bytes present, use grep -a).
- flights/flight-01/preflight.json + qemu-command.txt — supervisor-verified inputs + QEMU command line.
- progress-t722.txt — full lane record (both WAIT-DISK exits, Step 2 characterization, smoke PASS, orphan event + checks, adjudication, investigation).
- guest-inputs/ — flown artifacts (SHAs in S3).
- scripts/ — supervisor_t722.py, analyze_t722.py, cpio_member_hash.py, 3 launch scripts.
- Box sources read (not in the worktree): /home/muse/t722-work/tramp_t722.S + t722_patch_report.json + patch_vendor_t722.py; /home/muse/t721-work/tramp.S + composer.rc; /home/muse/t720-work/t720-probe.c; kernel drivers/dma-buf/sw_sync.c + sync_debug.c; vault t721-composer.rc + t721-tramp.bin; revert system image t721-system-revert.img (read-only debugfs).
