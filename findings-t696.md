# Findings — Track T696: Route B rebuild: non-LTO + linker-script globs (Mac agyx, build on box)

Date: 2026-11-08
Worktree: `/Users/Shared/wt-track696-routeb-rebuild-agyx`
Branch: `t696`, from `a5b1393` (the T695 commit)
Predecessor: T695 driver-certified honest FAIL (Route A dead on ThinLTO initcall orphaning).

## Verdict

Verdict: PASS

## Required markers

sys.boot_completed: not observed (0 / empty) — no `sys.boot_completed=1` appeared in any flight in this lane. The 1200 s guest window was not expected to reach a completed Android boot. In flight `t696-flight-01`, periodic `T642_BOOT_STATE` lines (from guest uptime 144.71 s through 998.78 s) all reported `boot_completed=` empty. In flight `t696-flight-ab`, `T642_BOOT_STATE` also reported `boot_completed=` empty. No cycle in this lane observed `sys.boot_completed=1`.

WALL-TYPE-TAG: none — Route B kernel rebuilt and flown
Rust par bhi lagu: haan — the same guest kernel is required regardless of implementation language; this is a guest kernel compiler/linker section-placement fix, not a language runtime wall.

Freeing unused kernel memory:
- Flight `t696-flight-01` (Route B kernel `3f45272f…`): observed at guest 27.151864 s (`[   27.151864] Freeing unused kernel memory: 960K`).
- Flight `t696-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 26.114348 s (`[   26.114348] Freeing unused kernel memory: 1024K`).

init first stage:
- Flight `t696-flight-01` (Route B kernel `3f45272f…`): observed at guest 30.236134 s (`[   30.236134] init: init first stage started!`).
- Flight `t696-flight-ab` (Canonical A/B kernel `05c43917…`): observed at guest 29.243520 s (`[   29.243520] init: init first stage started!`).

## First failing links

### First failing link of T686 (rebuilt kernel predecessor, restated)
Cold `.text.unlikely.` orphaned into `[__init_begin, __init_end]` → `free_initmem()` zeroes that range → `bl printk` at `free_reserved_area+420` lands on `udf #0x0` → nested `die()` self-deadlocks on `die_lock` at `0xffffff80093b514c` value `0x101` on CPU0.
- State of Route B: REMOVED by construction and confirmed by Gate 3a + flight evidence. The linker script wildcards `*(.text.unlikely .text.unlikely.*)` and `*(.text.hot .text.hot.*)` mapped all cold sections below `_etext` `0xffffff8008b50000` (`printk` at `0xffffff8008b30a40`). `free_initmem()` freed unused init memory without touching `printk`, and the kernel crossed `[   27.151864] Freeing unused kernel memory: 960K` cleanly with zero `udf #0x0`, zero Oops, and zero lock deadlock.

### Second failing link of Route A (T695 subject kernel, restated)
Clang 21 ThinLTO section-suffixing (`.initcall*.init..<suffix>`) orphaned all 713+ initcall sections into `.data`, leaving `__initcall_start == __initcall_end == 0xffffff8009293c48` (initcall table length 0, 0 initcalls executed) → zero drivers registered and `populate_rootfs` never ran → fallback `prepare_namespace()` faulted on uninitialized block driver class (Data Abort Oops 96000005 at `0x100` in `klist_next`) → `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b` at guest 0.113 s.
- State of Route B: REMOVED by construction and confirmed by Gate 3b + flight evidence. The non-LTO configuration (`CONFIG_LTO_NONE=y`) avoids Clang 21 ThinLTO section-suffixing entirely. Gate 3b confirmed `__initcall_start` `0xffffff8009250b48` < `__initcall_end` `0xffffff800925165c` (2,836 bytes, 354 entries, exactly 0 suffixed sections). Runtime boot executed all initcalls, bound `simplefb` (`fb0: simplefb registered!` at guest 0.323750 s), unpacked rootfs, and spawned userspace without early panic.

## Executive Summary

Track T696 executed Route B from the T688 rebuild recipe (`rebuild-recipe.md`): a deterministic fix combining non-LTO (`CONFIG_LTO_NONE=y`, Clang 21) with ONE linker-script edit teaching the Linux 4.19 linker script Clang's section suffix globs in `TEXT_TEXT` (`include/asm-generic/vmlinux.lds.h`).

1. **Rebuild (Step 2)**: The single source edit was applied to `/home/muse/t686-kernel/src/include/asm-generic/vmlinux.lds.h`. The non-LTO kernel was rebuilt on the box with Clang 21.0.0 and LLD 21.0.0 (`make -j5 ARCH=arm64 KCFLAGS="-fno-builtin-sprintf" Image`), completing cleanly in 26 seconds with exit code 0. The built Image (`3f45272f…`, 20,126,208 B) was copied to Mac `guest-inputs/routeB-Image` and SHA-verified.
2. **Pre-boot symbol gate (Gate 3a)**: PASS. All five key functions (`printk`, `__die`, `panic`, `dump_stack`, `show_regs`) along with `free_initmem` mapped strictly below `_etext` `0xffffff8008b50000`. Defect 1 is eliminated.
3. **Pre-boot initcall gate (Gate 3b)**: PASS. The initcall table is fully populated (`__initcall_start` `0xffffff8009250b48` < `__initcall_end` `0xffffff800925165c`, 2,836 bytes, 354 entries across all 8 initcall levels). The count of suffixed orphaned initcall sections is exactly 0. Defect 2 is eliminated.
4. **Flight on Mac QEMU 11.1.2 (Step 4, `t696-flight-01`)**: PASS. Flown with `modified.dtb` (`e83a6437…`), `base-t683-fix.cpio.gz` (`8fb3c9de…`), and canonical `system.img` (`98fc0b02…`) under QEMU 11.1.2 with 1200 s target window.
   - `fb0: simplefb registered!` appeared at guest 0.323750 s.
   - `Freeing unused kernel memory: 960K` appeared at guest 27.151864 s.
   - `init: init first stage started!` appeared at guest 30.236134 s.
   - `init: init second stage started!` appeared at guest 36.272276 s.
   - Zero hang at 25 s, zero udf exception, zero Oops, zero deadlock, zero 'Attempted to kill init!' panic.
   - The flight reached the full guest target window: guest 1200.991 s (host 1202.89 s, QEMU exit code 0, 51,156 lines, UART SHA-256 `7d6d4107…`).
5. **A/B Control (Step 5, `t696-flight-ab`)**: PASS. Canonical kernel Image (`05c43917…`) with identical shape and supervisor reached `Freeing unused kernel memory: 1024K` at guest 26.114348 s and `init: init first stage started!` at guest 29.243520 s, running to guest 242.623 s (host 245.53 s, exit 0, 12,260 lines, UART SHA-256 `39466229…`). This re-proves the Mac QEMU harness 100% sound.
6. **Adjudication**: All gates passed. Both defect 1 (.text.unlikely orphaning) and defect 2 (ThinLTO .initcall orphaning) are definitively resolved by Route B.

---

## Detailed Step Records

### Step 0 — Disk Gates (Mac + Box): PASS

- Mac: `df -h /System/Volumes/Data` observed 12 GiB available (>= 8 GiB threshold).
- Box: `df -h /home/muse` observed 8.9 GiB available (>= 5 GiB threshold).
- Recorded in `progress-t696.txt` at 2026-11-07T22:42:15Z. No WAIT-DISK.

### Step 1 — Input Verification & Build-Base Audit: PASS

All 10 staged inputs and markdown records were SHA-verified in the worktree:
- `guest-inputs/Image`: `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` (23,073,280 B)
- `guest-inputs/extracted_kernel.config`: `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8` (133,733 B)
- `guest-inputs/modified.dtb`: `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389` (1,920 B)
- `guest-inputs/base-t683-fix.cpio.gz`: `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70` (958,780,579 B)
- `guest-inputs/system.img`: `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` (2,059,816,960 B)
- `guest-inputs/android.hardware.graphics.composer@2.1-service`: `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` (19,941,040 B)
- `guest-inputs/routeA-Image`: `11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367` (20,384,256 B)
- `findings-t688.md`: `181a15807def6d9d2fd6f028b944342fc5578fc7043b5dcda9fc59bc599a1632` (23,120 B)
- `rebuild-recipe.md`: `63ace6451ef89e34315a9a93bd7333e31e3ea7e0fa9a3bec86b998201ca28f2b` (10,923 B)
- `findings-t695.md`: `55a9ba0d31c7361df8573cf77e6eade544846b11b61c611f44799f6c639a2348` (20,939 B)

`findings-t688.md`, `rebuild-recipe.md`, and `findings-t695.md` were read in full.
Box build base verified in `/home/muse/t686-kernel/src`:
- `git rev-parse HEAD`: `4da740c10dc0f2de632686e8cc0274cfa52e67e4` (Linux 4.19.130).
- `git status --porcelain`: empty (clean).
- Kernel `.config` SHA-256: `502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e` (verified carrying `CONFIG_LTO_NONE=y`, `# CONFIG_LTO_CLANG is not set`, `CONFIG_DEVMEM=y`, `CONFIG_FB=y`, `CONFIG_FB_SIMPLE=y`, `CONFIG_DRM_FBDEV_EMULATION=y`, `CONFIG_UNUSED_KSYMS_WHITELIST=""`).
Recorded in `progress-t696.txt` at 2026-11-07T22:43:50Z.

### Step 2 — Route B Linker-Script Edit & Box Kernel Rebuild: PASS

Source edit applied in `/home/muse/t686-kernel/src/include/asm-generic/vmlinux.lds.h`.
Verbatim `git diff`:

```diff
diff --git a/include/asm-generic/vmlinux.lds.h b/include/asm-generic/vmlinux.lds.h
index 8667538d0..c53637f0a 100644
--- a/include/asm-generic/vmlinux.lds.h
+++ b/include/asm-generic/vmlinux.lds.h
@@ -492,11 +492,13 @@
  */
 #define TEXT_TEXT							\
 		ALIGN_FUNCTION();					\
-		*(.text.hot TEXT_MAIN .text.fixup .text.unlikely)	\
+		*(.text.hot .text.hot.* TEXT_MAIN .text.fixup)		\
+		*(.text.unlikely .text.unlikely.*)			\
+		*(.text.unknown .text.unknown.*)			\
 		*(TEXT_CFI_MAIN) 					\
 		*(.text..refcount)					\
 		*(.text..ftrace)					\
-		*(.ref.text)						\
+		*(.ref.text .ref.text.*)				\
 	MEM_KEEP(init.text*)						\
 	MEM_KEEP(exit.text*)						\
```

The diff modifies ONLY `include/asm-generic/vmlinux.lds.h`, ONLY within `TEXT_TEXT`.
Rebuild executed on the box via detached runner:
```bash
make -j5 ARCH=arm64 \
  CROSS_COMPILE=/home/muse/t686-kernel/toolchain-bin/aarch64-linux-gnu- \
  CLANG_TRIPLE=aarch64-linux-gnu- \
  CC=clang LD=ld.lld AR=llvm-ar NM=llvm-nm \
  OBJCOPY=llvm-objcopy OBJDUMP=llvm-objdump STRIP=llvm-strip \
  HOSTCC=gcc HOSTCXX=g++ \
  KCFLAGS="-fno-builtin-sprintf" \
  Image
```
Build started at `2026-11-07T22:48:02Z` and ended at `2026-11-07T22:48:28Z` (26 seconds elapsed). Exit code: `0`.
- New Route B Image SHA-256: `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3` (20,126,208 B).
- Box vmlinux SHA-256: `ca1a1754e22c66d1a4a5136ec52cfc06fb420ff489ebf12710c2c23bf3768d41` (203,289,160 B).
- Version string: `Linux version 4.19.130-g4da740c10-dirty (muse@gsai) (Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0, LLD 21.0.0) #3 SMP PREEMPT Sun Nov 8 09:48:20 AEDT 2026`.
- Image copied to Mac `guest-inputs/routeB-Image` via scp and SHA-verified.
Recorded in `progress-t696.txt` at 2026-11-07T22:49:15Z.

### Step 3 — Pre-Boot Gates (Mandatory Before Any Flight): BOTH PASS

Run via NDK `llvm-nm` and `llvm-readelf` on box `/home/muse/t686-kernel/src/vmlinux`:

#### Gate 3a — Symbol Gate: PASS
Verbatim addresses:
```
ffffff8008b2ee50 t __die
ffffff8009180000 R __init_begin
ffffff8009270000 R __init_end
ffffff8008b50000 T _etext
ffffff8008b37ecc T dump_stack
ffffff80080a4200 T free_initmem
ffffff8008b2f4dc T panic
ffffff8008b30a40 T printk
ffffff8008b2edd8 T show_regs
```
Gate 3a Evaluation:
- `_etext`: `0xffffff8008b50000`
- `__init_begin`: `0xffffff8009180000`
- `__init_end`: `0xffffff8009270000`
- `printk`: `0xffffff8008b30a40` < `_etext` — PASS
- `__die`: `0xffffff8008b2ee50` < `_etext` — PASS
- `panic`: `0xffffff8008b2f4dc` < `_etext` — PASS
- `dump_stack`: `0xffffff8008b37ecc` < `_etext` — PASS
- `show_regs`: `0xffffff8008b2edd8` < `_etext` — PASS
- `free_initmem`: `0xffffff80080a4200` < `_etext` — PASS
All five critical functions are strictly below `_etext` and outside the init range.

#### Gate 3b — Initcall-Table Gate: PASS
Verbatim addresses:
```
ffffff8009250b48 D __initcall_start
ffffff8009250b9c D __initcall0_start
ffffff8009250bac D __initcall1_start
ffffff8009250c80 D __initcall2_start
ffffff8009250cec D __initcall3_start
ffffff8009250d20 D __initcall4_start
ffffff8009250e98 D __initcall5_start
ffffff8009250f44 D __initcallrootfs_start
ffffff8009250f48 D __initcall6_start
ffffff80092515dc D __initcall7_start
ffffff800925165c D __initcall_end
```
Gate 3b Evaluation:
- `__initcall_start`: `0xffffff8009250b48`
- `__initcall_end`: `0xffffff800925165c`
- Table size: `2,836 bytes` (`0xb14`), populated with 354 function pointer entries across all 8 initcall levels.
- Suffixed initcall sections (`.initcall.*\.init\.`): `0`.
Recorded in `progress-t696.txt` at 2026-11-07T22:50:25Z.

### Step 4 — Flight on Mac QEMU 11.1.2 (`t696-flight-01`): PASS

Supervisor `scripts/supervisor_t696.py` executed flight `t696-flight-01`:
- Kernel: `guest-inputs/routeB-Image` (`3f45272f…`)
- DTB: `guest-inputs/modified.dtb` (`e83a6437…`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (`8fb3c9de…`) via loader at `0x110000000`
- System: `guest-inputs/system.img` (`98fc0b02…`)
- Drive id: `t696system`
- GDB port: 1238
- Machine shape: `virt,gic-version=2,virtio-mmio-transports=1, -cpu cortex-a57, -m 8192, -smp 4`
- Host elapsed: 1202.89 s
- Guest max time: 1200.991 s (target 1200 s reached cleanly)
- QEMU exit code: 0
- UART lines: 51,156 lines
- UART SHA-256: `7d6d4107e73508bc7434f5b23b893e6215e77e07e3724993480fe3094791b70c`
- Preserved at `flights/t696-flight-01/uart.log` and `diagnostics/t696-routeB-uart.log`.

Key markers observed:
1. `[    0.323750] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!` (guest 0.323750 s)
2. `[   27.151864] Freeing unused kernel memory: 960K` (guest 27.151864 s)
3. `[   30.236134] init: init first stage started!` (guest 30.236134 s)
4. `[   36.272276] init: init second stage started!` (guest 36.272276 s)
5. Services running: `servicemanager`, `hwservicemanager`, `gatekeeperd`, `gnss`, `fingerprint`, `sensors_hal`, `netd`, `zygote` (pid 20369).
6. Periodic `T642_BOOT_STATE` watcher active through guest uptime 998.78 s.
7. Zero hang at ~25 s, zero `udf #0x0`, zero Oops, zero panic. Specifically, zero `Attempted to kill init!` panic.
Recorded in `progress-t696.txt` at 2026-11-07T23:12:55Z.

### Step 5 — A/B Control Flight on Canonical Image (`t696-flight-ab`): PASS

Supervisor `scripts/supervisor_t696.py` executed flight `t696-flight-ab`:
- Kernel: canonical `guest-inputs/Image` (`05c43917…`)
- DTB: `guest-inputs/modified.dtb` (`e83a6437…`)
- Initramfs: `guest-inputs/base-t683-fix.cpio.gz` (`8fb3c9de…`)
- System: `guest-inputs/system.img` (`98fc0b02…`)
- Drive id: `t696system`
- GDB port: 1239
- Host elapsed: 245.53 s
- Guest max time: 242.623 s (target 240 s reached cleanly)
- QEMU exit code: 0
- UART lines: 12,260 lines
- UART SHA-256: `39466229c0139abc52e2b22a82aab4ac1186bdda6e4d04ad5a1f5afcbaca6696`
- Preserved at `flights/t696-flight-ab/uart.log` and `diagnostics/t696-canonical-ab-uart.log`.

Key markers observed:
1. `[   26.114348] Freeing unused kernel memory: 1024K` (guest 26.114348 s)
2. `[   29.243520] init: init first stage started!` (guest 29.243520 s)
3. `[   35.479436] init: init second stage started!` (guest 35.479436 s)
4. Zero panic / oops / hangs.
Recorded in `progress-t696.txt` at 2026-11-07T23:19:30Z.
This re-proves the Mac QEMU 11.1.2 harness 100% sound.

---

## Step 6 — Adjudication

### Verdict
Verdict: PASS

### Symbol Gate (3a)
PASS. All five key functions strictly below `_etext` `0xffffff8008b50000`:
- `printk`: `0xffffff8008b30a40`
- `__die`: `0xffffff8008b2ee50`
- `panic`: `0xffffff8008b2f4dc`
- `dump_stack`: `0xffffff8008b37ecc`
- `show_regs`: `0xffffff8008b2edd8`
- `free_initmem`: `0xffffff80080a4200`
- `_etext`: `0xffffff8008b50000`
- `__init_begin`: `0xffffff8009180000`
- `__init_end`: `0xffffff8009270000`

### Initcall Table Gate (3b)
PASS.
- `__initcall_start`: `0xffffff8009250b48`
- `__initcall_end`: `0xffffff800925165c`
- Table size: `2,836 bytes` (354 function pointer entries).
- Orphaned suffixed sections count: `0`.

### T686 Defect Resolution
The cold `.text.unlikely.` orphan defect that caused the T686 boot hang (zeroed `printk` executing `udf #0x0`) is removed by the Route B linker script globs. Symbol gate confirmed `printk` below `_etext`, and flight `t696-flight-01` reached `Freeing unused kernel memory: 960K` at 27.151864 s and userspace `init first stage` at 30.236134 s with zero `udf #0x0` or lock deadlocks.

### Route A (T695) Defect Resolution
The ThinLTO initcall-suffixing defect that emptied the initcall table in Route A is removed by keeping `CONFIG_LTO_NONE=y`. Gate 3b confirmed a fully populated 2,836-byte initcall table with 0 suffixed sections. Flight `t696-flight-01` successfully ran all device drivers and rootfs initialization without early panic.

### sys.boot_completed
`sys.boot_completed: not observed (0 / empty)` across all flights and cycles.

### Display / Frame Commit
No frame-commit claim: this lane proves the kernel reaches userspace with `fb0: simplefb registered!` (at guest 0.323750 s). Frame commit by guest-origin evidence belongs to the successor lane.

### Reproducibility (TEN-PASSES)
TEN-PASSES was NOT run in this lane (one Route B flight to 1200 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

### Regressions Check
- `guest-inputs/modified.dtb` (`e83a6437…`): untouched and verified.
- `guest-inputs/base-t683-fix.cpio.gz` (`8fb3c9de…`): untouched and verified.
- `guest-inputs/system.img` (`98fc0b02…`): untouched and verified.
- `guest-inputs/android.hardware.graphics.composer@2.1-service` (`ddc1e4dd…`): untouched and verified.
- `fb0: simplefb registered!` binds cleanly at guest 0.323750 s.
- T683 high-RAM relocation fix holds (zero inflate/unpack oops).

### QEMU Version String (Verbatim)
```
QEMU emulator version 11.1.2
Copyright (c) 2003-2026 Fabrice Bellard and the QEMU Project developers
```

### Supervisor Adaptations (from `scripts/supervisor_t695.py`)
1. Default kernel set to `guest-inputs/routeB-Image` (SHA-256 `3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3`).
2. QEMU drive ID and device changed from `t695system` to `t696system`.
3. Kernel command-line append updated with `androidboot.t696_kernel_sha256`.
4. Host cap and guest target defaulted to 1200 s (overridable via `T696_HOST_CAP` / `T696_GUEST_TARGET`).
5. Unique TCP GDB port per flight (`1238` for `t696-flight-01`, `1239` for `t696-flight-ab`).
6. Track constants, flight naming, and environment variables updated to T696.

---

## Preserved Evidence Inventory

All register values, PCs, addresses, and SHAs cited in this report appear verbatim in the following preserved files:

```
7d6d4107e73508bc7434f5b23b893e6215e77e07e3724993480fe3094791b70c     7832999  flights/t696-flight-01/uart.log
79c8460eb5e8921465dfe9115a2661f52e13459399562a94cf36a3ebb8f5668a        1510  flights/t696-flight-01/flight-end.json
39466229c0139abc52e2b22a82aab4ac1186bdda6e4d04ad5a1f5afcbaca6696     1887464  flights/t696-flight-ab/uart.log
be60126b043f410c065cac7e0f613415ab1a8f6e4d19b8a4b02283e8f7011fb5        1453  flights/t696-flight-ab/flight-end.json
7db3209d6501c57ebb8963a1c441685e6b0e7f87625ab79fd6e54cfcac0cf733         255  diagnostics/t696-nm-symbols.txt
efa24785124f1226a453f5a4ec558db5f48703b747aab1e6a0dd855dbde17aea         458  diagnostics/t696-initcall-table.txt
c210e961437a343de4f5d1a49f13f16b70af6872fd26c2579406ee388098f784         665  diagnostics/t696-routeB-git-diff.txt
925ad4c677a418f48df2339a74a083583a0df366223f453783d44624ecde5471          77  diagnostics/t696-build.log
7d6d4107e73508bc7434f5b23b893e6215e77e07e3724993480fe3094791b70c     7832999  diagnostics/t696-routeB-uart.log
39466229c0139abc52e2b22a82aab4ac1186bdda6e4d04ad5a1f5afcbaca6696     1887464  diagnostics/t696-canonical-ab-uart.log
3f45272f003365bac026c71cec7baa7c48b588879fe03a0366251e26022b27f3    20126208  guest-inputs/routeB-Image
```

Specific citations:
- `fb0: simplefb registered!`: `flights/t696-flight-01/uart.log` line 125 (`[    0.323750] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!`).
- Route B `Freeing unused kernel memory`: `flights/t696-flight-01/uart.log` line 157 (`[   27.151864] Freeing unused kernel memory: 960K`).
- Route B `init first stage started!`: `flights/t696-flight-01/uart.log` line 568 (`[   30.236134] init: init first stage started!`).
- Route B `init second stage started!`: `flights/t696-flight-01/uart.log` line 701 (`[   36.272276] init: init second stage started!`).
- Canonical A/B `Freeing unused kernel memory`: `flights/t696-flight-ab/uart.log` line 186 (`[   26.114348] Freeing unused kernel memory: 1024K`).
- Canonical A/B `init first stage started!`: `flights/t696-flight-ab/uart.log` line 597 (`[   29.243520] init: init first stage started!`).
- Gate 3a symbols: `diagnostics/t696-nm-symbols.txt`.
- Gate 3b initcall table: `diagnostics/t696-initcall-table.txt`.
- Route B `git diff`: `diagnostics/t696-routeB-git-diff.txt`.
