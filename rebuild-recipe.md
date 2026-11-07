# T688 — rebuild recipe for the next lane (T689)

Status: recipe only. T688 did **not** rebuild anything. DTB, initramfs and
system image are verified and must not be touched.

## What the diagnosis proved

The T686 Image hangs because the **cold-function text section was
orphan-placed inside the freeable init range and then zeroed**.

- T686 Image (clang 21.0.0 / LLD 21.0.0) symbol layout
  (`diagnostics/t688-symbol-placement-ab.txt`):

  ```
  _etext       0xffffff8008b40000
  __init_begin 0xffffff8009170000
  show_regs    0xffffff8009231038   INSIDE-INIT
  __die        0xffffff80092310b0   INSIDE-INIT
  panic        0xffffff800923173c   INSIDE-INIT
  printk       0xffffff8009232ca0   INSIDE-INIT
  dump_stack   0xffffff800923a12c   INSIDE-INIT
  __init_end   0xffffff8009270000
  ```

- Canonical working Image (clang 11.0.1 / LLD 11.0.1) symbol layout, same
  fields:

  ```
  _etext       0xffffff8008ca0000
  printk       0xffffff8008216980   below-etext
  panic        0xffffff80081ad5b4   below-etext
  dump_stack   0xffffff8008c6382c   below-etext
  show_regs    0xffffff8008184568   below-etext
  die          0xffffff800818cef4   below-etext
  __init_begin 0xffffff8009440000
  __init_end   0xffffff8009540000
  ```

- `free_initmem` passes `[0xffffff8009170000, 0xffffff8009270000)` with
  poison `w2=0` to `free_reserved_area`, which memsets every page; the
  `bl printk` at `free_reserved_area+420` (`0xffffff80081ec4b8`) targets
  `0xffffff8009232ca0` (`printk+0`), now zeroed, so the CPU executes
  `udf #0x0`. Reproduced live: `diagnostics/t688-lldb-all-cpus.log`,
  `diagnostics/t688-free-initmem-disassembly.txt`.

- Why the section was orphaned: Linux 4.19 `TEXT_TEXT` matches
  `.text.unlikely` and `.text.hot` **exactly**. Clang 12+ (LLVM D92493 /
  rG3733463dbb58, 2020-12) emits `.text.unlikely.` and `.text.hot.`
  (trailing dot). T686 clang 21 emits the new name; clang 11 (canonical)
  predates the change. Evidence:
  `diagnostics/t688-clang-cold-section-naming.txt`,
  `diagnostics/t688-linker-script-v4.19-excerpt.txt`.

- A/B control: canonical Image `05c43917…` with the **same** modified.dtb,
  initramfs, system.img and QEMU shape printed
  `[   26.095196] Freeing unused kernel memory: 1024K` and
  `[   29.207838] init: init first stage started!` and ran to guest 241.806 s.
  T686 Image in the same shape stops at
  `[   26.015644] uart-pl011 9000000.pl011: no DMA platform data` with zero
  panic/oops. Evidence: `diagnostics/t688-repro-uart.log`,
  `diagnostics/t688-ab-canonical-uart.log`.

- Config delta that matters: `CONFIG_CLANG_VERSION=210000` (T686) vs
  `110001` (canonical). `CONFIG_LTO_NONE=y` replaced the LTO trio in the
  T686 config; CFI and SHADOW_CALL_STACK disappeared with it. The evidence
  makes the **compiler's section-name change** the culprit, not locking
  code, not DEVMEM/FB, and not the DTB.

## Source base (identical to T686)

- Repo: `https://android.googlesource.com/kernel/common`
- Branch: `deprecated/android-4.19`
- Exact commit: `4da740c10dc0f2de632686e8cc0274cfa52e67e4`
  (Linux 4.19.130; `git rev-parse HEAD` must print this SHA).
- No tracked source edits in T686. For Route B below there is exactly one
  source edit (linker script).

## Toolchain (T686 exact, known present on the box)

- NDK r29: `/home/muse/t681-link/toolchain/android-ndk-r29`
- clang: `Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c)
  clang version 21.0.0`
- LLD 21.0.0, llvm-ar, llvm-nm, llvm-objcopy, llvm-objdump, llvm-strip
  from the same NDK.
- `/home/muse/t686-kernel/toolchain-bin` with
  `as -> /usr/bin/aarch64-linux-gnu-as` and the `aarch64-linux-gnu-*`
  binutils symlinks (required because 4.19 passes `-no-integrated-as`).
- Do **not** use the NDK's Android-target clang wrapper (the tree rejects
  an Android `--target`); use generic NDK clang with
  `--target=aarch64-linux-gnu`.

## Config

Base: preserved canonical config
`/Users/Shared/preserved-t667/extracted_kernel.config`
(SHA-256 `d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8`).

T686 final config SHA-256:
`502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e`.
That artifact is **not** present in this worktree; regenerate from the
canonical config plus the recorded T686 delta in `findings-t686.md`.
The four intended framebuffer lines are:

```
CONFIG_DEVMEM=y
CONFIG_DRM_FBDEV_EMULATION=y
CONFIG_FB=y
CONFIG_FB_SIMPLE=y
```

Build-environment substitution (required):

```
CONFIG_UNUSED_KSYMS_WHITELIST=""
```

Keep the rest of the T686 final config. The recorded delta includes
`CONFIG_LTO_NONE=y`, `# CONFIG_LTO_CLANG is not set`,
`# CONFIG_SHADOW_CALL_STACK is not set`, and no CFI lines.

## Test order (do A, then B; C is fallback)

### Route A — config-only test: restore canonical LTO (cheap, decisive)

This is the minimal config change to test first. It checks whether clang
21's LTO codegen avoids the unmatched cold-section name. Evidence predicts
it will **not** fix the naming (the name is chosen in codegen, independent
of LTO), but the test is cheap and rules the config hypothesis in or out.

Set in `.config` (then run `make ... olddefconfig` and confirm the lines
survive):

```
CONFIG_LTO=y
CONFIG_THINLTO=y
# CONFIG_LTO_NONE is not set
CONFIG_LTO_CLANG=y
```

Optionally restore full canonical hardening parity:

```
CONFIG_CFI=y
# CONFIG_CFI_PERMISSIVE is not set
CONFIG_CFI_CLANG=y
CONFIG_CFI_CLANG_SHADOW=y
CONFIG_SHADOW_CALL_STACK=y
# CONFIG_SHADOW_CALL_STACK_VMAP is not set
```

Decision gate after build:

```
llvm-nm out/vmlinux | grep -E ' (printk|__die|panic|dump_stack|show_regs)$'
llvm-nm out/vmlinux | grep -E ' (_etext|__init_begin|__init_end)$'
```

- If all five key functions are **below `_etext`**: boot the Image. This is
  the config-only fix; record the winning config lines.
- If any key function is `>= __init_begin`: Route A failed. Do **not**
  boot; apply Route B.
- If the LTO build fails at compile/link (bitcode/plugin/LLD-21 issue):
  do not debug it; go to Route B.

### Route B — deterministic fix: teach the 4.19 linker script the glob

Recommended. One source edit, keep the T686 config unchanged
(`CONFIG_LTO_NONE=y`, clang 21).

File: `include/asm-generic/vmlinux.lds.h`, in `TEXT_TEXT`:

```
-		*(.text.hot TEXT_MAIN .text.fixup .text.unlikely)	\
-		*(.text..refcount)					\
-		*(.ref.text)						\
+		*(.text.hot .text.hot.* TEXT_MAIN .text.fixup)		\
+		*(.text.unlikely .text.unlikely.*)			\
+		*(.text.unknown .text.unknown.*)			\
+		*(.text..refcount)					\
+		*(.ref.text .ref.text.*)				\
```

Why each pattern:

- `.text.unlikely.*` matches clang's `.text.unlikely.` and
  `.text.unlikely.<function>`; this is the actual T686 orphan.
- `.text.hot.*` matches `.text.hot.` (same LLVM change, hot half).
- `.text.unknown.*` covers clang's unknown-profile bucket.
- `.ref.text.*` future-proofs the `__ref` section against suffixing.
- The old exact names stay, so older compilers keep working.

Do not edit the arm64 `vmlinux.lds.S` for this; `TEXT_TEXT` is the shared
macro it expands.

### Route C — fallback: canonical clang 11 + canonical config + 4 lines

If clang 21 cannot produce a bootable Image (Route A and B both fail),
rebuild on the toolchain that is proven by the A/B control:

- clang 11.0.1 / LLD 11.0.1 (AOSP r383902 / build 6443078; the toolchain
  named in the canonical Image version string). This toolchain is **not
  verified present on the box**; obtaining it is the cost of this route.
- Canonical config unchanged except:

  ```
  CONFIG_DEVMEM=y
  CONFIG_DRM_FBDEV_EMULATION=y
  CONFIG_FB=y
  CONFIG_FB_SIMPLE=y
  CONFIG_UNUSED_KSYMS_WHITELIST=""
  ```

- Keep `CONFIG_LTO=y`, `CONFIG_THINLTO=y`, `CONFIG_LTO_CLANG=y`,
  `CONFIG_CFI=y`, `CONFIG_SHADOW_CALL_STACK=y` exactly as canonical.
- This is the lowest-risk kernel: Image `05c43917…` from the same
  toolchain and config already boots to userspace in this lane's A/B.

## Exact build command (T686 shape)

```
make -j5 ARCH=arm64 \
  CROSS_COMPILE=/home/muse/t686-kernel/toolchain-bin/aarch64-linux-gnu- \
  CLANG_TRIPLE=aarch64-linux-gnu- \
  CC=clang LD=ld.lld AR=llvm-ar NM=llvm-nm \
  OBJCOPY=llvm-objcopy OBJDUMP=llvm-objdump STRIP=llvm-strip \
  HOSTCC=gcc HOSTCXX=g++ \
  KCFLAGS="-fno-builtin-sprintf" \
  Image
```

`KCFLAGS="-fno-builtin-sprintf"` is mandatory: clang 21 rewrites
`sprintf(p,"%s",...)` in `tty_io.c` to `stpcpy`, which 4.19 does not
provide. Do not remove it. `CONFIG_UNUSED_KSYMS_WHITELIST=""` avoids the
missing AOSP `/buildbot/.../abi_whitelist.raw` error.

## Pre-boot symbol gate (mandatory for Routes A and B)

```
llvm-nm out/vmlinux | grep -E ' (printk|__die|panic|dump_stack|show_regs|_etext|__init_begin|__init_end|free_initmem)$'
```

Required: every one of `printk`, `__die`, `panic`, `dump_stack`,
`show_regs` has an address **strictly below `_etext`** (and therefore below
`__init_begin`). If not, the Image will reproduce the T686 hang; do not
spend a flight on it.

Also record the new Image SHA-256 and the version string from the UART.

## Boot gate

Same shape as T688 Step 2/4 (QEMU 11.1.2):

- kernel: the new `Image`
- DTB: `modified.dtb` SHA-256
  `e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389`
- initramfs: `guest-inputs/base-t683-fix.cpio.gz` SHA-256
  `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70`
- system: `guest-inputs/system.img` SHA-256
  `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d`
- supervisor shape: `scripts/supervisor_t688.py`, fully detached
  (`setsid` + `nohup`, stdin `/dev/null`), host cap 300 s, guest target
  240 s, gdb port unique per flight.
- Do not modify the DTB, initramfs, system image, or Composer ELF.

Passing result, in order:

1. `Freeing unused kernel memory: ...` appears (T686 never reaches it).
2. `init: init first stage started!` (T686 never reaches it), then second
   stage/services.
3. No `uart-pl011 ... no DMA platform data`-then-silence hang; no
   `udf`/Oops/panic/die_lock spin.
4. `simple-framebuffer ... fb0: simplefb registered!` still appears (the
   framebuffer half of T686 must not regress).
5. TEN-PASSES: before calling the fix stable, run 10 consecutive boots
   with the same signature. A single pass proves the wall is broken; ten
   prove reproducibility.

## What T689 must not re-derive

- Spin lock: `die_lock` at `0xffffff80093b514c`, value `0x101`; holder is
  the outer `die()` frame on the same (only online) CPU0. It is a symptom
  of the zeroed `printk`/`__die`, not a locking bug.
- First failing link: cold-function text orphaned into
  `[__init_begin,__init_end]`, zeroed by `free_initmem`, then `printk`
  executes `udf #0x0`.
- Excluded: DTB (base-DTB control hangs identically in T687), initramfs
  (high-RAM relocation, zero inflate/oops), system image, Composer ELF,
  qspinlock code, `CONFIG_DEVMEM`/`FB*` (no causal path), and
  `CONFIG_ARM64_LSE_ATOMICS` (unchanged canonical value).
