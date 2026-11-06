# T686 — Canonical kernel rebuild with framebuffer support (box)

Date: 2026-11-07 (box UTC 2026-11-06/07)
Workdir: /home/muse/t686-kernel
Predecessor: T685 driver-certified honest FAIL (DTB/fb0 lead is a kernel wall)
Flight in this lane: none. Booting the rebuilt kernel is T687's job.

## Verdict

Verdict: PASS

The canonical guest kernel (Linux 4.19.130-00651-g4da740c10dc0-ab6640132) has
been rebuilt from the matching source commit with the missing kernel symbols
enabled: CONFIG_DEVMEM=y, CONFIG_FB=y, CONFIG_FB_SIMPLE=y, and
CONFIG_DRM_FBDEV_EMULATION=y. The resulting out/Image is a valid ARM64 kernel
Image and contains both simplefb and /dev/mem support. The T685 kernel wall is
removed at its named source: a kernel now exists that can service the T681
Composer's /dev/mem mapping and can consume the DTB simple-framebuffer node
that T687 will add.

## sys.boot_completed

sys.boot_completed: not observed — this lane runs no flight. T686 is a kernel
rebuild only; boot-marker observation belongs to T687 (the flight lane).

## Wall tags

WALL-TYPE-TAG: none — kernel rebuilt with framebuffer support
Rust par bhi lagu: haan — the same guest kernel is required regardless of
implementation language; the fix is a guest-kernel capability, not a
language/runtime fix.

(The original T685 wall was guest type-1. T686 does not prove a new wall; it
removes the guest-kernel half of the T685 wall.)

---

## Step 0 — Box disk gate: PASS

`df -h /home/muse` at start:

```
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda3       143G  117G   18G  87% /
```

18 GiB available, >= 8 GiB required. No WAIT-DISK. Workdir is under
/home/muse as required.

## Step 1 — Inputs verified and kernel identified: PASS

Re-verified both staged inputs:

```
05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04  Image
d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8  extracted_kernel.config
Image size  = 23,073,280 B   (matches brief)
config size = 133,733 B      (matches brief)
```

Both SHAs match the brief exactly. No mismatch; no STOP.

Kernel version string from `strings guest-inputs/Image` (verbatim):

```
Linux version 4.19.130-00651-g4da740c10dc0-ab6640132 (android-build@abfarm-01180) (Android (6443078 based on r383902) clang version 11.0.1 (https://android.googlesource.com/toolchain/llvm-project b397f81060ce6d701042b782172ed13bee898b79), LLD 11.0.1 (/buildbot/tmp/tmp6_m7QH b397f81060ce6d701042b782172ed13bee898b79)) #2 SMP PREEMPT Tue Jun 30 06:03:01 UTC 2020
```

Config header:

```
# Linux/arm64 4.19.130 Kernel Configuration
```

CONFIG_LOCALVERSION_AUTO=y (config line 25). Identification matches the
expected canonical kernel exactly.

Base-config wall lines (preserved T667 config):

```
1927:# CONFIG_DEVMEM is not set
2734:# CONFIG_DRM_FBDEV_EMULATION is not set
2770:# CONFIG_DRM_VIRTIO_GPU is not set
2830:# CONFIG_FB is not set
```

No CONFIG_FB_SIMPLE / CONFIG_DRM_SIMPLEDRM entries exist in the base config.

## Step 2 — Matching kernel source: PASS

- Remote: https://android.googlesource.com/kernel/common
- Requested branch `android-4.19` does not exist as a remote ref
  (`git ls-remote --heads` shows `deprecated/android-4.19` as the 4.19 branch).
- Branch used: `deprecated/android-4.19` (HEAD 280f881918f4355defc34f34f7bf0b7308597b7a,
  "Merge 4.19.132 into android-4.19").
- Exact canonical commit fetched by full SHA and checked out:
  `4da740c10dc0f2de632686e8cc0274cfa52e67e4`
  (gitiles/git: commit date 2020-06-29 18:26:31 +0000, subject
  `Revert "ANDROID: fscrypt: add key removal notifier chain"`).
- `git rev-parse HEAD` = 4da740c10dc0f2de632686e8cc0274cfa52e67e4.
- Top-level Makefile:
  `VERSION = 4`, `PATCHLEVEL = 19`, `SUBLEVEL = 130`, `EXTRAVERSION =` (empty)
  -> 4.19.130 tree confirmed.
- Checkout size: 1.1 GiB total (.git 192 MiB) — well under the 5 GiB cap.
- `git status --porcelain --untracked-files=no` is empty: no tracked source
  modifications. (Only build artifacts, generated files, and .config are
  untracked.)

## Step 3 — Config delta: PASS (with one documented substitution)

Base copy: `guest-inputs/extracted_kernel.config` -> `src/.config`
(baseline preserved as `config-baseline.txt`).

Explicit target delta applied with `scripts/config --enable`, then
`make ARCH=arm64 ... olddefconfig`:

```
1920:CONFIG_DEVMEM=y
2728:CONFIG_DRM_FBDEV_EMULATION=y
2827:CONFIG_FB=y
2879:CONFIG_FB_SIMPLE=y
```

These exact lines also appear in the delivered final config `out/.config`
(SHA-256 502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e),
lines quoted from out/.config:

```
1920:CONFIG_DEVMEM=y
2728:CONFIG_DRM_FBDEV_EMULATION=y
2827:CONFIG_FB=y
2879:CONFIG_FB_SIMPLE=y
```

### DRM_SIMPLEDRM substitution (evidence, not a guess)

CONFIG_DRM_SIMPLEDRM does not exist in this 4.19 tree. Evidence:

```
$ grep -rni 'simpledrm' --include='Kconfig*' --include='Makefile' .
(no matches)
$ grep -c 'CONFIG_DRM_SIMPLEDRM' .config
0
```

simpledrm is an upstream Linux 5.14+ driver; it is absent from 4.19. The
available route to /dev/fb0 for a DTB simple-framebuffer node is
CONFIG_FB_SIMPLE, which is fully enabled (=y). CONFIG_DRM=y and
CONFIG_DRM_KMS_HELPER=y were already enabled in the preserved config, so
CONFIG_DRM_FBDEV_EMULATION=y was also enabled as requested. There is no
simpledrm Kconfig symbol to enable, so it cannot appear in out/.config. This is
recorded as an evidence-backed substitution, not a config wall: the wall's
actual requirement (a real /dev/fb0 driver for the simple-framebuffer node)
is met by FB_SIMPLE.

### Build-enabling adjustment (not a Kconfig dependency)

The preserved config contained an AOSP build-host absolute path:

```
CONFIG_UNUSED_KSYMS_WHITELIST="/buildbot/src/android/common-android-4.19-stable/out/android-4.19-stable/common/abi_whitelist.raw"
```

That path does not exist on this box, and the first build failed with
`ERROR: '/buildbot/.../abi_whitelist.raw' whitelist file not found`
(include/generated/autoksyms.h). The in-tree `abi_gki_aarch64_whitelist` is a
different format (has an `[abi_whitelist]` header/comments) and is not the
generated `.raw` file. The path was therefore cleared:

```
CONFIG_UNUSED_KSYMS_WHITELIST=""
```

CONFIG_TRIM_UNUSED_KSYMS stays =y; `scripts/gen_autoksyms.sh` then uses
/dev/null for the whitelist, which is valid because only the built-in Image is
built (no modules), so there are no module-needed symbols to preserve. This
is a build-environment substitution and is listed in the change list.

### Every other config change, one line each

Generated by `diff config-baseline.txt out/.config` (also saved verbatim in
`config-changes-one-per-line-t686.txt` and `config-delta-t686.txt`). The full
list is below (ADD/DEL relative to baseline):

```
DEL: # Compiler: Android (6443078 based on r383902) clang version 11.0.1 (https://android.googlesource.com/toolchain/llvm-project b397f81060ce6d701042b782172ed13bee898b79)
ADD: # Compiler: Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0 (https://android.googlesource.com/toolchain/llvm-project 5e96669f06077099aa41290cdb4c5e6fa0f59349)
DEL: CONFIG_CLANG_VERSION=110001
ADD: CONFIG_CLANG_VERSION=210000
DEL: CONFIG_COMPAT_VDSO=y
DEL: CONFIG_LTO=y
DEL: CONFIG_THINLTO=y
DEL: # CONFIG_LTO_NONE is not set
DEL: CONFIG_LTO_CLANG=y
DEL: CONFIG_CFI=y
DEL: # CONFIG_CFI_PERMISSIVE is not set
DEL: CONFIG_CFI_CLANG=y
DEL: CONFIG_CFI_CLANG_SHADOW=y
ADD: CONFIG_LTO_NONE=y
ADD: # CONFIG_LTO_CLANG is not set
DEL: CONFIG_SHADOW_CALL_STACK=y
DEL: # CONFIG_SHADOW_CALL_STACK_VMAP is not set
ADD: # CONFIG_SHADOW_CALL_STACK is not set
ADD: CONFIG_HAVE_ARCH_PREL32_RELOCATIONS=y
DEL: CONFIG_UNUSED_KSYMS_WHITELIST="/buildbot/src/android/common-android-4.19-stable/out/android-4.19-stable/common/abi_whitelist.raw"
ADD: CONFIG_UNUSED_KSYMS_WHITELIST=""
DEL: # CONFIG_DEVMEM is not set
ADD: CONFIG_DEVMEM=y
DEL: # CONFIG_DRM_FBDEV_EMULATION is not set
ADD: CONFIG_DRM_KMS_FB_HELPER=y
ADD: CONFIG_DRM_FBDEV_EMULATION=y
ADD: CONFIG_DRM_FBDEV_OVERALLOC=100
ADD: # CONFIG_DRM_FBDEV_LEAK_PHYS_SMEM is not set
DEL: # CONFIG_FB is not set
ADD: CONFIG_FB_NOTIFY=y
ADD: CONFIG_FB=y
ADD: # CONFIG_FIRMWARE_EDID is not set
ADD: CONFIG_FB_CFB_FILLRECT=y
ADD: CONFIG_FB_CFB_COPYAREA=y
ADD: CONFIG_FB_CFB_IMAGEBLIT=y
ADD: CONFIG_FB_SYS_FILLRECT=y
ADD: CONFIG_FB_SYS_COPYAREA=y
ADD: CONFIG_FB_SYS_IMAGEBLIT=y
ADD: # CONFIG_FB_FOREIGN_ENDIAN is not set
ADD: CONFIG_FB_SYS_FOPS=y
ADD: CONFIG_FB_DEFERRED_IO=y
ADD: # CONFIG_FB_MODE_HELPERS is not set
ADD: # CONFIG_FB_TILEBLITTING is not set
ADD: 
ADD: #
ADD: # Frame buffer hardware drivers
ADD: #
ADD: # CONFIG_FB_CIRRUS is not set
ADD: # CONFIG_FB_PM2 is not set
ADD: # CONFIG_FB_ARMCLCD is not set
ADD: # CONFIG_FB_CYBER2000 is not set
ADD: # CONFIG_FB_ASILIANT is not set
ADD: # CONFIG_FB_IMSTT is not set
ADD: # CONFIG_FB_EFI is not set
ADD: # CONFIG_FB_OPENCORES is not set
ADD: # CONFIG_FB_S1D13XXX is not set
ADD: # CONFIG_FB_NVIDIA is not set
ADD: # CONFIG_FB_RIVA is not set
ADD: # CONFIG_FB_I740 is not set
ADD: # CONFIG_FB_MATROX is not set
ADD: # CONFIG_FB_RADEON is not set
ADD: # CONFIG_FB_ATY128 is not set
ADD: # CONFIG_FB_ATY is not set
ADD: # CONFIG_FB_S3 is not set
ADD: # CONFIG_FB_SAVAGE is not set
ADD: # CONFIG_FB_SIS is not set
ADD: # CONFIG_FB_NEOMAGIC is not set
ADD: # CONFIG_FB_KYRO is not set
ADD: # CONFIG_FB_3DFX is not set
ADD: # CONFIG_FB_VOODOO1 is not set
ADD: # CONFIG_FB_VT8623 is not set
ADD: # CONFIG_FB_TRIDENT is not set
ADD: # CONFIG_FB_ARK is not set
ADD: # CONFIG_FB_PM3 is not set
ADD: # CONFIG_FB_CARMINE is not set
ADD: # CONFIG_FB_SMSCUFX is not set
ADD: # CONFIG_FB_UDL is not set
ADD: # CONFIG_FB_IBM_GXT4500 is not set
ADD: # CONFIG_FB_VIRTUAL is not set
ADD: # CONFIG_FB_METRONOME is not set
ADD: # CONFIG_FB_MB862XX is not set
ADD: # CONFIG_FB_BROADSHEET is not set
ADD: CONFIG_FB_SIMPLE=y
ADD: # CONFIG_FB_SSD1307 is not set
ADD: # CONFIG_FB_SM712 is not set
ADD: # CONFIG_LOGO is not set
ADD: # CONFIG_FB_SM750 is not set
ADD: # CONFIG_FB_XGI is not set
ADD: # CONFIG_FB_TFT is not set
DEL: CONFIG_GENERIC_COMPAT_VDSO=y
ADD: CONFIG_HAVE_FUNCTION_GRAPH_TRACER=y
ADD: CONFIG_STRICT_DEVMEM=y
ADD: # CONFIG_IO_STRICT_DEVMEM is not set
```

Interpretation:
- Target/dependency additions: DEVMEM, FB, FB_SIMPLE, DRM_FBDEV_EMULATION,
  FB_NOTIFY, FB_CFB_*, FB_SYS_*, FB_DEFERRED_IO, DRM_KMS_FB_HELPER,
  DRM_FBDEV_OVERALLOC, STRICT_DEVMEM.
- Menu-visibility additions (all remain `not set`): the FB_* hardware driver
  entries, FB_EFI, FB_FOREIGN_ENDIAN, FB_MODE_HELPERS, FB_TILEBLITTING, LOGO,
  FIRMWARE_EDID, IO_STRICT_DEVMEM, DRM_FBDEV_LEAK_PHYS_SMEM. These options
  became visible because CONFIG_FB was enabled; none were enabled.
- Compiler re-resolution from clang 11 -> clang 21 (the build toolchain):
  CLANG_VERSION, LTO/THINLTO/LTO_CLANG -> LTO_NONE, CFI/CFI_CLANG/
  CFI_CLANG_SHADOW removed, SHADOW_CALL_STACK removed, COMPAT_VDSO/
  GENERIC_COMPAT_VDSO removed, HAVE_ARCH_PREL32_RELOCATIONS and
  HAVE_FUNCTION_GRAPH_TRACER added. These are Kconfig compiler-capability
  resolutions, not manual feature changes.
- Build-environment: UNUSED_KSYMS_WHITELIST path cleared as documented above.

No required symbol was blocked. There is no config wall.

## Step 4 — Build ARM64 Image: PASS

Toolchain used (first choice, present and re-verified):
- NDK r29 at /home/muse/t681-link/toolchain/android-ndk-r29 (read-only reuse)
- clang version string:
  `Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0 (https://android.googlesource.com/toolchain/llvm-project 5e96669f06077099aa41290cdb4c5e6fa0f59349)`
- LLD 21.0.0; AR/NM/OBJCOPY/etc. from the same NDK (llvm-ar, llvm-nm,
  llvm-objcopy, llvm-objdump, llvm-strip).
- 4.19's `-no-integrated-as` path needs a plain `as`; a small prefix dir
  /home/muse/t686-kernel/toolchain-bin was created with
  `as -> /usr/bin/aarch64-linux-gnu-as` (GNU binutils 2.45) plus the
  `aarch64-linux-gnu-*` binutils symlinks, and CROSS_COMPILE was pointed at it.
  This is a toolchain plumbing detail, not a source change.
- NDK's Android-target clang wrapper cannot be used directly: the tree rejects
  an Android --target (`Makefile:489 "Clang with Android --target detected"`).
  Generic NDK `clang` with `--target=aarch64-linux-gnu` is used instead.

Exact successful build command (also in build-clang.sh):

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

Build attempts and exact failures (recorded):
1. Missing AOSP ABI whitelist:
   `ERROR: '/buildbot/src/android/common-android-4.19-stable/out/android-4.19-stable/common/abi_whitelist.raw' whitelist file not found`
   -> fixed by clearing CONFIG_UNUSED_KSYMS_WHITELIST (Step 3).
2. External assembler was host `/usr/bin/as`:
   `/usr/bin/as: unrecognized option '-EL'` / `clang: error: assembler command failed`
   -> fixed by toolchain-bin prefix (aarch64-linux-gnu-as).
3. Final link: `ld.lld: error: undefined symbol: stpcpy`, referenced by
   `tty_io.c:1137 show_cons_active` (clang 21 rewrites `sprintf(p,"%s",...)`
   to stpcpy, absent from 4.19 lib/string.c).
   -> fixed without source edit by `KCFLAGS="-fno-builtin-sprintf"`; verified
   tty_io.o no longer references stpcpy.
4. Rebuild/link with the flag: exit 0.

Result: build exit code 0, target `Image`.
Elapsed: successful invocation 2026-11-06T22:19:16Z -> 2026-11-06T22:23:55Z
= 4m39s (incremental; the preceding full compile of all objects took
2026-11-06T22:13:33Z -> 22:18:40Z = 5m07s). Total including failed attempts
roughly 10 minutes, far inside the 3–4 hour budget.

Build log tail (`out/build.log`):

```
drivers/usb/gadget/function/f_accessory.c:1297:18: warning: variable 'dev' set but not used [-Wunused-but-set-variable]
 1297 |         struct acc_dev *dev;
      |                         ^
4 warnings generated.
BUILD_EXIT=0
```

Full log retained at `out/build.log`; failed-attempt log retained at
`out/build-clang-attempt2-stpcpy.log`.

## Step 5 — Verify the built kernel: PASS

Built Image staged at `out/Image`; final config at `out/.config`.

Version string from `strings out/Image` (verbatim; localversion differs from
canonical as permitted, base version and commit match):

```
Linux version 4.19.130-g4da740c10 (muse@gsai) (Android (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang version 21.0.0 (https://android.googlesource.com/toolchain/llvm-project 5e96669f06077099aa41290cdb4c5e6fa0f59349), LLD 21.0.0 (/mnt/disks/build-disk/src/googleplex-android/llvm-r563880-release/out/llvm-project/llvm 5e96669f06077099aa41290cdb4c5e6fa0f59349)) #2 SMP PREEMPT Sat Nov 7 09:23:51 AEDT 2026
```

Base `4.19.130` and commit `g4da740c10` match the canonical source; the
`-00651-` describe count and `-ab6640132` Android build id are not reproduced
because this is a plain one-commit checkout (localversion is
`4.19.130-g4da740c10`).

Final config lines (quoted from out/.config):

```
1920:CONFIG_DEVMEM=y
2728:CONFIG_DRM_FBDEV_EMULATION=y
2827:CONFIG_FB=y
2879:CONFIG_FB_SIMPLE=y
```

CONFIG_DRM_SIMPLEDRM is absent by design (symbol does not exist in 4.19; see
Step 3 evidence).

### simplefb / fbdev evidence in out/Image (strings, decimal offsets)

```
15415432 simple-framebuffer
17123278 simple-framebuffer
17289921 Unable to register simplefb: %d
17312359 fb%d: simplefb registered!
17081290 simplefb_clocks_get
17374814 simplefb_clocks_enable
17166984 fb%d: %s frame buffer device
```

Canonical Image comparison (re-confirmed this lane):

```
simplefb          canonical=0  built=5
simple-framebuffer canonical=0 built=2
simpledrm         canonical=0  built=0 (driver absent in 4.19)
virtio_gpu        canonical=0  built=0 (not enabled, as in canonical)
efi-framebuffer   canonical=1
```

### /dev/mem support evidence

Built kernel symbol table (`aarch64-linux-gnu-nm src/vmlinux`) contains the
drivers/char/mem.c devmem objects:

```
ffffff80080a4bf4 T devmem_is_allowed
ffffff8008f41870 d mem_fops
ffffff80085853cc t mmap_mem
ffffff8008f41e70 d mmap_mem_ops
ffffff800858508c t read_mem
ffffff8008585264 t write_mem
```

`drivers/char/mem.c` registers `/dev/mem` under `#ifdef CONFIG_DEVMEM`:
`#ifdef CONFIG_DEVMEM` ... `[1] = { "mem", 0, &mem_fops, FMODE_UNSIGNED_OFFSET }`.
Unique mem.c log string present in out/Image:

```
17483802 unable to get major %d for memory devs
```

Config confirms `CONFIG_DEVMEM=y` and `CONFIG_STRICT_DEVMEM=y`. The /dev/mem
device and its read/write/mmap operations are therefore compiled in, unlike
the canonical Image (CONFIG_DEVMEM not set; no dev/mem strings).

### File identity and size

```
$ file out/Image
out/Image: Linux kernel ARM64 boot executable Image, little-endian, 4K pages

$ readelf -h src/vmlinux | grep -E 'Class:|Type:|Machine:'
  Class:                             ELF64
  Type:                              DYN (Shared object file)
  Machine:                           AArch64

Image header bytes: 4d5a 0091 ... "ARMd" at 0x38 (standard arm64 Image magic)
```

Built out/Image: 20,126,208 bytes
SHA-256: 66d3fc1b6832a6caa66cd26bbb4417c9ef1f7a6c017dfc44a93063b8ff2813dc
Canonical Image: 23,073,280 bytes
Size delta: -2,947,072 bytes (smaller; consistent with no LTO/CFI and a
non-Android build id/payload set).

Final config SHA-256:
`502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e`

It is an ARM64 Linux kernel Image — not x86-64, not vmlinux, not zImage.

## Done-criteria checklist

- [x] Step 0 disk gate observed (18 GiB >= 8 GiB) and recorded.
- [x] Step 1 both input SHAs verified; kernel identified verbatim.
- [x] Step 2 source acquired at exact canonical commit 4da740c10dc0...;
      branch and size recorded.
- [x] Step 3 config delta applied; final .config lines quoted; simpledrm
      absence proven; every other change listed one per line.
- [x] Step 4 build exit 0 with Image target; toolchain, command, elapsed time
      and log tail recorded.
- [x] Step 5 verification complete with strings + offsets + SHA + size delta
      recorded.
- [x] Verdict PASS (source matched, deltas applied, build exit 0,
      verification holding).
- [x] `sys.boot_completed` line present (not observed — no flight).
- [x] WALL-TYPE-TAG and `Rust par bhi lagu:` lines present.
- [x] Progress heartbeat: progress-t686.txt one line per step; heartbeat.json
      refreshed per step.
- [x] Findings, outcome, and death note written in workdir. No push, no commit,
      no second agent, no flight.

## Evidence-rule notes

- TEN-PASSES: not claimed. This lane produced one successful build; no
  reproducibility claim is made.
- GUEST-MARKER-ONLY: no boot markers are used; no flight ran.
- NO-MISSING-INPUTS: both required inputs present and SHA-verified.
- GUEST-CONSUMES-ARTIFACT: not applicable to T686; consumption/boot proof is
  T687's job.
- DISPLAY-SCAFFOLD: no frame-commit claim; no flight.
- The build is a capability fix (kernel symbols), not a rendered-frame proof.

## Deliverables

- out/Image — 20,126,208 B, SHA-256
  66d3fc1b6832a6caa66cd26bbb4417c9ef1f7a6c017dfc44a93063b8ff2813dc
- out/.config — SHA-256
  502e79c3de81ccefb0bbcc54a82ba5d068b57d34657b1899374a43a6ba7d701e
- out/build.log — full successful build log (tail above)
- out/build-clang-attempt2-stpcpy.log — retained failed-link log
- config-delta-t686.txt, config-changes-one-per-line-t686.txt — config evidence
- build-clang.sh — reproducible build command
- progress-t686.txt, heartbeat.json — heartbeat trail
- findings-t686.md, outcome-t686.txt, DEATH_NOTE-T686.md
