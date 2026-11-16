# Findings — Track T721: composer sw_sync retire fence

Verdict: PASS

WALL-TYPE-TAG: fence-delivery

Wall: NO-RETIRE-FENCE — the composer backend hardcoded `setPresentFence(-1)`
on every present path, so SurfaceFlinger never received a real retire fence
and logged `Invalid present fence` on every presented frame. T720 proved the
fence source live in the guest (CONFIG_SW_SYNC kernel + debugfs sw_sync
interface). This lane replaces the hardcoded -1 with a REAL fence created
from the sw_sync timeline, and proves it live.

Rust par bhi lagu: the Rust emulator's composer backend must implement the
same retire-fence semantics — one real fence per present, created from the
sw_sync timeline and signalled after the frame blit completes — because
SurfaceFlinger treats a -1 present fence as an invalid frame and drops the
fence accounting. A Rust backend that passes -1 reproduces this wall exactly.

## 1. Summary

The composer backend's `present()` now calls a trampoline hosted in
repurposed dead `sync_file_info` code. The trampoline opens the sw_sync
timeline once (cached), creates one fence per present with a monotonically
increasing value, signals it immediately (retire semantics: the blit is
already done), and returns the fence fd to `setPresentFence`. On any
failure it returns -1, reproducing the previous behaviour exactly.

Proven live in the guest (see §4):

- The one-time composer log line `T721 fd=10 tl=9` appears in flight-01
  (L23105) and in the scaffold-revert flight (L22806): a real fence fd 10
  from timeline fd 9, from the composer process.
- AVC audit lines show the composer process (pid 2416) issuing
  CREATE_FENCE (ioctl 0x5700) + INC (0x5701) ioctls on
  `/sys/kernel/debug/sync/sw_sync` (flight-01 L22382–L22554).
- The scaffold-revert flight holds Invalid present fence at 0 WITHOUT the
  scaffold (original surfaceflinger restored): the fence is real.
- All T715–T720 holds intact: REJECT 0, EPERM 0, Watchdog kills 0,
  `sys.boot_completed` present in every flight.

## 2. Characterisation (Step 2)

All from the flown composer ELF dumped read-only from the vault vendor
image: `/bin/hw/android.hardware.graphics.composer@2.1-service`,
SHA `8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401`,
19,942,712 B, full symbol table (7,090 symbols).

### 2.1 Present path (T719 addresses re-verified exactly)

- `ComposerClientImpl::present()` at `0x5bf1c`. All paths converge at
  `0x5bff0` (`ldr x0, [x19, #64]` — the CommandWriterBase pointer).
- `0x5bff4`: `mov w1, #-1` (`12800001`).
- `0x5c000`: epilogue + `b 0x5c1e0` (tail-call to
  `CommandWriterBase::setPresentFence(int)`).
- Exactly ONE branch/call site to `0x5c1e0` in the whole disassembly
  (2,800 functions). The patch site `0x5bff4` is the single choke point.

### 2.2 setPresentFence + writeFence fd-ownership contract

`setPresentFence` (0x5c1e0) saves the fd, calls `beginCommandBase`
(command `0x1030000`, length 1), then `writeFence` (0x5c3c0). `writeFence`:

- fd >= 0: `native_handle_create(1, 0)`, stores the fd at handle+12,
  `push_back` into the writer's handle vector (this+0x38), then
  `writeHandle(handle, false)`.
- fd < 0: `writeHandle(null)` (the T719 -1 path).
- Handle-create failure only: logs, `sync_wait(fd, -1)`, then `close(fd)`.

The caller TRANSFERS ownership: on the success path the fd lives in a
native_handle owned by the writer's vector; on the failure path writeFence
waits on and closes it. The caller (present) never closes after the call.
The fd is consumed exactly once on every path: returning a fresh fence fd
per present cannot leak by construction.

### 2.3 sw_sync helpers: present, dead, usable

`sw_sync_timeline_create` (0xb78b4), `sw_sync_timeline_inc` (0xb78ec),
`sw_sync_fence_create` (0xb7918) are linked into the composer binary with
ZERO call sites in the whole disassembly (T719 confirmed). Timeline create
opens `"/sys/kernel/debug/sync/sw_sync"` FIRST (O_RDWR), falling back to
`"/dev/sw_sync"`. CREATE_FENCE ioctl `0xC0285700`, INC ioctl `0x40045701`
— matching the T720 probe. The dead libsync code is usable in this guest.

### 2.4 Debugfs reachability (the mount question)

- The composer service runs as `user root`, `group graphics drmrpc`,
  `capabilities SYS_NICE SYS_RAWIO` — NO CAP_SYS_ADMIN, so it cannot mount
  debugfs itself.
- SELinux is permissive (cmdline `androidboot.selinux=permiss`).
- init does NOT mount debugfs: `init.rc` + `fstab.virt` from the T720 CPIO
  contain no debugfs reference; fstab lists only /system + /vendor. The
  T720 probe mounted debugfs itself on a private mountpoint.
- Resolution: an `on post-fs-data` trigger in the composer .rc mounts
  debugfs at /sys/kernel/debug (init runs it as root with full
  capabilities). The trigger is part of the vendor image patch (§3.3).

### 2.5 Route justification: binary patch (source rebuild unproven)

T675/T676 record exhaustive local + public searches finding no SimpleFB /
Composer implementation source, no AOSP checkout, no proven build recipe.
Own checks confirm: `/Users/Shared/composer-2-1-tree` holds HIDL interfaces
+ utils only, no simplefb backend; the box holds no composer source
(`.cpp`/`.h`/`Android.bp` absent). Source rebuild is unproven; the binary
patch follows the T718 precedent (dead-code trampoline + recorded
restoration bytes).

### 2.6 Trampoline host + static storage

- `.bss` slack is 0 (last symbol ends exactly at the section end); no
  NOP/zero run >= 64 B exists in `.text`.
- Host: `sync_file_info` at `0xb7670`, 536 B. Proven dead: 0 branch
  targets, 0 data pointers in `.data.rel.ro`/`.data`, 0 `adrp`/`adr`
  references, absent from `.dynsym` (168 entries). Original 536 bytes
  saved (`sync_file_info-orig.bin`, SHA `e6ed67fc...`) for restoration.
- Static storage: `.bss` is last in the final LOAD segment (memsz end
  `0x115BC8`, same page to `0x116000`). The splice extends `p_memsz` and
  `.bss` `sh_size` by 12 bytes (timeline_fd + fence_counter + logged_flag,
  zero-initialised). The extension stays in the mapped page; the heap
  start (page-aligned) is unchanged.

## 3. Implementation (Step 2, continued)

### 3.1 Trampoline design (tramp.S, 241 B)

Called via `bl` from the patch site. On entry x0 = writer (preserved),
x19 = `this` (preserved). Returns w1 = fence fd or -1.

- Frame (48 B, 16-aligned, slot-disjoint, verified from assembled
  offsets): saved x29/x30, saved x0/x19, 16 B scratch (fence-fd save).
- Timeline: load cached fd; if <= 0, `sw_sync_timeline_create`; on
  failure return -1 (previous behaviour); on success cache it. A failed
  open is retried on the next present (no negative caching); a successful
  open is cached for the service lifetime ("opens once").
- Fence: counter++, `sw_sync_fence_create(timeline, "t721retire", counter)`;
  on failure return -1. Save the fence fd; `sw_sync_timeline_inc(timeline,
  1)` — signal-before-return (retire semantics: the blit already wrote
  fb0, so the fence is signalled before SF ever sees it).
- One-time log: on the first successful fence only,
  `__android_log_print(INFO, "T721", "T721 fd=%d tl=%d", fence_fd,
  timeline_fd)`. The name/tag/format strings live as data after the code
  in the same cave (same 4 KiB page, `adrp`+`add` reachable).
- Fail path: `mov w1, #-1` (encoding `0x12800001` — the exact original
  instruction), restore, return.

Assembly note: the first assembly produced WRONG `bl` targets — GAS
resolves CALL26 to absolute `.set` symbols at assembly time against base
0. Fixed by leaving call targets undefined and resolving them at link
time (`ld --defsym`); all four targets re-verified in disassembly
(create `0xb78b4`, fence `0xb7918`, inc `0xb78ec`, log `0xb5b80`).

### 3.2 Splice (splice_t721.py)

- Asserts the original binary SHA + size, the patch-site word
  (`0x12800001`), the PHDR memsz (`0x1a98`) and `.bss` size (`0x1748`),
  parsed from the ELF headers (no hardcoded offsets).
- Writes: patch-site `bl` (`0x94016d9f`), the trampoline + NOP fill over
  the 536 B cave, `p_memsz` +12, `.bss` `sh_size` +12.
- Full byte-diff confinement: 521 bytes differ, all inside the 4 expected
  regions (patch site, cave, PHDR, section header). No other byte moves.
- A first splice attempt overflowed the cave by 1 byte (241 B trampoline
  is not 4-aligned, so the last NOP word spilled past byte 536). The
  script's confinement assertion REFUSED the write; fixed by zero-padding
  the trampoline to a 4-byte boundary first. No bad artifact was produced.
- Patched composer: 19,942,712 B (same size),
  SHA `f7600821894b923f5a014143ac6ea57dec6e1f2d448ec78f6d17d983159e2ea5`.
  Patch site + cave + headers re-verified by disassembly of the patched
  binary itself.

### 3.3 Vendor image (patch_vendor_t721.py, T718 procedure)

Base: T718 vendor.img (`bac3cdc1...`, 134,217,728 B). Two files patched
via rm-first-then-write (zero free inodes), explicit metadata + xattr,
e2fsck validation:

1. Composer ELF → patched binary (`f7600821...`), inode 93 reused,
   mode 0100755, uid 0, gid 2000, xattr
   `hal_graphics_composer_default_exec`.
2. Composer .rc → original 259 B + appended trigger (exact original bytes
   extended, SYS_RAWIO line preserved):
   `on post-fs-data` / `mount debugfs debugfs /sys/kernel/debug`.
   New .rc 320 B, SHA `485a59c9...`, inode 305 reused, mode 0100644,
   uid 0, gid 0, xattr `vendor_configs_file`.

Verified: e2fsck `-f -n` exit 0; dumped ELF + .rc byte-identical to
staged files; all 7 neighbours byte-identical; T718 shim (`3daa5a49...`)
+ HIDL allocator byte-identical; base untouched.
Result: `t721-vendor.img`, 134,217,728 B,
SHA `6bf3e2e0ebbc7dc06031ad64bfaae1acb7fe207e1453e297af6639bbb215148b`.

### 3.4 CPIO rebuild + DTB re-align (the embedded-vendor correction)

CRITICAL CORRECTION (see §7): the guest loop-mounts the vendor image
EMBEDDED in the CPIO (`ramdisk-images/vendor.img`, attached read-only to
/dev/loop1 by the early-ramdisk script). QEMU gets no vendor drive, so
the supervisor's vendor file is a consistency check only. The first
smoke + flight-01 flew the T720 CPIO embedding the ORIGINAL composer and
were aborted/archived when the mismatch was proven (UART L339 +
CPIO member hash).

- `cpio_replace_t721.py` (T720 procedure): replaces ONLY the
  `ramdisk-images/vendor.img` member (same size, 134,217,728 B);
  member-by-member comparison proves all other 237 members + trailer
  byte-identical, headers identical; deterministic gzip (`-9 -n`).
  Base `.cpio` proven to re-gzip to the verified `848a470f...` first.
  Result: `t721-fix.cpio.gz`, 53,383,972 B,
  SHA `6a302597c19156b99b3b07a8c8a33ca2ef9d823af9aa1008a5da67e822a018f1`.
- `dtb_patch_t721.py`: initrd-end `0x1132e947f` → `0x1132e9324`
  (start `0x110000000` + new size); 2-byte confined diff.
  Result: `t721-modified.dtb`, 2,049 B,
  SHA `91b45a10ea3f33edc36b8a945230ea7873b7b9ffe91bbff7f23cb34628ee8bf4`.

### 3.5 Scaffold-revert image (patch_system_revert_t721.py, T719 procedure)

Restores the ORIGINAL surfaceflinger (`259d39f5...`, 8,336,320 B) into a
copy of the T719 scaffold system image: the 4 scaffold bytes at
`0x204060` (`06000014`) revert to the original bytes (`c0030054`); the
diff is exactly those 3 differing bytes. e2fsck clean; dumped SF
byte-identical to the original; all 9 neighbours byte-identical
(incl. installd.rc `3ea3466b...` + trigger line); base untouched.
Result: `t721-system-revert.img`, 2,059,816,960 B,
SHA `0ba6d6d353b406a7c0a3df340be60bde678ee215b5b8c3bee5ec7a7274d3e3d3`.

### 3.6 Guest-visible proof lines (recorded BEFORE flying)

1. UART line matching `T721 fd=` + `tl=` from the composer process
   (one-time trampoline log) — proves the mount + the fence creation.
2. Revert-flight fence-errors-0 WITHOUT the scaffold — proves the fence
   reaches SF as a valid fd (decisive).
3. A/B control: fence errors RETURN with the old composer — attributes
   the effect to the new composer.

## 4. Flights (Step 3)

Inputs pinned by the supervisor (SHA-verified at every launch): kernel
`t720-Image` (`70d3859d...`), DTB `t721-modified.dtb` (`91b45a10...`,
A/B: `t720-modified.dtb` `3ff2bb8e...`), CPIO `t721-fix.cpio.gz`
(`6a302597...`, A/B: `t720-fix.cpio.gz` `848a470f...`), system
`t719-system.img` (`ac15a68a...`, revert/A/B: `t721-system-revert.img`
`0ba6d6d3...`), vendor file `t721-vendor.img` (`6bf3e2e0...`, A/B:
`t718-vendor.img` `bac3cdc1...`, matching the embedded copy).

### 4.1 Smoke (Step 3a) — PASS (redo with corrected CPIO)

The first smoke (T720 CPIO, original composer) passed the allocator /
holds prongs but never tested the fence; archived to
`flights/smoke-cpio-vendor-mismatch/` and superseded. The redo:

- Host timeout 300.6 s, guest 186.3 s, UART 13,829 lines. Cmdline
  confirms the corrected inputs (initramfs `6a302597...`, DTB
  `91b45a10...`); embedded vendor attached to loop1.
- Structural all PASS. REJECT 0, CALL allocate2 2, GBA failures 0,
  allocateBuffers failures 0, EPERM 0, Watchdog kills 0,
  installd-not-found 0, sensorservice avc 0, system_server 1,
  T720_PROBE DONE PASS 1 / FAIL 0.
- mapped 0 + T721 log 0: present() had not run (first /dev/mem open
  ~522 s guest, past the window), so the fence prong was DEFERRED to
  flight-01 per the observable-proxy rule — not a fence fail.

### 4.2 Flight-01 (Step 3b, scaffold IN) — PASS

- Guest 2,464.5 s (2400 s target exceeded), UART 166,172 lines,
  21,507,584 B,
  SHA `8ecdf8e413d12c17c483fc55b020d0bdb8a602a674576f8148c540a0bd90e13a`.
- T721 fence log: 1 (L23105: `T721 fd=10 tl=9`, pid 2416). Sw_sync
  ioctls audited from the composer process (CREATE + INC, permissive).
- REJECT 0, CALL allocate2 102, GBA 0, allocateBuffers 0, EPERM 0,
  mapped 1, Watchdog kills 0, `sys.boot_completed` 13, Launcher3 dexopt
  SKIPPED 0, Invalid present fence 0 (scaffold IN), composer E-lines 0,
  T720_PROBE DONE PASS 1 / FAIL 0.
- Churn: 8 system_server lifetimes. Cause adjudicated: the KNOWN
  GateKeeper NPE race (L23144, exact T715–T720 signature
  `IGateKeeperService.reportDeviceSetupComplete()` on null). No composer
  crash (2 pattern matches were binary-blob false positives: a build-id
  dump and a benign composer hex-dump line). FATALs were app processes
  only. CALL/dup-fail counts lower than T720 from less steady-state
  presenting time, not a fence regression. GateKeeper remains the
  parallel wall (separate named lane, not this track).
- HARNESS CAVEAT: the supervisor exited(10) at host 1,382 s on the disk
  gate (7.96 GiB < 8 GiB); QEMU was orphaned and ran on to guest
  2,464 s; manual SIGTERM stop. No supervisor flight-end.json; a manual
  flight-end record stands (`flights/flight-01/flight-end-manual.json`).
  The evidence window exceeds the target; the irregular ending does not
  affect the guest evidence.

### 4.3 Scaffold-revert flight (Step 3c) — PASS, DECISIVE

- Guest 600.6 s (target reached), QEMU exit 0, UART 24,748 lines,
  SHA `76d47743fc835d5389b9868cf91bce83d8f8e00c62f809ed9ef2b7e9e430507c`.
- T721 fence log: 1 (L22806: `T721 fd=10 tl=9`, pid 2417) — presents
  happened in the window.
- **Invalid present fence: 0 WITHOUT the scaffold** (original
  surfaceflinger). At the T718 rate (~1/s once presenting), the ~35 s
  presenting window would show ~30+ errors for a fake fence. Zero with
  presents confirmed = the retire fence is real.
- REJECT 0, CALL 12, EPERM 0, mapped 1, kills 0, system_server 1
  lifetime, `sys.boot_completed` 4, Launcher3 dexopt SKIPPED 0,
  T720_PROBE DONE PASS 1 / FAIL 0.

### 4.4 A/B control (Step 3d) — PASS (differential proven)

Old composer (T720 CPIO embedding the original vendor image, T720 DTB)
+ revert system (original surfaceflinger) on the T720 kernel. Preflight
SHAs confirm the configuration: kernel `70d3859d...`, DTB `3ff2bb8e...`,
CPIO `848a470f...`, system `0ba6d6d3...`, vendor file `bac3cdc1...`
(matching the embedded copy).

- Guest 700.2 s (target reached), QEMU exit 0, UART 29,393 lines,
  3,399,508 B,
  SHA `99246fbaa596452d75ed7a0bfca521a853ad3039fe928b90585f58f8dc41eb58`.
- T721 fence log: 0 (old composer, as expected).
- **Invalid present fence: 72 — the errors RETURN with the old
  composer.** First error L23911 (`E/Scheduler( 2491): trackPendingFrame:
  Invalid present fence`), then ~1/s through the presenting window.
- REJECT 0, CALL 14, EPERM 0, mapped 1, kills 0, `sys.boot_completed` 7,
  T720_PROBE DONE PASS 1 / FAIL 0.

The differential is airtight: flight-revert and flight-ab share the
kernel, the system image, and the flight harness; the ONLY difference is
the composer. 0 errors (new composer) vs 72 errors (old composer) =
the retire fence is real, and the new composer is its cause.

## 5. Adjudication (Step 4)

| Criterion (brief) | Result |
|---|---|
| Composer creates a real fence per present (guest-visible log) | PASS — `T721 fd=10 tl=9` in flight-01 (L23105) + revert (L22806); sw_sync ioctls audited from the composer process |
| Fence errors 0 WITHOUT the scaffold (Step 3c, decisive) | PASS — 0 in the revert flight, with presents confirmed |
| A/B control: errors return with the old composer (Step 3d) | PASS — 72 errors, T721 log 0 |
| Holds: REJECT 0, EPERM 0, kills 0, `sys.boot_completed` present | PASS — every flight |
| No fd leak / no composer regression | PASS — writeFence ownership contract (§2.2); composer E-lines 0 in flight-01; no new crash class |
| Smoke (Step 3a) | PASS — holds + structural; fence prong deferred to flight-01 per the observable-proxy rule |

Verdict: PASS. The NO-RETIRE-FENCE wall is BROKEN and PROVEN: the
composer now delivers a real sw_sync retire fence per present, and
SurfaceFlinger accepts it. Successor: EGL native-fence lane (T722).

## 6. Holds and regressions

- T628 shim (T718): REJECT 0 in every flight; CALL counts 2/102/12.
- T716 CAP_SYS_RAWIO: EPERM 0, mapped >= 1 in flight-01 + revert.
- T715 installd trigger: installd-not-found 0, fixAppsData 0 (healthy).
- T720 fence source: T720_PROBE DONE PASS in every flight, FAIL 0.
- GateKeeper NPE: persists (parallel wall, unchanged signature).
- EGL dup-fail (T722 scope): 10 in flight-01 (T720: 233), 4 in revert —
  lower counts follow from less presenting time / shorter windows, not
  from a fence change; EGL native-fence work is the successor lane.
- No new crash class. No composer error lines in any flight.

## 7. Caveats and near-misses

1. **CPIO-embedded vendor (the lane's central miss).** Step 1 verified
   the vendor FILE SHA but not the CPIO-embedded copy. The first smoke +
   flight-01 flew the original composer for ~2 h of wall time before the
   `mapped=1 + T721-log 0` signature forced the investigation that found
   UART L339 (`attached embedded vendor.img to loop1`) + the member hash
   (`bac3cdc1...`). Both flights were aborted, archived, and re-run with
   the rebuilt CPIO. The supervisor now documents the embedded-vendor
   fact, and the verified vendor file must always match the embedded
   member. See §8 reflection.
2. **GAS CALL26 resolution.** The first trampoline assembly encoded
   branch targets against base 0 (absolute `.set` symbols). Caught by
   disassembly verification before any splice; fixed via link-time
   `--defsym`.
3. **Splice NOP overflow.** The first splice spilled 1 byte past the
   cave end (non-4-aligned trampoline). The confinement assertion refused
   the write; no bad artifact was produced.
4. **Supervisor disk-gate exit.** Flight-01's supervisor died at host
   1,382 s (7.96 GiB); QEMU orphaned to guest 2,464 s. Evidence complete;
   manual flight-end record written. Disk recovered to 8.06 GiB after
   freeing regenerable derivatives; the revert + A/B flights ran clean.
5. **240 s A/B abort.** The first A/B launch (240 s) was aborted after
   ~1 min: it would end before the first present (~565 s) and be
   vacuous. Relaunched at 700 s. Partial UART archived.
6. **No TEN-PASSES claim.** The brief's ten-passes / futex-vector
   language is not UART-checkable in this lane; no such claim is made.

## 8. Reflection

(Required by the reflect-on-every-problem rule for the §7 problems.)

**What happened:** the lane changed the vendor image file without
rebuilding the CPIO that embeds it, and flew two flights on the wrong
composer before the `mapped=1 + T721-log 0` signature exposed it.

**Root cause:** the supervisor verifies a vendor FILE that QEMU never
consumes, and Step 1 verified that file's SHA — so every check passed
while the consumed artifact (the CPIO member) was stale. The T718
precedent (CPIO rebuilt with the vendor image) was in the record but was
not applied as a checklist item; the CPIO was treated as "the T720 CPIO,
unchanged" instead of "the CPIO, whose vendor member is a T721 input."

**Missed signal:** the QEMU command line has no vendor drive. Reading
the supervisor's QEMU command before the first launch would have shown
the vendor file is unconsumed and forced the question "where does the
guest get /vendor?" — answerable in 2 minutes from the early-ramdisk
script. The command was read only AFTER the mismatch signature appeared.

**What changes:** (1) The supervisor docstring now states the
embedded-vendor fact + the file/member correspondence rule. (2) Future
briefs that change vendor.img must name the CPIO member replacement +
DTB re-align as explicit steps (this lane's §3.4 is the template).
(3) A pre-flight check for vendor-changing lanes: hash the CPIO's
embedded `ramdisk-images/vendor.img` member and require it to equal the
expected vendor SHA before the first launch.

**Where it lives:** this section; the supervisor docstring
(`scripts/supervisor_t721.py`); the CPIO rebuild script
(`scripts/cpio_replace_t721.py`, which asserts the old/new member SHAs).

The assembly + splice near-misses (§7.2–7.3) share one lesson: verify
assembled/linked bytes by disassembly and diff confinement BEFORE the
artifact is trusted — both were caught by the verification step, never
by the flight. That verification order (assemble → disassemble-verify →
splice → diff-verify → fly) is the control that worked, and it stays.

## 9. Artifacts

| Artifact | Size (B) | SHA-256 |
|---|---|---|
| Trampoline `tramp.bin` | 241 | `878f6b7d82e9a2508c881659d1ab0f9c0962c55fcaad38e0d96d5dc148994f1c` |
| Cave original `sync_file_info-orig.bin` | 536 | `e6ed67fc755acba17a3d9e01366a6595397d01178f056e2f6ff616de2af99170` |
| Patched composer | 19,942,712 | `f7600821894b923f5a014143ac6ea57dec6e1f2d448ec78f6d17d983159e2ea5` |
| Original composer | 19,942,712 | `8d62a642c815f56c343cc2167126db9fab62a199796efe78f1414a3bd93bd401` |
| `t721-vendor.img` | 134,217,728 | `6bf3e2e0ebbc7dc06031ad64bfaae1acb7fe207e1453e297af6639bbb215148b` |
| New composer .rc | 320 | `485a59c9407a5ac8e45dfd91136047998cd3daffbe7772d7ba14f672486a952c` |
| `t721-fix.cpio.gz` | 53,383,972 | `6a302597c19156b99b3b07a8c8a33ca2ef9d823af9aa1008a5da67e822a018f1` |
| `t721-modified.dtb` | 2,049 | `91b45a10ea3f33edc36b8a945230ea7873b7b9ffe91bbff7f23cb34628ee8bf4` |
| `t721-system-revert.img` | 2,059,816,960 | `0ba6d6d353b406a7c0a3df340be60bde678ee215b5b8c3bee5ec7a7274d3e3d3` |
| Smoke-redo UART | 13,829 lines | `729f42a12f4dd60de54f5ca90b5e52733c4753ddf5f6f33b14b8e2e744140777` |
| Flight-01 UART | 166,172 lines | `8ecdf8e413d12c17c483fc55b020d0bdb8a602a674576f8148c540a0bd90e13a` |
| Revert UART | 24,748 lines | `76d47743fc835d5389b9868cf91bce83d8f8e00c62f809ed9ef2b7e9e430507c` |
| A/B UART | 29,393 lines | `99246fbaa596452d75ed7a0bfca521a853ad3039fe928b90585f58f8dc41eb58` |

## Appendix: restoration record

To restore the original composer from the patched binary (file offsets
are derived from the ELF program headers, as `splice_t721.py` does —
do NOT assume a fixed vaddr-to-offset shift; the T718 shim's 0x4000
shift does not apply to this binary):

- Patch site vaddr `0x5bff4`: restore word `0x12800001`
  (`mov w1, #-1`).
- Cave vaddr `0xb7670..0xb7888` (536 B): restore
  `sync_file_info-orig.bin` (SHA `e6ed67fc...`).
- PHDR `p_memsz`: `0x1aa4` → `0x1a98`; `.bss` `sh_size`: `0x1754` →
  `0x1748`.
- The restored binary must hash to the original composer SHA
  `8d62a642...` before any use.
