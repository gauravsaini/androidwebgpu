# Findings — Track T720: kernel fence source

Verdict: PASS

## Summary

T720 set out to break the NO-FENCE-SOURCE wall named by T719: the
guest kernel had no `CONFIG_SW_SYNC`, so no fence-creation mechanism
existed at all, and the composer backend's hardcoded
`setPresentFence(-1)` could never be replaced by a real retire fence.
Step 1 provenance ESTABLISHED the exact tree + config that produced
the flown Image `53d9563e` (box kernel tree HEAD `4da740c10dc0` plus
the uncommitted Route B lds patch; tree Image byte-identical to the
flown Image), so no unchanged-rebuild control was needed. Step 2
rebuilt the kernel with ONLY the fence-source change
(`CONFIG_SW_SYNC=y` + forced `CONFIG_DEBUG_FS=y` + default-y children):
new Image `70d3859d`, all pre-flight gates PASS.

A guest-side probe (raw AArch64 syscalls, no libc) PROVED the fence
source live in the guest: debugfs timeline open (fd 4), fence
creation with a valid fd (fd 5, not -1), timeline increment,
poll-signaled state, and FILE_INFO reporting 1 fence with
driver `sw_sync` — `T720_PROBE DONE status=PASS` in the smoke
(L238 area) and in flight-01 (L229). The A/B control on the
canonical kernel shows the exact differential the brief predicted:
debugfs mount fails (ENODEV), timeline open fails, probe FAILs —
the fence source comes from the T720 kernel build, not from
userspace or the CPIO.

Flight-01 (guest 2,403.3 s, 320,917 lines) proves every hold intact:
REJECT 0, 1,023 allocate2 CALLs, 0 GraphicBufferAllocator failures,
0 allocateBuffers failures, EPERM 0, simplefb mapped 1, 0 Watchdog
kills, Dexopt PERFORMED for com.android.launcher3, `sys.boot_completed`
present 10 times, Invalid present fence 0 (scaffold behaviour
unchanged), 0 kernel Oops/panic. The T719 wall is BROKEN and PROVEN.
The successor program — a real sw_sync retire fence in the composer
backend plus EGL native-fence support — is named in §9. The scaffold
patch STAYS until the real retire fence lands.

## WALL-TYPE-TAG: NO-FENCE-SOURCE

Rust par bhi lagu: the Rust emulator must provide a fence source
equivalent to the sw_sync timeline/fence interface proven here
(timeline open, fence creation with a valid fd, increment, signal
state) or SurfaceFlinger will hit the same -1 retire-fence path
there; note the 4.19 interface is debugfs-only (no /dev/sw_sync
misc device), so the Rust implementation must decide which
interface it exposes. The scaffold patch is guest-binary-specific
and does not transfer to the Rust emulator.

## 1. Step 0 — Mac disk gate: PASS

Recorded 2026-11-16 08:34:54 AEDT: Mac data volume 228 Gi, 10 Gi
available, 95% used — above the 8 GiB gate.

## 2. Step 1 — Input verification: PASS

Every input SHA re-hashed and matched before use, no network fetch.
T719 findings `290114ae` (17,943 B), T719 death note `35c3476f`
(3,883 B), T719 smoke UART `bfd345b8` (13,509 lines), T719
flight-01 UART `65c4cd9a` (309,407 lines), T719 flight-ab UART
`fbbc1b09` (575 lines), `t719-system.img ac15a68a`
(2,059,816,960 B), `t718-vendor.img bac3cdc1`,
`t718-fix.cpio.gz 1150c841`, `t718-modified.dtb d298f5ad`,
`t705-Image 53d9563e` (20,126,208 B), `extracted_kernel.config
d840310a` (133,733 B), T718 findings `da5fc919`, T718 death note
`62d00876`, box binaries `3daa5a49` / `bf8661d0` / `745d19d0` /
`fadee423` / `259d39f5`, all five vault lane-images match.
T719 findings read in full (376 lines).

### 2.1 Provenance ESTABLISHED (no unchanged-rebuild control needed)

Box kernel tree `/home/muse/t686-kernel/src`, HEAD `4da740c10dc0`
("Revert ANDROID: fscrypt: add key removal notifier chain"), with
ONE uncommitted modification: `include/asm-generic/vmlinux.lds.h`
(T688/T696 Route B globs: `.text.unlikely.*`, `.text.unknown.*`,
`.ref.text.*`; 4 insertions, 2 deletions). The dirty-tree patch is
part of the build state — preserved, not reverted. Tree
`arch/arm64/boot/Image` SHA = `53d9563e...` — byte-identical to
the flown Image. Tree `.config` (135,357 B, SHA `dc00bffc`),
System.map, vmlinux all Nov 9 00:44. Build command recorded:
`/home/muse/t705-build/build.sh` (NDK r29 clang 21.0.0,
`CROSS_COMPILE` from `/home/muse/t686-kernel/toolchain-bin`,
`KCFLAGS="-fno-builtin-sprintf"`, `make -j5 Image`); toolchain
verified present (clang 21.0.0, Android 13989888).

Baseline config delta (pre-existing, T686/T696/T705 lineage):
`src/.config` vs `extracted_kernel.config` = 99 changed lines
(clang 11→21, LTO off with THINLTO/CFI/SCS removed,
VIRTIO_BLK=y added, etc.), saved as
`/home/muse/t720-work/baseline-config-diff.txt` (128 lines). The
T720 config gate is therefore applied as new `.config` vs
`src/.config` (the actual build base), with new-vs-extracted =
baseline delta + fence change. This interpretation is stated
here so the driver adjudicates the gate correctly.

Baseline gates on the flown Image: five symbols present
(`_stext ffffff8008081000`, `_etext ffffff8008b50000`,
`__initcall_start ffffff8009250b48 < __initcall_end
ffffff8009251664`, `__bss_start`, `_end`); 0 suffixed initcall
sections; `CONFIG_SYNC_FILE=y`, `CONFIG_DMA_SHARED_BUFFER=y`
re-verified; `CONFIG_SW_SYNC` absent.

### 2.2 Config change (in-lane catch recorded)

`scripts/config --enable CONFIG_DEBUG_FS --enable CONFIG_SW_SYNC`,
then `olddefconfig`. The FIRST attempt, run without `LD=ld.lld`,
silently dropped `CONFIG_LD_IS_LLD`, `CONFIG_TOOLS_SUPPORT_RELR`,
`CONFIG_RELR` — caught from the diff, pristine `.config`
restored, redone with the FULL build environment; all toolchain
symbols survive. Final diff
(`/home/muse/t720-work/config-diff-t720.txt`, 32 lines): 1 removed
line (`# CONFIG_DEBUG_FS is not set`) + 15 added lines: 4
functional (`CONFIG_DEBUG_FS=y` forced dep, `CONFIG_SW_SYNC=y`,
default-y children `CONFIG_BLK_DEBUG_FS=y`,
`CONFIG_BT_DEBUGFS=y`) + 11 `#... is not set` comment lines
(newly-visible symbols resolved to default n by olddefconfig).
Zero other changes. New `.config` SHA `6c162ecf...`, saved as
`/home/muse/t720-work/dotconfig-t720.txt`. Pristine snapshot in
`/home/muse/t720-work/pristine/` (Image `53d9563e`, System.map
`a41d08c4`, `.config dc00bffc`), SHA-verified before building.

### 2.3 Interface correction — 4.19 sw_sync is debugfs-only

Read from the tree source (`drivers/dma-buf/sw_sync.c` header +
`drivers/dma-buf/sync_debug.c:195-205`): the timeline is created
by opening `<debugfs>/sync/sw_sync` (created by
`sync_debugfs_init`, `late_initcall`, fops
`sw_sync_debugfs_fops`). NO `/dev/sw_sync` misc device exists in
this tree (no `misc_register`/`device_create` in `sw_sync.c`).
The brief's "/dev/sw_sync" phrasing reflects the modern
misc-device interface, not this 4.19 tree. The observable proxy
is therefore: debugfs timeline open + fence creation + signal
state readable. The probe checks `/dev/sw_sync` empirically and
logs the expected ENOENT (rc=-2 in every flight). `SW_SYNC`
Kconfig (`drivers/dma-buf/Kconfig:19`): `depends on SYNC_FILE` +
`depends on DEBUG_FS` — hence the forced `CONFIG_DEBUG_FS=y`.

Ioctl facts verified from `include/uapi/linux/sync_file.h`:
`SYNC_IOC_MAGIC` is `'>'` (0x3E), NOT `'F'`;
`SW_SYNC_IOC_MAGIC` is `'W'`; `sw_sync_create_fence_data` = 40 B;
`sync_file_info` = 56 B (not 48 — caught before building; the
48 B value would have produced a non-matching ioctl number and
`-ENOTTY`); `sync_fence_info` = 80 B. Ioctl numbers,
compile-asserted: CREATE_FENCE `0xC0285700`, INC `0x40045701`,
FILE_INFO `0xC0383E04`. INC takes a pointer to u32.

## 3. Step 2 — Kernel build + pre-flight gates: PASS

Build PID 1381194, 08:39:46–08:42:47 AEDT, `BUILD_EXIT=0`, log
`/home/muse/t720-work/build.log`, script
`/home/muse/t720-work/build.sh` (exact T705 command shape).
New Image SHA
`70d3859d6aed1906d27217aa3fdfd9bb953a57f4490c89c51efa4f6490441e04`,
20,396,544 B; System.map `ba71da0a`. Recorded in
`progress-t720.txt` the moment it was built.

Gates: (i) config diff as in §2.2, every line named; (ii) five
symbols present (`_stext ffffff8008081000`, `_etext
ffffff8008b70000`, `__initcall_start ffffff8009292b48 <
__initcall_end ffffff800929369c`, `__bss_start ffffff80093f3a00`,
`_end ffffff8009470000`); (iii) 0 suffixed initcall sections,
initcall range inside `.init.data` (`ffffff8009283000` +
`0x108b8`); (iv) sw_sync symbols present (`sw_sync_ioctl`,
`sw_sync_debugfs_open/release`, `sw_sync_debugfs_fops`,
`sync_timeline_debug_add/remove`; 4 `sw_sync` lines; ABSENT in
the baseline System.map; 209 `debugfs` lines); (v) Image SHA +
size recorded.

## 4. Guest probe — three versions, two harness bugs, bisection

Source `t720-probe.c` (sandbox `~/workspace/t720-drafts/`): raw
AArch64 syscalls only, `-nostdlib`, static, no libc. Opens
`/dev/kmsg` (mknods char 1:11 if missing) + stdout; mounts
debugfs; opens the timeline; CREATE_FENCE value 1 name
`t720probe`; INC by 1; `ppoll` on the fence fd; FILE_INFO
two-call pattern; single-line `T720_PROBE DONE` summary. Always
exits 0, never blocks boot. The probe was NOT host-executed: no
qemu-user exists on the box (only `qemu-system-aarch64`); first
execution was the guest smoke. Recorded honestly.

### 4.1 v1 — sysfs mount aborts the boot (smoke attempt 1)

v1 (`015dc39b`, 7,568 B) mounted sysfs on `/sys`. The probe
itself PASSED (timeline fd 5, fence fd 6, poll signaled,
num_fences 1), but Android init's first-stage
`mount("sysfs", "/sys")` then failed EBUSY at guest 9.54 s and
aborted (`InitFatalReboot`): QEMU exit 0 at guest 9.6 s, 599
lines, UART `58534665`, archived `flights/smoke-attempt-1/`
(flight-end `d4a5e264`). Cause = probe bug, NOT kernel.

### 4.2 v2 — persistent debugfs mount kills switch_root (attempt 2)

v2 (`d25aee60`, 7,104 B) mounted debugfs on a private `/t720dbg`
and never touched `/sys`. The probe PASSED again (fd 4/5), the
boot reached switch_root (guest 9.93 s), then a SILENT reboot to
bootloader: QEMU exit 0 at guest 10.0 s, 560 lines, UART
`fc5731e7`, archived `flights/smoke-attempt-2/` (flight-end
`2a573f3a`). No panic, no init error.

Bisection (both healthy): A = new kernel + OLD CPIO/DTB
(`smoke-kernelonly`): guest 192.7 s, 13,640 lines, UART
`ee250495`, REJECT 0 CALL 2 GBA 0 EPERM 0 kills 0 — the new
kernel is FINE. B = OLD kernel + NEW CPIO/DTB v2
(`smoke-cpioonly`): guest 188.1 s, 13,825 lines, UART
`43d659ee`, REJECT 0 CALL 2, probe FAIL 1 (timeline open,
expected) — the new CPIO/DTB is FINE alone. A+B dies: the ONLY
differential persistent state is the debugfs mount (it succeeds
only on the new kernel; on the old kernel the mount fails
ENODEV and the probe exits clean). The `/dev/kmsg` mknod is
exonerated (present in healthy flight B).

### 4.3 v3 — zero persistent state (attempt 3, flown)

v3 (`61140fc7`, 7,440 B): close fds, `umount2(MNT_DETACH)` +
rmdir `/t720dbg` before exit, on ALL paths (including the FAIL
paths). The smoke PASSED (§5) and flight-01 confirms
`umount debugfs rc=0` (L237) + `rmdir /t720dbg rc=0` (L238).
The fix confirms the diagnosis: the persistent debugfs mount
was the attempt-2 killer.

Fence-status semantics correction: the v1 annotation
"(0=signaled, 1=active)" was INVERTED. `dma_fence_get_status_locked`
(`include/linux/dma-fence.h:475-477`) returns 0 if not signaled,
1 if signaled without error. The observed `fence status=1` IS
the signaled confirmation, consistent with ppoll POLLIN. The
v2/v3 annotation reads "(1=signaled, 0=active, <0=error)".

## 5. CPIO + DTB (T699/T716/T717/T718 procedure, adapted)

`cpio_replace_t720.py`: parses the newc stream, emits unchanged
members byte-verbatim, replaces `early-ramdisk` keeping its
header except the filesize field, inserts `t720-probe` before
`TRAILER!!!`, re-walks proving only intended diffs,
`gzip -9 -n`. The early-ramdisk gains exactly 2 hook lines after
the `[T552]` echo line (anchor count asserted == 1), launching
the probe backgrounded with `>/dev/console 2>&1 &` (the
established pattern). `early-ramdisk` `1a495ba6` (12,565 B) →
`57e8d0e2` (12,712 B). Member-by-member comparison PASS on all
three builds; shadow check PASS (no `t628` member).

Flown (v3): CPIO.gz SHA
`848a470fa1241ba05e412c70158407f1b0dbdc593b29d1ce0dc16596d5c9657a`,
53,384,319 B (uncompressed 725,750,104 B + probe). DTB SHA
`3ff2bb8ec704d6df555c333ef13ce157d02e7d5b7e872768a1f21776ad90a9b2`,
2,049 B: initrd-start `0x110000000` unchanged, initrd-end
`0x1132e947f` = start + 53,384,319, 2 diff bytes inside the
8-byte value. Superseded: v1 CPIO `d623d48c` / DTB `92c6fab8`,
v2 CPIO `e37ec1c7` / DTB `b1594857` — preserved on the box
(`t720-fix-v1/v2.cpio.gz`, `t720-modified-v1/v2.dtb`,
`t720-probe-v1/v2`).

All transfers box → sandbox → Mac used `--partial` + SHA
verification at every hop. The supervisor pins the flown SHAs
(kernel `70d3859d`, CPIO `848a470f`, DTB `3ff2bb8e`,
system.img `ac15a68a` unchanged, vendor `bac3cdc1` unchanged).

## 6. Step 3a — Smoke: PASS (attempt 3)

Attempt 3 launched 09:19:06 AEDT, guest target reached
(260.1 s), QEMU exit 0, 13,683 lines, UART
`67f92b81a116129de6598f04b94328de00795ced425bbc7172268c80ad596713`.
REJECT 0, CALL allocate2 2, GBA failures 0, allocateBuffers
failures 0, EPERM 0, Watchdog kills 0, subHal 0, sensorservice
avc 0, prime-cache 0, system_server 1. Probe: 24 lines,
timeline fd=4, fence fd=5, status=1 signaled, driver sw_sync,
DONE PASS 1 FAIL 0, umount rc=0. `simplefb: mapped` adjudicated
from flight-01 (0 in the smoke, as in T719).

Attempts 1–2 are preserved in `flights/smoke-attempt-1/` and
`flights/smoke-attempt-2/` (evidence-preservation rule: no
flight folder was overwritten in place). The bisection flights
`smoke-kernelonly` and `smoke-cpioonly` are preserved as flown.

## 7. Step 3b — Flight-01: PASS

Launched 09:25:00 AEDT, guest target reached (2,403.3 s), QEMU
exit 0, 320,917 lines, UART
`f1ee6d02f4ac503c9f64d50c5d2f58f605bb60da242a6166f800ae06617f33de`.
Analysis: `flights/flight-01/analysis.txt` (296 lines).

### 7.1 Observable proxy — PROVEN (the T719 wall, broken)

| Link | Evidence (flight-01 UART line) |
|---|---|
| `/dev/sw_sync` absent (4.19 debugfs-only) | L198: open rc=-2 |
| debugfs mount | L204: rc=0 |
| timeline open | L207: fd=4 |
| fence creation | L210: CREATE_FENCE rc=0; L211: fence fd=5 (not -1) |
| timeline increment | L212: INC rc=0 |
| signal state via poll | L216: ppoll rc=1; L217: revents=1 (POLLIN) |
| FILE_INFO | L218–L220: call1 rc=0, num_fences=1, call2 rc=0 |
| fence identity/state | L221: status=1 (signaled); L222: driver_name=sw_sync; L223: obj_name=t720-probe |
| summary | L229: `T720_PROBE DONE status=PASS timeline_fd=4 fence_fd=5 poll_rc=1 poll_revents=1 fileinfo_rc=0 fence_status=1 num_fences=1` |
| zero persistent state | L237: umount rc=0; L238: rmdir rc=0 |

DONE PASS 1, DONE FAIL 0, probe lines 24. The fence source is
proven live in the guest: a timeline opens, a fence is created
with a valid fd, the timeline increments, and the fence's
signal state is readable through two independent paths (poll +
FILE_INFO).

### 7.2 Allocation holds — intact

REJECT allocate2 0; CALL allocate2 1,023 (first L10360,
width=1280 height=720); T628 REGISTERED 1 (L10348);
getIMapperLibrarySuffix CALL 105; T637_MAPPER 1,633;
GraphicBufferAllocator failures 0; allocateBuffers failures 0.
Display formed: DisplayDeviceInfo 4 lines (1280x720, 60 fps),
first L11863. `getDisplayAttribute failed` 1 line (L10693) —
TOLERATED per T717 Step 2, report only. queueBuffer 0,
presentDisplay 0, present dispatch 0 — expected per T719
findings §3.5 (unobservable on success paths); the literal
queueBuffer>0 criterion was REPLACED by the observable proxy
in this brief.

### 7.3 Boot + system holds — intact

`sys.boot_completed` present 10 times (first L20369; also
L32641, L32739 in the second system_server lifetime).
system_server: 2 starts (L10909, L25478), 2 entered, 2
SensorService onBootPhase RETURNs (L13180, L27525),
FutureTask.get 0. installd STARTED 1 (L9754), installd
not-found 0, fixAppsData 0. Dexopt: Launcher3 PERFORMED
(L13407), Launcher3 SKIPPED 0 (92 skipped lines are other
packages). EPERM 0; simplefb mapped 1 (L22697); /dev/mem
avc-denial lines 6 — the permissive-mode class ruled out in
T716 (EPERM 0 holds). Watchdog KILLING 0. Kernel Oops/panic 0.
subHal config errors 0; `output buffer not gpu writeable` 0;
sensorservice avc denials 0. FallbackHome Displayed 2
(L19070, L32929), same as T718.

One system_server death occurred at guest ~527 s (native
crash, pid 3737 tombstone at L24102–L24104; DeadSystemException
cascade L24125–L24323; FATAL EXCEPTION 4, all consequences of
the death). The GateKeeper NPE parallel wall persists
(L22916: `IGateKeeperService.reportDeviceSetupComplete()` on a
null object reference — present since T715, separate named
lane). Lifetime 2 survived to guest 2,403 s with
`sys.boot_completed` present. This matches T719's shape
exactly (1 death, lifetime 2 survived). No new crash class
appeared; the death is not a T720 regression.

### 7.4 Scaffold behaviour — unchanged

Invalid present fence 0 (T718 flight-01: 2,241; T719: 0;
T720: 0). The scaffold patch (system.img `ac15a68a`, flown
unchanged) continues to silence the log line. Scheduler(2488)
lines 0, RenderEngine(2488) lines 0, composer-service E-lines 0.
RenderEngine EGL dup-fail 233 (first L22981; T718: 164,
T719: 178) — same class, count scales with frame activity
(CALL 1,023 vs T719's 571); the composer still hardcodes
setPresentFence(-1), so EGL still cannot dup a native fence.
The retire-fence lane (§9) is still owed; this is the expected
residual, not a regression.

## 8. A/B control — differential PROVEN

Flight-ab (canonical Image `05c43917`, same v3 CPIO/DTB):
launched 10:07:53 AEDT, QEMU self-exit 0 at guest 20.1 s,
595 lines, UART `ff06cba6`. REJECT 0, EPERM 0, mapped 0.
Probe lines 13: mount debugfs rc=-19 (ENODEV — no
CONFIG_DEBUG_FS), timeline open fd=-2, `T720_PROBE DONE
status=FAIL stage=timeline_open rc=-2` (L209), umount rc=-22
(nothing mounted, expected), rmdir rc=0. The probe FAILs on
the canonical kernel exactly as predicted — the differential
proves the fence source is the T720 kernel build.

TEN-PASSES was NOT run (one flight + A/B control per the
brief); no 10/10 claim is made. The futex regression vector
(`files/regression-vector-futex-null-probe.md`) lives in a
QEMU `-d int` log the supervisor does not enable, so it cannot
be checked from the UART — consistent with T714–T719 practice;
no UART pattern was invented.

## 9. Successor program + scaffold-revert requirement

The fence source now exists. What remains is the multi-lane
program named by T719 §6:

1. **Composer sw_sync retire fence**: replace the hardcoded
   `setPresentFence(-1)` in the composer backend with a real
   fence created from the sw_sync timeline (the interface
   proven here), so SurfaceFlinger receives a valid retire
   fence fd.
2. **EGL native-fence support**: RenderEngine must be able to
   dup/consume the native fence (the 233 dup-fail lines are
   the residual symptom until this lands).

SCAFFOLD-REVERT REQUIREMENT (restated): the 4-byte
surfaceflinger scaffold patch (system.img `ac15a68a`) reverts
ONLY when the real retire fence lands and is proven in the
guest. It stays in every flight until then. The GateKeeper NPE
is a separate named lane and is NOT part of this program.

## 10. Reflection (REFLECT-ON-EVERY-PROBLEM)

Two probe bugs appeared in this lane; both were caught by
evidence, not by luck, and both changed durable state.

**Problem 1 — v1 sysfs mount aborts the boot.** What happened:
the v1 probe mounted sysfs on `/sys`; init's first-stage mount
failed EBUSY and aborted at guest 9.54 s (attempt-1 UART,
InitFatalReboot trace). Root cause: the probe assumed mounting
sysfs was harmless because the early-ramdisk script creates
device nodes freely — but init CHECKs its own first-stage
mounts, and an already-mounted `/sys` is fatal. Missed signal:
the early-ramdisk script itself mounts NOTHING (verified by
grep before writing the probe) — that absence was the signal
that the pre-init environment must be left pristine; it was
read as "the probe must mount what it needs" instead of "the
probe must not leave mounts behind". What changes: guest
probes mount ONLY on private mountpoints they create, and
unmount before exit (v3 rule). Where it lives: this findings
§4 + the v3 probe source header + the brief-template lesson
for future probe lanes (a probe that leaves persistent guest
state is a boot hazard, not a neutral observer).

**Problem 2 — v2 persistent debugfs mount kills switch_root.**
What happened: with `/sys` untouched, the boot still died
silently right after switch_root (attempt 2, guest 10.0 s).
Root cause: the debugfs mount on `/t720dbg` persisted across
switch_root and broke the transition (mechanism unproven at
the init-code level; the bisection + fix confirm the mount as
the cause). Missed signal: attempt 1's diagnosis ("sysfs was
the problem") was accepted after ONE variable changed; the
v2 fix addressed the location (`/sys` → `/t720dbg`) but not
the property (persistence). The bisection (A healthy, B
healthy, A+B dead) was the correct next step and should have
been the FIRST step after attempt 1 — it isolated the
interaction in two flights instead of one more guess. What
changes: when a fix changes one variable and the failure
persists in a new form, bisect before guessing again; and
guest probes leave ZERO persistent state (unmount + rmdir on
all paths, verified in the UART). Where it lives: this
findings §4.2–4.3 + the v3 probe source + the lane's
bisection-flight evidence.

**What went right (kept):** the config-diff catch (missing
`LD=ld.lld` dropping toolchain symbols) was caught by reading
the diff, not by a failed boot; the 56 B `sync_file_info`
correction was caught before building; the fence-status
semantics were corrected from the kernel source, not assumed;
the evidence-preservation rule kept all three smoke attempts
plus both bisection flights. The probe's DONE-line design
(single summary line, always exit 0) made every flight's
adjudication a one-grep decision.

## 11. Artifact SHAs

| Artifact | SHA-256 (prefix) | Size |
|---|---|---|
| New kernel Image (`t720-Image`) | `70d3859d…` | 20,396,544 B |
| New `.config` (`dotconfig-t720.txt`) | `6c162ecf…` | — |
| Probe v3 (`t720-probe`) | `61140fc7…` | 7,440 B |
| CPIO.gz v3 (`t720-fix.cpio.gz`) | `848a470f…` | 53,384,319 B |
| DTB v3 (`t720-modified.dtb`) | `3ff2bb8e…` | 2,049 B |
| system.img (unchanged, scaffold) | `ac15a68a…` | 2,059,816,960 B |
| vendor.img (unchanged) | `bac3cdc1…` | 134,217,728 B |
| Baseline Image (`t705-Image`) | `53d9563e…` | 20,126,208 B |
| Canonical Image (control) | `05c43917…` | 23,073,280 B |
| Smoke UART (attempt 3) | `67f92b81…` | 13,683 lines |
| Smoke attempt-1 UART | `58534665…` | 599 lines |
| Smoke attempt-2 UART | `fc5731e7…` | 560 lines |
| Bisection A UART | `ee250495…` | 13,640 lines |
| Bisection B UART | `43d659ee…` | 13,825 lines |
| Flight-01 UART | `f1ee6d02…` | 320,917 lines |
| Flight-ab UART | `ff06cba6…` | 595 lines |
| Brief | `bf2f1782…` | 36,666 B |

Full SHAs: kernel
`70d3859d6aed1906d27217aa3fdfd9bb953a57f4490c89c51efa4f6490441e04`;
CPIO
`848a470fa1241ba05e412c70158407f1b0dbdc593b29d1ce0dc16596d5c9657a`;
DTB
`3ff2bb8ec704d6df555c333ef13ce157d02e7d5b7e872768a1f21776ad90a9b2`;
probe
`61140fc751bded9d4ca7f87190148f54a54b42e96c5386bd27328ab1be32b97d`;
smoke UART
`67f92b81a116129de6598f04b94328de00795ced425bbc7172268c80ad596713`;
flight-01 UART
`f1ee6d02f4ac503c9f64d50c5d2f58f605bb60da242a6166f800ae06617f33de`.
Scripts: `scripts/supervisor_t720.py` (pinned to the flown
SHAs), `scripts/verify_smoke_t720.py`,
`scripts/verify_flight_ab_t720.py`,
`scripts/analyze_t720_flight01.py`, launchers
`launch_smoke_t720.py` / `launch_flight_01_t720.py` /
`launch_flight_ab_t720.py` — all adapted from the T719
versions, import-tested and fixture-verified before launch
(exact reproduction on the T719 UARTs). Box build state in
`/home/muse/t720-work/` (build.sh, build.log, config diffs,
pristine snapshot, CPIO/DTB tooling, all three probe versions).
