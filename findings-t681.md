# T681 — Composer 2.1 link gap: bionic vendor ELF on Linux (gsai box)

Verdict: PASS

- Date: 2026-11-04
- Workdir: `/home/muse/t681-link`
- Reference tree (read-only): `/home/muse/t680-toolchain/composer-2-1-tree`,
  branch `android14-release`, HEAD `40e9f1537e308ed49e3b561ce333e3f2bb64f31e`
- Scope: generate interface sources, obtain an Android AArch64 toolchain, build the
  link-gap libraries, and link a bionic vendor ELF for the Composer 2.1 service.
  No flight, no boot, no guest claim in this lane.

This lane closes the T680 link gap up to a linked binary. The T679 vendor.img
CPIO-member install route is carried forward to the driver / T682, not executed here.

---

## Step 0 — Box disk gate

Recorded at lane start:

```
$ df -h /home/muse
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda3       143G  109G   27G  81% /
```

27 Gi available >= 20 Gi required. PASS, no disk wall.

Re-checked after toolchain + builds:

```
$ df -h /home/muse
/dev/sda3       143G  112G   24G  83% /
```

Still >= 20 Gi.

## Step 1 — Interface source generation

Generator: the T680 `hidl-gen` binary, copied read-only use into this workdir.

```
$ sha256sum hidl-gen
e9b8f732ece1195f6f747762a57e256ef4ce03472a1c13203fdc1a622b92ab95  hidl-gen
$ ls -l hidl-gen
5170504 bytes
```

Generation command (re-run into `/tmp/gencheck` to confirm provenance; the command
below reproduced the on-disk tree byte for byte):

```
./hidl-gen -o generated-sources -L c++-sources \
  -r android.hardware.graphics:/home/muse/t681-link/hidl-roots/android.hardware.graphics \
  -r android.hidl:/home/muse/t680-toolchain/aosp/libhidl/transport \
  android.hardware.graphics.composer@2.1 \
  android.hardware.graphics.common@1.0 \
  android.hidl.base@1.0 \
  android.hidl.manager@1.0 \
  android.hidl.manager@1.1 \
  android.hidl.manager@1.2
```

The read-only HAL roots are symlinks under `hidl-roots/android.hardware.graphics`:
`common -> T680/aosp/hwinterfaces/graphics/common`,
`composer -> T680/composer-2-1-tree/graphics/composer`; the `android.hidl` root is
`T680/aosp/libhidl/transport`.

Generated C++ sources (12 files, sizes in bytes):

| # | Path (under `generated-sources/`) | Bytes | Sources |
|---|-----------------------------------|------:|---------|
| 1 | `android/hardware/graphics/common/1.0/types.cpp` | 1,008 | common@1.0 |
| 2 | `android/hardware/graphics/composer/2.1/ComposerAll.cpp` | 34,771 | composer@2.1 |
| 3 | `android/hardware/graphics/composer/2.1/ComposerCallbackAll.cpp` | 30,407 | composer@2.1 |
| 4 | `android/hardware/graphics/composer/2.1/ComposerClientAll.cpp` | 186,724 | composer@2.1 |
| 5 | `android/hardware/graphics/composer/2.1/types.cpp` | 1,018 | composer@2.1 |
| 6 | `android/hidl/base/1.0/BaseAll.cpp` | 55,195 | hidl base@1.0 |
| 7 | `android/hidl/base/1.0/types.cpp` | 905 | hidl base@1.0 |
| 8 | `android/hidl/manager/1.0/ServiceManagerAll.cpp` | 82,355 | hidl manager@1.0 |
| 9 | `android/hidl/manager/1.0/ServiceNotificationAll.cpp` | 21,379 | hidl manager@1.0 |
| 10 | `android/hidl/manager/1.1/ServiceManagerAll.cpp` | 29,821 | hidl manager@1.1 |
| 11 | `android/hidl/manager/1.2/ClientCallbackAll.cpp` | 19,792 | hidl manager@1.2 |
| 12 | `android/hidl/manager/1.2/ServiceManagerAll.cpp` | 69,321 | hidl manager@1.2 |

`generated-sources/` total 608 KiB. Headers were generated with the same roots using
`-L c++-headers` into `generated-headers/`: 51 header files, 932 KiB, also byte-identical
on regeneration.

Provenance check (one repeat, not a ten-pass claim):

```
$ diff -rq /tmp/gencheck generated-sources && echo SOURCES_IDENTICAL
SOURCES_IDENTICAL
$ diff -rq /tmp/gencheckh generated-headers && echo HEADERS_IDENTICAL
HEADERS_IDENTICAL
```

No generation wall.

## Step 2 — Android AArch64 target toolchain

Probe (a), NDK r29 Linux package — first choice, reachable:

```
$ curl -sIL https://dl.google.com/android/repository/android-ndk-r29-linux.zip
HTTP/1.1 200 OK
content-length: 783549481
```

Downloaded 783,549,481 bytes (747.3 MiB), under the 5 GiB Step 2 cap.

```
$ sha256sum toolchain/android-ndk-r29-linux.zip
4abbbcdc842f3d4879206e9695d52709603e52dd68d3c1fff04b3b5e7a308ecf
```

Matches the published r29 checksum. Extracted size 2.4 GiB under
`toolchain/android-ndk-r29` (zip retained in workdir).

Compiler and sysroot:

```
compiler: /home/muse/t681-link/toolchain/android-ndk-r29/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang++
sysroot : /home/muse/t681-link/toolchain/android-ndk-r29/toolchains/llvm/prebuilt/linux-x86_64/sysroot
clang   : Android (13989888, ... based on r563880c) clang version 21.0.0
target  : aarch64-unknown-linux-android34
```

API 34 was selected because AOSP `liblog` uses symbols introduced in API 30
(`__system_property_wait`, `__system_property_read_callback`). The NDK sysroot ships
`libc.so`, `libm.so`, `libdl.so` stubs and the bionic crt objects for that API.

Probe (b), AOSP prebuilt clang + bionic sysroot: not needed, because probe (a)
yielded a complete target compiler + bionic sysroot in one round. The T680 trees do
not contain an AOSP prebuilt compiler. No toolchain wall.

New fetch for this step: 747.3 MiB.

## Step 3 — Link-gap libraries built for AArch64/bionic

All libraries were built from the AOSP sources already staged by T680 plus the sparse
`system/core` tree fetched in this lane, as **static archives** for
`aarch64-linux-android34`, with `-fPIC -O2 -g` and bionic headers from the NDK sysroot.
No host glibc include or link path was used at any point.

Build script: `build-libs.sh` (re-runnable). `logs/build-libs.log` ends in
`ALL_LIBS_OK`. Archives in `libs/`:

| Archive | AOSP sources | Bytes |
|---------|-------------:|------:|
| `liblog.a` | 13 | 1,517,696 |
| `libbase.a` | 16 | 2,365,744 |
| `libcutils.a` | 20 (+`strlcpy.c`) | 1,027,590 |
| `libsync.a` | 1 | 25,544 |
| `libvndksupport.a` | 1 | 17,230 |
| `libutils.a` | 20 | 1,275,484 |
| `libhwbinder.a` | 11 | 2,387,896 |
| `libfmq.a` | 2 | 192,164 |
| `libhidlbase.a` | 12 | 6,190,778 |
| `libgraphics-composer@2.1.a` | 4 | 7,852,210 |
| `libgraphics-common@1.0.a` | 1 | 54,480 |
| `libhidl-base@1.0.a` | 2 | 1,856,200 |
| `libhidl-manager@1.0.a` | 2 | 3,592,178 |
| `libhidl-manager@1.1.a` | 1 | 1,698,824 |
| `libhidl-manager@1.2.a` | 2 | 3,567,324 |

Total `libs/` = 33 MiB. Every object in every archive was checked:

```
ELF 64-bit LSB relocatable, ARM aarch64
```

Additions beyond the T680 enumeration, each justified by a concrete missing
symbol/include:

- `libsync.a` (25,544 B) — `android::Fence` / sync-file helpers referenced by the
  composer client fencing path; fetched as `system/core/libsync` alongside the core tree.
- `libvndksupport.a` (17,230 B) — `android::vndk::BaseVndkSupport` namespace lookup used
  by libhidlbase's `dlopen` path.
- `libhidl-manager@1.1.a` (1,698,824 B) and `libhidl-manager@1.2.a` (3,567,324 B) —
  the transport manager interfaces referenced by libhidlbase's service-manager client;
  1.0 alone leaves unresolved vtable/transaction symbols.
- `libcutils` needed `strlcpy.c` compiled with C flags; `liblog` needed
  `-DANDROID_DEBUGGABLE=0 -DSNET_EVENT_LOG_TAG=1397638484 -DLIBLOG_LOG_TAG=1006`;
  `-DPAGE_SIZE=4096` is defined for the FMQ/page-size code.
- System/core trees `libprocessgroup` and `libsystem` were used for headers only
  (`processgroup`/`system` properties helpers inline through libbase), so they add no
  archive and no new fetch.

New fetches across Steps 2–3: 747.3 MiB (NDK zip) + 3.8 MiB (sparse `system/core`
clone at `aosp/core`, HEAD `f53626de0f57e5ba91931d8c2154a83407c1d570`) ≈ 751 MiB,
well under the 20 GiB combined cap and under the 5 GiB Step 2 cap. The
`libvndksupport` standalone repo probe returned 404, so that library was built from the
already-fetched `system/core/libvndksupport` instead of a second clone.

No library-build wall.

## Step 4 — Service implementation and link

`t681-service.cpp` (23,196 B) implements the Composer 2.1 HIDL service:

- `IComposer::createClient` returns a `ComposerClientImpl`;
- `IComposerClient::executeCommands` reads the input FMQ command buffer using the
  staged `ComposerCommandBuffer.h` wire format, applies the state commands, and writes
  the result commands back to the output FMQ;
- `Command::PRESENT_DISPLAY` dispatches toward the simplefb MMIO framebuffer at
  `0x5F000000` (defaults 1280x720, stride = width, 32 bpp; overridable with
  `T681_FB_WIDTH/HEIGHT/STRIDE/BPP` and `T681_FB_PHYS`);
- `registerAsService()` registers `android.hardware.graphics.composer@2.1` on the
  hwbinder transport, then joins the RPC thread pool.

`t681-dl-shim.cpp` (964 B) supplies `android_get_exported_namespace`, which is private
bionic API absent from the NDK headers. It resolves the symbol at runtime from
`libdl_android.so` / `libdl.so` via `dlopen`+`dlsym`, and returns null when unavailable
so libvndksupport falls back to its documented plain-`dlopen` path.

Link (`build-service.sh`):

```
aarch64-linux-android34-clang++ --target=aarch64-linux-android34 \
  -fPIE -pie -static-libstdc++ -Wl,--gc-sections -Wl,--build-id \
  -o out/android.hardware.graphics.composer@2.1-service \
  obj/t681-service.o obj/t681-dl-shim.o \
  -Wl,--start-group <the 15 archives above> -Wl,--end-group \
  -ldl -lm -lc
```

Static libc++ and static AOSP archives mean the ELF has no AOSP `.so` dependencies;
only bionic's public `libdl.so`, `libm.so`, `libc.so` are `NEEDED`. The install target
carried forward is `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service`
(T679 path); the linked artifact is
`/home/muse/t681-link/out/android.hardware.graphics.composer@2.1-service`.

```
size   : 19,941,040 bytes
sha256 : ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132
```

Link log: `logs/service-link.err` is empty, `service link exit=0`. No link wall.

## Step 5 — ELF verification (no flight)

Exact outputs are in `logs/verify-file.txt`, `logs/verify-readelf-header.txt`,
`logs/verify-needed.txt`, `logs/verify-interp.txt`, `logs/verify-glibc.txt`,
`logs/verify-versions-final.txt`, `logs/verify-deps-final.txt`,
`logs/verify-symbols-defined.txt`, `logs/verify-symbols-hidl.txt`,
`logs/verify-final.txt`.

### `file`

```
out/android.hardware.graphics.composer@2.1-service: ELF 64-bit LSB pie executable,
ARM aarch64, version 1 (SYSV), dynamically linked,
interpreter /system/bin/linker64, for Android 34, built by NDK r29 (14206865),
BuildID[sha1]=df188e94f3ac4ed2d587926d01d8fa03b6cb294d,
with debug_info, not stripped
```

Wording note: this host runs GNU `file` 5.46. Since file 5.33, the string
`pie executable` is used for an `ET_DYN` ELF that carries `PT_INTERP`; older file
releases printed `shared object` for the same object. The ELF is a shared-object-type
(`ET_DYN`) Android executable:

```
$ readelf -h out/android.hardware.graphics.composer@2.1-service
  Class:                             ELF64
  Data:                              2's complement, little endian
  Type:                              DYN (Position-Independent Executable file)
  Machine:                           AArch64
  Entry point address:               0x57870
```

An alternate `-shared` link (probe, not the deliverable) makes `file` print
`shared object`, but lld then drops `PT_INTERP`; such an object is not directly
executable by Android init. The `-pie` `ET_DYN` form above is the correct Android
vendor binary and is what `file` 5.33+ names `pie executable`. All substantive
AArch64/bionic/service-entry checks pass.

### Interpreter and dependencies

```
[Requesting program interpreter: /system/bin/linker64]

NEEDED  libdl.so
NEEDED  libm.so
NEEDED  libc.so
FLAGS   BIND_NOW
FLAGS_1 NOW PIE
```

No `RPATH`, no `RUNPATH`, no `libc.so.6`, no `ld-linux*`, no `libstdc++.so.6`:

```
GLIBC: 0
libc.so.6: 0
ld-linux: 0
/lib/x86_64: 0
libstdc++.so.6: 0
```

All strong undefined dynamic symbols carry bionic version tags only:

```
151 (LIBC)
  2 (LIBC_O)
  4 (LIBC_Q)
  8 (LIBC_R)
```

The only unversioned undefined symbols are three **weak** `android::CallStack`
debuggerd hooks (`getCurrentInternal`, `logStackInternal`, `deleteStack`); weak
references do not block loading. There is no host-glibc reference anywhere.

### Service entry points present

```
0000000000057970 T main
0000000000057e48 t ComposerImpl::createClient(...)
0000000000058ef8 t ComposerClientImpl::executeCommands(...)
000000000005f360 T IComposer::registerAsService(std::string const&)
```

### Verification result

- AArch64, `ET_DYN`/PIE Android vendor executable: YES
- Interpreter `/system/bin/linker64`: YES
- bionic-only dynamic dependencies, no host glibc: YES
- Composer 2.1 service entry points present: YES

No verification wall.

## Walls

None. No disk, generation, toolchain-acquisition, library-build, link, or
verification wall was hit.

WALL-TYPE-TAG: none — link proven

## Required attestations

sys.boot_completed: NOT observed in T681 — this lane flies nothing; no guest claim is made.

Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.

## Carried forward / limits of this proof

- This is a **linked binary**, not a boot or flight. Nothing was executed on a device
  or emulator; `sys.boot_completed` was not observed and no guest claim is made.
- T679 vendor.img CPIO-member replacement install route is carried forward to T682 on
  the Mac; it was not executed here.
- Runtime concerns intentionally left to T682: vendor sepolicy for `/dev/mem` access,
  exact simplefb physical/stride values on the target panel, and runtime linking
  against the device's own `/system/lib64` bionic (the NDK r29 sysroot targets API 34;
  the device's linker must be at least that API for the `LIBC_R` unwinder tags).
- No repo commit, no push, no Mac access, and no deletion outside the workdir occurred.
- The single regeneration repeat is a provenance check, not a TEN-PASSES
  reproducibility claim.

## Artifacts

- `findings-t681.md` — this file
- `outcome-t681.txt` — `PASS`
- `DEATH_NOTE-T681.md` — lane death note
- `out/android.hardware.graphics.composer@2.1-service` — linked vendor ELF
- `t681-service.cpp`, `t681-dl-shim.cpp` — service sources
- `build-libs.sh`, `build-service.sh` — re-runnable builds
- `libs/` — 15 AArch64 static archives
- `generated-sources/` (12 `.cpp`), `generated-headers/` (51 `.h`)
- `logs/` — disk probe, generation, build, link and verification evidence
