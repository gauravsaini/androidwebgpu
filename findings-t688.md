# T688 — T686 rebuilt-kernel boot-hang diagnosis

Date: 2026-11-07
Worktree: `/Users/Shared/wt-track688-kernel-boot-hang-diagnosis`
Branch: `t688`, from `712e9fb` (the T687 commit)
Predecessor: T687 driver-certified FAIL — DTB half solid, kernel half broken.

## Verdict

Verdict: PASS

## Required markers

sys.boot_completed: not observed (0 / empty) — no `sys.boot_completed=1`
line appears in any T688 UART. The T686 diagnostic boots never reached
userspace. The canonical A/B boot reached init first/second stage and
printed `T642_BOOT_STATE ... boot_completed=` empty at guest uptimes
148.22 s, 175.60 s, 199.15 s and 224.33 s; the flight ended at guest
241.806 s without a completed Android boot. No cycle in this lane observed
`sys.boot_completed=1`.

WALL-TYPE-TAG: none — kernel boot hang root-caused
Rust par bhi lagu: haan — the same guest kernel is required regardless of
implementation language; the fix is a guest-kernel section-placement fix,
not a language/runtime fix.

## First failing link

First failing link: the T686 Image's cold-function text
(`.text.unlikely.`) was orphan-placed inside `[__init_begin, __init_end]`;
`free_initmem()` zeroes that range; the `bl printk` at
`free_reserved_area+420` lands on `udf #0x0`; the nested `die()` then
self-deadlocks on `die_lock`.

## Executive summary

The T686 kernel does not hang in a driver, a lock or the DTB. It executes
correct code until `free_initmem()`, then jumps into kernel text that
`free_initmem()` has just zeroed.

The T686 Image (clang 21.0.0 / LLD 21.0.0) placed its cold functions —
including `printk`, `__die`, `panic`, `dump_stack` and `show_regs` — inside
the freeable init range `[0xffffff8009170000, 0xffffff8009270000)`.
`free_initmem()` calls `free_reserved_area()` with poison `0` over exactly
that range, so every instruction there becomes `0x00000000`, which AArch64
decodes as `udf #0x0`.

The first executed zeroed instruction is the `printk` called by
`free_reserved_area+420` while printing the "Freeing unused kernel memory"
message. That undefined-instruction exception enters `die()` and takes
`die_lock`. `die()` calls `__die()`, which is also in the zeroed range, so
it faults again. The second `die()` tries to take the same `die_lock` and
spins forever. Only CPU0 is online, so this is a re-entrant self-deadlock,
not cross-CPU contention. The console is dead because `printk` itself was
zeroed.

The cold-section name is the compiler-side cause. Linux 4.19's
`TEXT_TEXT` linker-script macro matches `.text.unlikely` exactly. Clang 12+
(LLVM D92493 / rG3733463dbb58, 2020-12) emits `.text.unlikely.` with a
trailing dot; clang 21 therefore emits a section the 4.19 script cannot
claim, and LLD orphan-places it into the init range. The canonical kernel
in the A/B control is clang 11.0.1, predates that naming change, and places
`printk` below `_etext`; it boots through the same 25 s point to userspace
with identical DTB, initramfs, system image and QEMU shape.

## Step 0 — Mac disk gate: PASS

`df -h /System/Volumes/Data` at start: 18 GiB available, above the 8 GiB
gate. No WAIT-DISK. Recorded in `progress-t688.txt` at
2026-11-07T00:53:34Z.

## Step 1 — Inputs verified: PASS

All eight staged inputs re-verified against the brief:

```
66d3fc1b6832a6caa66cd26bbb4417c9ef1f7a6c017dfc44a93063b8ff2813dc  guest-inputs/t686-Image
05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04  guest-inputs/Image
f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0  guest-inputs/minimal-virt-fixed.dtb
e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389  modified.dtb
98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d  guest-inputs/system.img
8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70  guest-inputs/base-t683-fix.cpio.gz
bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1  guest-inputs/vendor-t683.img
ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132  guest-inputs/android.hardware.graphics.composer@2.1-service
```

T687 findings, death note, flight-04 UART, base-DTB control, initcall log
and oops logs were read in full. No input wall.

The T686 final `.config` artifact (recorded SHA-256
`502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e`)
is **not present** in this Mac worktree; only the recorded delta in
`findings-t686.md` is available. This is an evidence gap, named in Step 5.

## Step 2 — Hang reproduced: PASS

Four T688 diagnostic flights used the T686 Image
(`66d3fc1b…`), `modified.dtb` (`e83a6437…`), the T683 fix initramfs
(`8fb3c9de…`) and the canonical `system.img` (`98fc0b02…`) under QEMU
11.1.2 with `-smp 4`, fully detached.

| flight | host s | guest s | last guest line | UART SHA-256 |
|---|---:|---:|---|---|
| t688-flight-01 | 301.01 | 26.016 | `[   26.015644] uart-pl011 9000000.pl011: no DMA platform data` | `8f9b7b8813a2f92fafbdb432db24ea309f939485a9cd77ffceeb2b62ad60dfc1` |
| t688-flight-02 | 180.64 | 26.941 | same uart-pl011 signature | `fb7039be635cd538a42dfc2fb423c47264f98e5d0c138f8459b159bcb8b59048` |
| t688-flight-03 | 301.04 | 24.225 | same uart-pl011 signature | `9e2823c65d1143f13457d8c28865c8010840785cb051d51cb397d5944aa4336b` |
| t688-flight-04 | 301.00 | 25.638 | `[   25.638402] uart-pl011 9000000.pl011: no DMA platform data` | `2d8c4e23a56986aa840329e0bdf952017ded1fdf7ca08c2e431686e35b2d9a7c` |

Primary preserved capture: `diagnostics/t688-repro-uart.log` (185 lines,
SHA-256 `8f9b7b88…`). It contains zero panic/oops/`Unable to handle`/
`inflate_fast` lines. It never prints `Freeing unused kernel memory`.

The T686 Image is clang 21.0.0 / LLD 21.0.0, verbatim from
`diagnostics/t688-repro-uart.log` line 2:

```
[    0.000000] Linux version 4.19.130-g4da740c10 (muse@gsai) (Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0 (https://android.googlesource.com/toolchain/llvm-project 5e96669f06077099aa41290cdb4c5e6fa0f59349), LLD 21.0.0 (/mnt/disks/build-disk/src/googleplex-android/llvm-r563880-release/out/llvm-project/llvm 5e96669f06077099aa41290cdb4c5e6fa0f59349)) #2 SMP PREEMPT Sat Nov 7 09:23:51 AEDT 2026
```

Only one CPU is online, so the hang is a same-CPU path:

```
[    0.102424] smp: Brought up 1 node, 1 CPU
[    0.102634] SMP: Total of 1 processors activated.
```

The T687 framebuffer fix is still present on the hanging kernel — the
simple-framebuffer binds and registers `fb0` before the freeze:

```
[    0.333574] simple-framebuffer 5f000000.framebuffer: framebuffer at 0x5f000000, 0x384000 bytes, mapped to 0x(____ptrval____)
[    0.333996] simple-framebuffer 5f000000.framebuffer: format=a8b8g8r8, mode=1280x720x32, linelength=5120
[    0.334872] simple-framebuffer 5f000000.framebuffer: fb0: simplefb registered!
```

TEN-PASSES status: T688 observed 4/4 hangs; T687 observed 3 more hangs
(flights 03, 04, base-DTB control). T688 did not run 10 consecutive boots,
so this report claims "4/4 T688 boots and 7 total across T687/T688", not a
statistical 10/10 reproducibility claim.

## Step 3 — Multi-CPU lldb capture: PASS

Raw lldb output preserved verbatim at
`diagnostics/t688-lldb-all-cpus.log` (SHA-256 `db3cace1…`). Reconstructed
T686 symbol table preserved at `diagnostics/t688-kallsyms-symbols.txt`
(SHA-256 `c9cbfe18…`).

### (a) Which CPU is spinning, and at what PC

CPU0 is spinning in the qspinlock slow path:

```
* thread #1, stop reason = signal SIGTRAP
      frame #0: 0xffffff800810c7b4 t688-vmlinux.elf`queued_spin_lock_slowpath + 132
->  0xffffff800810c7b4 <+132>: ldar   w9, [x8]
```

Registers at the stop:

```
x1  = 0xffffff80093b514c
x8  = 0xffffff80093b514c
x9  = 0x0000000000000101
x30 = 0xffffff800810c780 ; queued_spin_lock_slowpath+80
sp  = 0xffffff800800b8e0
pc  = 0xffffff800810c7b4 ; queued_spin_lock_slowpath+132
```

CPUs 2–4 are parked at zero; they are not running and cannot hold the lock:

```
thread #2: tid = 0x0002, 0x0000000000000000
thread #3: tid = 0x0003, 0x0000000000000000
thread #4: tid = 0x0004, 0x0000000000000000
```

### (b) Lock word address and value

The lock word is `die_lock` at `0xffffff80093b514c` (also the symbol address
in `diagnostics/t688-kallsyms-symbols.txt`: `ffffff80093b514c t die_lock`).
Its value is `0x101`:

- bit 0 = 1: locked;
- bit 8 = 1: pending waiter;
- tail (bits 16–31) = 0: no other CPU queued.

Only CPU0 is online, so the holder can only be CPU0 itself.

### (c) Holder — identified

The holder is the **outer `die()` frame on CPU0**, i.e. this is a
re-entrant self-deadlock. The frame chain proves two nested `die()`
invocations (inner spinner at fp `0xffffff800800b8f0`, outer die at fp
`0xffffff800800bb90`):

```
=== FRAME CHAIN CURRENT (start fp=0xffffff800800b8f0) ===
  [0] fp=0xffffff800800b8f0 next=0xffffff800800b910 ret=die+40
  [1] fp=0xffffff800800b910 next=0xffffff800800b9d0 ret=force_signal_inject+204
  [2] fp=0xffffff800800b9d0 next=0xffffff800800ba20 ret=do_undefinstr+796
  [3] fp=0xffffff800800ba20 next=0xffffff800800bb80 ret=el1_undef+16
  [4] fp=0xffffff800800bb80 next=0xffffff800800bb90 ret=__die+0
  [5] fp=0xffffff800800bb90 next=0xffffff800800bc50 ret=force_signal_inject+204
  [6] fp=0xffffff800800bc50 next=0xffffff800800bca0 ret=do_undefinstr+796
  [7] fp=0xffffff800800bca0 next=0xffffff800800be00 ret=el1_undef+16
  [8] fp=0xffffff800800be00 next=0xffffff800800be20 ret=printk+0
  [9] fp=0xffffff800800be20 next=0xffffff800800be80 ret=free_initmem+80
 [10] fp=0xffffff800800be80 next=0xffffff800800bea0 ret=kernel_init+28
 [11] fp=0xffffff800800bea0 next=0x0000000000000000 ret=ret_from_fork+16
```

The outer fault (the `printk` call) is:

```
=== UNDEF-1 pt_regs @ 0xffffff800800bcd0 ===
  pc     = 0xffffff8009232ca0
  lr     = 0xffffff80081ec4bc
  sp     = 0xffffff800800be10
  x29    = 0xffffff800800be20
  pstate = 0x0000000060400005
  -> pc=printk+0
  -> lr=free_reserved_area+420
```

The inner fault (the second `die`'s `__die`) is:

```
=== UNDEF-0 pt_regs @ 0xffffff800800ba50 ===
  pc     = 0xffffff80092310b0
  lr     = 0xffffff800808fdf8
  sp     = 0xffffff800800bb90
  x29    = 0xffffff800800bb90
  pstate = 0x0000000060400085
  -> pc=__die+0
  -> lr=die+92
```

Live memory at both fault PCs is zeroed. The preserved lldb disassembly
reads `udf #0x0` ten times at each PC:

```
=== instructions at 0xffffff80092310b0 ===
  0xffffff80092310b0: udf #0x0
  ...

=== instructions at 0xffffff8009232ca0 ===
  0xffffff8009232ca0: udf #0x0
  ...
```

The same addresses in the on-disk Image hold correct code (symbolized as
`__die+0` and `printk+0`), so this is runtime zeroing, not a bad
disassembly.

## Step 4 — A/B control: PASS

The canonical Image (`05c43917…`, clang 11.0.1 / LLD 11.0.1) was booted
with the **same** `modified.dtb`, initramfs, system image, machine shape
and supervisor; host cap 300 s, guest target 240 s, no second relaunch
needed. Flight `t688-flight-ab`:

```
stop_reason: Guest target reached (241.8s >= 240s)
host_elapsed_seconds: 243.13
guest_log_max_seconds: 241.806
qemu_exit_code: 0
log_lines: 12116
log_sha256: 1017cffd4b2e610eb556e3daf0258a882a42157763c53bfa6bce94995a8581eb
```

The canonical kernel crosses the exact point where T686 dies:

```
[   25.990270] uart-pl011 9000000.pl011: no DMA platform data
[   26.095196] Freeing unused kernel memory: 1024K
[   29.207838] init: init first stage started!
[   35.339172] init: init second stage started!
```

It starts services and runs to guest 241.806 s with zero panic/oops. The
canonical Image's own version line confirms the toolchain:

```
[    0.000000] Linux version 4.19.130-00651-g4da740c10dc0-ab6640132 (android-build@abfarm-01180) (Android (6443078 based on r383902) clang version 11.0.1 (https://android.googlesource.com/toolchain/llvm-project b397f81060ce6d701042b782172ed13bee898b79), LLD 11.0.1 (/buildbot/tmp/tmp6_m7QH b397f81060ce6d701042b782172ed13bee898b79)) #2 SMP PREEMPT Tue Jun 30 06:03:01 UTC 2020
```

A/B conclusion: with everything else held constant, the kernel binary
delta is decisive. The DTB, initramfs and system image are not the wall.

## Step 5 — Config delta, placement and culprit: PASS

### Config delta (from the T686 record)

The T686 final `.config` artifact is missing locally. The recorded delta in
`findings-t686.md` (SHA-256 `7d37a3c5…`) is the authoritative substitute.
The relevant lines, one per line, T686 vs canonical:

```
T686  ADD: # Compiler: Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0
canon  DEL: # Compiler: Android (6443078 based on r383902) clang version 11.0.1
T686  ADD: CONFIG_CLANG_VERSION=210000
canon  DEL: CONFIG_CLANG_VERSION=110001
T686  ADD: CONFIG_LTO_NONE=y
canon  DEL: CONFIG_LTO=y
canon  DEL: CONFIG_THINLTO=y
canon  DEL: CONFIG_LTO_CLANG=y
canon  DEL: # CONFIG_LTO_NONE is not set
T686  ADD: # CONFIG_LTO_CLANG is not set
T686  DEL: CONFIG_CFI=y
T686  DEL: CONFIG_CFI_CLANG=y
T686  DEL: CONFIG_CFI_CLANG_SHADOW=y
T686  ADD: # CONFIG_SHADOW_CALL_STACK is not set
canon  DEL: CONFIG_SHADOW_CALL_STACK=y
T686  ADD: CONFIG_HAVE_ARCH_PREL32_RELOCATIONS=y
T686  DEL: CONFIG_UNUSED_KSYMS_WHITELIST="/buildbot/src/android/common-android-4.19-stable/out/android-4.19-stable/common/abi_whitelist.raw"
T686  ADD: CONFIG_UNUSED_KSYMS_WHITELIST=""
T686  ADD: CONFIG_DEVMEM=y
canon  DEL: # CONFIG_DEVMEM is not set
T686  ADD: CONFIG_DRM_FBDEV_EMULATION=y
canon  DEL: # CONFIG_DRM_FBDEV_EMULATION is not set
T686  ADD: CONFIG_FB=y
canon  DEL: # CONFIG_FB is not set
T686  ADD: CONFIG_FB_SIMPLE=y
```

`CONFIG_ARM64_LSE_ATOMICS=y` is unchanged on both sides. No lock-related
config line changes.

### Symbol placement A/B

Preserved in `diagnostics/t688-symbol-placement-ab.txt`:

```
T686: __init_begin=0xffffff8009170000 __init_end=0xffffff8009270000 _etext=0xffffff8008b40000
  printk       0xffffff8009232ca0 INSIDE-INIT
  __die        0xffffff80092310b0 INSIDE-INIT
  die          0xffffff800808fd9c below-etext
  panic        0xffffff800923173c INSIDE-INIT
  dump_stack   0xffffff800923a12c INSIDE-INIT
  show_regs    0xffffff8009231038 INSIDE-INIT

canonical: __init_begin=0xffffff8009440000 __init_end=0xffffff8009540000 _etext=0xffffff8008ca0000
  printk       0xffffff8008216980 below-etext
  die          0xffffff800818cef4 below-etext
  panic        0xffffff80081ad5b4 below-etext
  dump_stack   0xffffff8008c6382c below-etext
  show_regs    0xffffff8008184568 below-etext
```

`die()` itself is below `_etext` on both kernels; its cold helper `__die()`
is the one that moved into the T686 init range. That is exactly the nested
fault path: the outer `die()` survives, `__die()` does not.

### Zeroing proof

`diagnostics/t688-free-initmem-disassembly.txt` preserves the disassembly:

```
ffffff80080a4200 <free_initmem>:
  ...
  adrp  x19, 0xffffff8009170000 <stext>
  add   x19, x19, #0x0
  ...
  adrp  x20, 0xffffff8009270000 <init_thread_union>
  add   x20, x20, #0x0
  ...
  mov   w2, wzr
  ...
  bl    0xffffff80081ec318 <free_reserved_area>
  ...
  bl    0xffffff800822d130 <unmap_kernel_range>
```

`free_reserved_area` memsets each page (poison `w2=0`) at
`0xffffff80081ec41c <free_reserved_area+0x104>`, then prints at
`0xffffff80081ec4b8`:

```
ffffff80081ec4b8: 944119fa      bl    0xffffff8009232ca0 <printk>
ffffff80081ec4bc: 17fffff3      b     0xffffff80081ec488 <free_reserved_area+0x170>
```

`0xffffff80081ec4bc` is exactly the outer fault's LR
(`free_reserved_area+420`), and `0xffffff8009232ca0` is exactly the outer
fault's PC (`printk+0`) — the first zeroed instruction executed.

### Section-name proof

Preserved in `diagnostics/t688-clang-cold-section-naming.txt` and
`diagnostics/t688-linker-script-v4.19-excerpt.txt`.

The v4.19 script matches only these hot/cold names:

```
*(.text.hot TEXT_MAIN .text.fixup .text.unlikely)
*(.text..refcount)
*(.ref.text)
```

`TEXT_MAIN` is exactly `.text` because the canonical config does not enable
`CONFIG_LD_DEAD_CODE_DATA_ELIMINATION`. There is no `.text.unlikely.*` or
`.text.hot.*` wildcard.

The compiler test (Homebrew clang 23.1.3, the only local clang; same
post-LLVM-12 behavior as clang 21) shows:

```
  2 .text           00000008 ...
  3 .text.unlikely. 00000008 ...
  4 .init.text      00000008 ...
```

and with `-ffunction-sections`:

```
  3 .text.unlikely.cold_fn 00000008 ...
```

The emitted assembly directive is:

```
.section .text.unlikely.,"ax",@progbits
```

Match test against the v4.19 patterns:

```
.text.unlikely.          matched_by=NONE (orphan)
.text.hot.               matched_by=NONE (orphan)
.text.unlikely.cold_fn   matched_by=NONE (orphan)
.ref.text                matched_by=['.ref.text']
```

LLVM D92493 / rG3733463dbb58 (2020-12) introduced the trailing-dot names;
clang 11.0.1 predates it. The canonical kernel's correct placement
(`printk` at `0xffffff8008216980`, below `_etext` `0xffffff8008ca0000`) is
consistent with the old exact-name match.

### Config culprit

Primary: `CONFIG_CLANG_VERSION=210000` (T686) vs `110001` (canonical) —
the clang 21 toolchain emits `.text.unlikely.`/`.text.hot.` trailing-dot
section names that the 4.19 `TEXT_TEXT` macro cannot match. This is a
literal line in the recorded config delta.

Secondary/config-level test: `CONFIG_LTO_NONE=y` (T686) vs the canonical
LTO trio. T686 runs clang 21 without LTO; the canonical clang 11 build runs
LTO. LTO-off is not proven to change the section name, so the rebuild
recipe tests restoring the LTO trio as the minimal config-only experiment,
with an explicit symbol-placement gate; if the gate fails, the linker-script
glob is the deterministic fix.

Excluded by the evidence:

- `CONFIG_DEVMEM`, `CONFIG_FB`, `CONFIG_FB_SIMPLE`,
  `CONFIG_DRM_FBDEV_EMULATION` — the T687 base-DTB control hangs identically
  with no framebuffer node, and the canonical A/B kernel without these
  options boots to userspace.
- `CONFIG_ARM64_LSE_ATOMICS` — unchanged on both sides.
- `CONFIG_CFI*`, `CONFIG_SHADOW_CALL_STACK` — hardening features; their
  removal follows LTO-off and does not rename text sections.
- `CONFIG_UNUSED_KSYMS_WHITELIST` — build-time path only.
- `CONFIG_COMPAT_VDSO` removal / `CONFIG_HAVE_ARCH_PREL32_RELOCATIONS`
  addition — derived config consequences, not section-placement controls.
- Locking code — no lock config delta; `die_lock` is a static raw spinlock
  in `kernel/panic.c`; the spin is a symptom of the nested fault.

### Rebuild recipe

`rebuild-recipe.md` (SHA-256 `63ace645…`) contains the executable recipe:
exact source commit, exact T686 toolchain, exact build command, Route A
(config-only LTO restore), Route B (recommended linker-script glob),
Route C (canonical clang 11 fallback), the mandatory pre-boot symbol gate,
the boot gate and the TEN-PASSES gate. No rebuild was performed in T688.

## Step 6 — Adjudication

### Lock identity and holder

- Lock: `die_lock` at `0xffffff80093b514c`, value `0x101`.
- Guards: serialization of `die()`/oops reporting in `kernel/panic.c`.
- Holder: the outer `die()` frame on CPU0 at fp
  `0xffffff800800bb90` (return address `force_signal_inject+204`).
- Spinner: the inner `die()` on the same CPU at fp
  `0xffffff800800b8f0` (return address `die+40`), inside
  `queued_spin_lock_slowpath+132` at `0xffffff800810c7b4`.
- CPUs 2–4: PC/SP/LR all `0` (parked/never online; the guest reports
  `smp: Brought up 1 node, 1 CPU`).
- Conclusion: re-entrant self-deadlock, not cross-CPU contention.

### First failing link (one line)

First failing link: T686 Image cold-function text orphaned inside
`[__init_begin,__init_end]` → `free_initmem()` zeroes it → `bl printk`
executes `udf #0x0` → nested `die()` spins on `die_lock`.

### sys.boot_completed

sys.boot_completed: not observed (0 / empty). The T686 boots never reached
userspace; the canonical A/B boot reached init first/second stage and
`T642_BOOT_STATE` at guest 224.33 s still printed `boot_completed=` empty.
No `sys.boot_completed=1` was observed in any cycle in this lane.

### No frame-commit claim

The display wall is unassessable in this lane. The T686 kernel registers
`fb0: simplefb registered!` before it dies, but never reaches userspace, so
no guest process can commit a frame. The canonical A/B boot reaches
userspace but its config has no `CONFIG_FB`, so init only creates a tmpfs
`/dev/fb0` node; that is not a real framebuffer and not frame commit. No
frame-commit progress is claimed.

### Regressions

- DTB: `modified.dtb` SHA-256 `e83a6437…` and
  `minimal-virt-fixed.dtb` SHA-256 `f5f7e340…` re-verified; untouched.
- Initramfs: `base-t683-fix.cpio.gz` SHA-256 `8fb3c9de…` re-verified;
  untouched. The T686 repro shows zero `inflate_fast`/oops/panic, so the
  T683 high-RAM relocation fix still holds.
- System image: `system.img` SHA-256 `98fc0b02…` re-verified; untouched.
- Composer ELF: SHA-256 `ddc1e4dd…` re-verified; untouched.
- T687 framebuffer fix: still binds — `simple-framebuffer 5f000000…` and
  `fb0: simplefb registered!` appear at guest 0.333 s on the T686 repro.
- T686 rebuild inputs: T686 Image `66d3fc1b…` and canonical Image
  `05c43917…` re-verified; no rebuild, no DTB rebuild, no initramfs
  rebuild in this lane.

## Evidence inventory

All register values, PCs and stack lines cited above appear verbatim in the
following preserved files:

```
1017cffd4b2e610eb556e3daf0258a882a42157763c53bfa6bce94995a8581eb  diagnostics/t688-ab-canonical-uart.log
db3cace18292d6428ab74e79c206c0f648976c47181e23e3569f4541309cbc18  diagnostics/t688-lldb-all-cpus.log
8f9b7b8813a2f92fafbdb432db24ea309f939485a9cd77ffceeb2b62ad60dfc1  diagnostics/t688-repro-uart.log
2d8c4e23a56986aa840329e0bdf952017ded1fdf7ca08c2e431686e35b2d9a7c  diagnostics/t688-stack-uart.log
c9cbfe1882cf87117f80cbf757c762b1141dd7128172d314d2dece6a8ce30651  diagnostics/t688-kallsyms-symbols.txt
d07efcc0fb669e7c59a431ee2f676e57ffa6bf150ef6e13a01ba5a856571a4cf  diagnostics/t688-free-initmem-disassembly.txt
6fc618d7dcbc4f0f95934d3673c6bc591c96772df155e441bdf718c503dc07f0  diagnostics/t688-symbol-placement-ab.txt
5d5a43624a683936fce4642c250a18c640c7da5aae031a7af825240b73dc9279  diagnostics/t688-clang-cold-section-naming.txt
aa9815605437c4ab2b2e009bcded36a72479a627992a2c813e66c0a57cff3f25  diagnostics/t688-linker-script-v4.19-excerpt.txt
```

Flight records: `flights/t688-flight-01/`, `-02/`, `-03/`, `-04/`,
`t688-flight-ab/` (each has `flight-end.json` with host/guest times, QEMU
exit code and UART SHA-256).

## Honest gaps

1. The exact T686 `.config` artifact (SHA `502e79c3…`) is not present in
   this worktree. The config comparison uses the recorded delta in
   `findings-t686.md`; the final config SHA cannot be re-verified here.
2. The T686 build's linker map, object files and `out/vmlinux` section
   headers are not available. The orphan-placement conclusion rests on the
   preserved T686 symbol addresses (all cold helpers inside the init range),
   the live faults at those addresses, the v4.19 script patterns, and the
   compiler section test. The exact LLD placement decision is not logged.
3. TEN-PASSES was not run in T688: 4/4 T688 boots hung, 7 total across
   T687/T688. No 10/10 claim is made.
