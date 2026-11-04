# T680 Findings — Composer 2.1 toolchain feasibility on Linux (gsai box)

Date: 2026-11-04
Box: gsai Linux (Ubuntu), workdir `/home/muse/t680-toolchain`
Lane: T680 (successor to driver-certified T679 FAIL)
Verdict: **PASS** (toolchain proven; no service link attempted)

This lane answers one question: what is the minimal AOSP tree set that yields a
service-capable Composer 2.1 toolchain (build `hidl-gen`, generate the Composer
2.1 C++ headers, compile a service-shaped translation unit to objects)? A full
service link is explicitly out of scope and was not performed.

---

## Step 0 — Box disk gate

```
$ df -h /home/muse
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda3       143G  109G   27G  81% /
```

Available = **27 Gi**, required >= 20 Gi. **Gate PASS.** (Reading unchanged at
the end of the lane: 27 Gi available.)

---

## Step 1 — Staged tree verification

```
$ git -C composer-2-1-tree rev-parse HEAD
40e9f1537e308ed49e3b561ce333e3f2bb64f31e
$ git -C composer-2-1-tree branch --show-current
android14-release
$ git -C composer-2-1-tree log -1 --format='%H %ci %s'
40e9f1537e308ed49e3b561ce333e3f2bb64f31e 2023-07-28 01:53:14 +0000 Merge cherrypicks of ['googleplex-android-review.googlesource.com/24232539'] into udc-release.
```

HEAD matches the driver-supplied value exactly. Required files:

| file | bytes |
|---|---:|
| `graphics/composer/2.1/IComposerClient.hal` | 45790 |
| `graphics/composer/2.1/IComposer.hal` | 3221 |
| `graphics/composer/2.1/types.hal` | 1265 |
| `graphics/composer/2.1/default/service.cpp` | 1937 |
| `graphics/composer/2.1/default/Android.bp` | 1065 |

All present, non-zero. `git status --short` shows exactly one working-tree
change: ` D .clang-format` (a deleted symlink, irrelevant to the HAL sources).
The staged tree is **not damaged**; no re-fetch was done.

Integrity corroboration: the dependency tree `platform/hardware/interfaces`
fetched for `graphics/common` is at the **same HEAD** `40e9f15…`, so the
`android.hardware.graphics.common@1.0` types used for generation are from the
identical revision as the staged Composer 2.1 HAL.

---

## Step 2 — Egress probe

Exactly one attempt, no retry needed (AOSP source host answered).

```
$ git ls-remote https://android.googlesource.com/platform/system/tools/hidl HEAD
d11716d0c896b6850c53ba59642570d60966843a	HEAD
exit code: 0   (stderr empty)

$ git ls-remote https://github.com HEAD
fatal: repository 'https://github.com/' not found
exit code: 128
```

`https://github.com` is not a git repository at its root, so exit 128 is the
host answering, not an egress block. A real GitHub repository was probed to
disambiguate:

```
$ git ls-remote https://github.com/aosp-mirror/platform_system_tools_hidl HEAD
fatal: could not read Username for 'https://github.com': No such device or address
exit code: 128
```

GitHub asks for credentials for that mirror; AOSP — the only source this lane
needs — is reachable with exit 0. **Egress gate PASS.** No proxy/mirror hunt was
started.

---

## Step 3 — Minimal fetch

All fetches are `--depth 1` on branch `android14-release` (except `native`,
which was a path probe and is unused). Sparse checkouts were used where only a
subtree was needed.

| AOSP tree | checkout path | HEAD | on-disk size |
|---|---|---|---:|
| `platform/system/tools/hidl` | `aosp/hidl` | `aed54501bd16136bc4f20e727ba76d95080102f1` | 4.7 M |
| `platform/system/libbase` | `aosp/libbase` | `b082e1445ae49943bdff3f0799ed00f7bee61325` | 1.4 M |
| `platform/external/jsoncpp` | `aosp/jsoncpp` | `22d33955e4c2525e659f729c9f02afe09183b107` | 3.5 M |
| `platform/system/libhwbinder` | `aosp/libhwbinder` | `45db897b21385e9db4f6d1a0af6573ca649bc88b` | 932 K |
| `platform/system/logging` | `aosp/liblog` | `0d18f57987b1dbf3494ca7d2b13645eac002d3cd` | 3.5 M |
| `platform/system/libhidl` | `aosp/libhidl` | `69123db938e2d4b7d129e8d7bedbfdb98ec6e5b8` | 1.4 M |
| `platform/system/libfmq` | `aosp/libfmq` | `d7798a6f5e6ab6ea0ae1a33b7fa2370107617926` | 676 K |
| `platform/system/core` (sparse: `libutils/include`, `libcutils/include`) | `aosp/core` | `f53626de0f57e5ba91931d8c2154a83407c1d570` | 2.2 M |
| `platform/hardware/interfaces` (sparse: `graphics/common`) | `aosp/hwinterfaces` | `40e9f1537e308ed49e3b561ce333e3f2bb64f31e` | 3.8 M |
| `platform/frameworks/native` (sparse: `libs/binder/include`) — **probe only, unused** | `aosp/native` | `4f463a6b1de9198963dc6aff74154a504ba3f8f6` | 2.6 M |

```
$ du -sh aosp
25M	aosp
```

Running total **25 MiB**, far under the 20 GiB hard cap. **Size gate PASS.** No
full platform `repo sync` was run. Each tree was added only after a concrete
missing include proved it necessary:

- `system/libbase` → `android-base/logging.h`, `hex.h`, `strings.h`, …
- `system/logging/liblog` → `android/log.h` (libbase `logging.cpp` includes it on host)
- `external/jsoncpp` → `json/json.h` (`generateInheritanceHierarchy.cpp`)
- `system/libhwbinder` → `hwbinder/IBinder.h` (`Interface.cpp`)
- `system/core` → `utils/Errors.h`, `utils/NativeHandle.h`, `utils/misc.h`, `cutils/native_handle.h`
- `system/libhidl` → HIDL runtime headers + `android.hidl.base`/`android.hidl.manager` `.hal`
- `system/libfmq` → `fmq/MQDescriptorBase.h` (pulled by `hidl/MQDescriptor.h`)
- `hardware/interfaces` → `android.hardware.graphics.common@1.0` types

---

## Step 4 — Build hidl-gen

`system/tools/hidl` at this revision is Soong-only (no `CMakeLists.txt`), so
`cmake`/`ninja` were not applicable. The host toolchain was driven directly:
g++ 15.2.0, flex 2.6.4, bison 3.8.2.

Generation of the lexer/parser:

```
$ flex -o build-t680/gen/hidl-gen_l.cpp aosp/hidl/hidl-gen_l.ll        # exit 0
$ bison --defines=build-t680/gen/hidl-gen_y.h \
        -o build-t680/gen/hidl-gen_y.cpp aosp/hidl/hidl-gen_y.yy      # exit 0
```

Build command (reproducible script kept at `build-t680/build.sh`):

```
$ ./build-t680/build.sh                                                # exit 0
```

The script compiles the 38 hidl-gen sources + flex/bison output + 16 libbase
sources + 3 bundled jsoncpp sources into per-TU objects, then links.

- Binary path: `/home/muse/t680-toolchain/build-t680/hidl-gen`
- Size: 5,170,504 bytes
- SHA-256: `e9b8f732ece1195f6f747762a57e256ef4ce03472a1c13203fdc1a622b92ab95`
- Link line: `g++ <objects> -o hidl-gen -lcrypto -lpthread -ldl -lrt`
  (system OpenSSL `libcrypto` supplies `SHA256_*`; no AOSP boringssl needed)

Proof the binary runs — documented help:

```
$ ./build-t680/hidl-gen -h
Usage: ./build-t680/hidl-gen -o <output path> -L <language> [-O <owner>] [-p <root path>] (-r <interface root>)+ [-R] [-F] [-v] [-d <depfile>] FQNAME...

Process FQNAME, PACKAGE(.SUBPACKAGE)*@[0-9]+.[0-9]+(::TYPE)?, to create output.
...
```

(`-h` prints usage and returns 1 by design.) Functional proof is the successful
`-L c++-headers` and `-L hash` runs below (exit 0).

Host-build accommodations (all documented, none touch the HAL inputs):

1. g++ 15.2 is stricter than AOSP clang about transitive includes; supplied
   `-include cstdint -include cstring -include cstdlib -include algorithm -include limits`.
2. Mapped clang's `__builtin_available(...)` to the host fallback (`=0`).
3. `libbase/logging.cpp` compiles a never-called logd fallback that references
   `__android_log_buf_print`; linked a 1-symbol host stub
   (`build-t680/host_stubs.cpp`) instead of the whole host liblog.
4. `posix_strerror_r.cpp` built with `-U_GNU_SOURCE` (g++ predefines it, which
   selects the GNU `char*` variant instead of the POSIX `int` variant).

**Build wall: none.** Final build exit 0.

---

## Step 5 — Generate the Composer 2.1 headers

Root layout (symlinks only; the staged tree was not modified):

```
hidl-roots/android.hardware.graphics/composer -> composer-2-1-tree/graphics/composer
hidl-roots/android.hardware.graphics/common   -> aosp/hwinterfaces/graphics/common
```

Generation command:

```
$ ./build-t680/hidl-gen -o generated-t680 -L c++-headers -R \
    -r android.hardware.graphics:/home/muse/t680-toolchain/hidl-roots/android.hardware.graphics \
    -r android.hidl:/home/muse/t680-toolchain/aosp/libhidl/transport \
    android.hardware.graphics.composer@2.1
exit code: 0   (stderr empty)
```

(The single `android.hardware.graphics` root is required because `hidl-gen`
rejects nested package roots; `android.hardware` + `android.hardware.graphics.common`
produced `ERROR: conflicting package roots`.)

### Composer 2.1 headers (17 files)

| generated header | bytes |
|---|---:|
| `android/hardware/graphics/composer/2.1/IComposer.h` | 20955 |
| `android/hardware/graphics/composer/2.1/IComposerClient.h` | 145877 |
| `android/hardware/graphics/composer/2.1/IComposerCallback.h` | 19658 |
| `android/hardware/graphics/composer/2.1/types.h` | 8822 |
| `android/hardware/graphics/composer/2.1/hwtypes.h` | 589 |
| `android/hardware/graphics/composer/2.1/IHwComposer.h` | 895 |
| `android/hardware/graphics/composer/2.1/IHwComposerClient.h` | 981 |
| `android/hardware/graphics/composer/2.1/IHwComposerCallback.h` | 785 |
| `android/hardware/graphics/composer/2.1/BnHwComposer.h` | 2765 |
| `android/hardware/graphics/composer/2.1/BnHwComposerClient.h` | 8340 |
| `android/hardware/graphics/composer/2.1/BnHwComposerCallback.h` | 2870 |
| `android/hardware/graphics/composer/2.1/BpHwComposer.h` | 3475 |
| `android/hardware/graphics/composer/2.1/BpHwComposerClient.h` | 12637 |
| `android/hardware/graphics/composer/2.1/BpHwComposerCallback.h` | 3683 |
| `android/hardware/graphics/composer/2.1/BsComposer.h` | 19571 |
| `android/hardware/graphics/composer/2.1/BsComposerClient.h` | 55179 |
| `android/hardware/graphics/composer/2.1/BsComposerCallback.h` | 19219 |

### Support-package headers also generated (19 files)

These are required because the Composer 2.1 headers include them.

| generated header | bytes |
|---|---:|
| `android/hardware/graphics/common/1.0/types.h` | 144323 |
| `android/hardware/graphics/common/1.0/hwtypes.h` | 577 |
| `android/hidl/base/1.0/IBase.h` | 7848 |
| `android/hidl/base/1.0/types.h` | 7949 |
| `android/hidl/base/1.0/hwtypes.h` | 459 |
| `android/hidl/base/1.0/IHwBase.h` | 502 |
| `android/hidl/base/1.0/BnHwBase.h` | 3930 |
| `android/hidl/base/1.0/BpHwBase.h` | 4019 |
| `android/hidl/base/1.0/BsBase.h` | 14011 |
| `android/hidl/manager/1.0/IServiceManager.h` | 30176 |
| `android/hidl/manager/1.0/IServiceNotification.h` | 12212 |
| `android/hidl/manager/1.0/IHwServiceManager.h` | 1249 |
| `android/hidl/manager/1.0/IHwServiceNotification.h` | 625 |
| `android/hidl/manager/1.0/BnHwServiceManager.h` | 4053 |
| `android/hidl/manager/1.0/BnHwServiceNotification.h` | 2248 |
| `android/hidl/manager/1.0/BpHwServiceManager.h` | 6024 |
| `android/hidl/manager/1.0/BpHwServiceNotification.h` | 3009 |
| `android/hidl/manager/1.0/BsServiceManager.h` | 28883 |
| `android/hidl/manager/1.0/BsServiceNotification.h` | 16231 |

Total: **36 generated headers**, all listed above with path and size.
**Generation wall: none.**

Interface hashes from the built binary (independent proof the staged HAL parsed):

```
$ ./build-t680/hidl-gen -L hash ... android.hardware.graphics.composer@2.1
e205dd30f5ff99445b706a901de8ebc46c379e9d7c1921d6a327ed2082cfa83d android.hardware.graphics.composer@2.1::types
b3aac6c3817f039964fcd62268274b3039e17bd7d0d5b40b4d1d1c7b19a1f866 android.hardware.graphics.composer@2.1::IComposer
b19d00eb8a8b3b0034a0321f22e8f32162bf4c2aebbce6da22c025f56e459ea2 android.hardware.graphics.composer@2.1::IComposerCallback
d4fea995378bb4f421b4e24ccf68cad2734ab07fe4f874a126ba558b99df5766 android.hardware.graphics.composer@2.1::IComposerClient
exit code: 0
```

---

## Step 6 — Compile a service translation unit (compile-only)

TU: `/home/muse/t680-toolchain/t680-service-probe.cpp`. Shape follows the
staged `default/service.cpp` (obtain `IComposer`, `createClient`, then
`registerAsService`), but avoids the platform-only passthrough loader and
binder `ProcessState` so it can compile from the generated headers alone. It
references `IComposer`, `IComposerClient`, `Error`, `createClient`,
`registerAsService` and `getService`.

Compiler used: extracted Ubuntu clang 20.1.8 + libc++ 20 (real Android ABI).
Reason: g++ 15.2 + libstdc++ cannot compile the frozen HIDL headers — libstdc++'s
`std::string` is 8 bytes larger, so the Android-ABI static assert fires:

```
hidl/HidlInternal.h:216:40: error: static assertion failed: HidlInstrumentor size frozen by prebuilts
  comparison reduces to '(104 == 88)'
```

clang + libc++ satisfies the frozen layout, so the AOSP headers were used
**unmodified**. clang/libc++ were extracted from Ubuntu `.deb` packages into
`toolchain-extras/` (no system install, no sudo).

Compile command:

```
$ clang++ -std=c++17 -fPIC -O0 -g -w \
    -nostdinc++ -isystem toolchain-extras/usr/include/c++/v1 \
    -resource-dir toolchain-extras/usr/lib/llvm-20/lib/clang/20 \
    -Igenerated-t680 \
    -Iaosp/libhidl/base/include -Iaosp/libhidl/transport/include \
    -Iaosp/libhwbinder/include -Iaosp/core/libutils/include \
    -Iaosp/core/libcutils/include -Iaosp/libbase/include \
    -Iaosp/liblog/liblog/include -Iaosp/libfmq/base -Iaosp/libfmq/include \
    -c t680-service-probe.cpp -o build-t680/service-obj/t680-service-probe.o
exit code: 0   (stderr empty)
```

Object files produced:

| object | bytes | SHA-256 | type |
|---|---:|---|---|
| `build-t680/service-obj/t680-service-probe.o` | 174792 | `7b0e0532aef82449b3326f2eb6df5110b85705fb4e8ddf4cac189f6002c94e62` | ELF 64-bit LSB relocatable, x86-64, with debug_info |

Symbol evidence that the object really references the Composer 2.1 interface:

```
$ nm -C build-t680/service-obj/t680-service-probe.o
T t680_composer21_service_probe
U android::hardware::graphics::composer::V2_1::IComposer::getService(std::__1::basic_string<...> const&, bool)
U android::hardware::graphics::composer::V2_1::IComposer::registerAsService(std::__1::basic_string<...> const&)
W android::hardware::graphics::composer::V2_1::IComposer::getService(char const*, bool)
W android::sp<android::hardware::graphics::composer::V2_1::IComposerClient>::...
```

These are host x86-64 objects. **No link was performed.** `IComposer::getService`
and `registerAsService` remain undefined, as expected for a compile-only step.

---

## Link gap — what a full service link still needs (required section, not a failure)

The staged `default/Android.bp` declares these `shared_libs` (plus one header
lib). None of them is linked in T680. Source tree for each:

| library | AOSP source tree | role / why needed |
|---|---|---|
| `libhidlbase` | `platform/system/libhidl` (`base/`, `transport/`) | HIDL runtime: `hidl_string`/`hidl_vec`, `Return`, `sp`, `getService`, `registerAsService`, `IServiceManager`, `joinRpcThreadpool` |
| `libfmq` | `platform/system/libfmq` | `MQDescriptorSync` / `MessageQueue` used by `IComposerClient` command queue |
| `libhwbinder` | `platform/system/libhwbinder` | `hwbinder/IBinder`, `Parcel`, `ProcessState`, `IPCThreadState` underneath the HIDL transport |
| `libbase` | `platform/system/libbase` | `android-base` logging/strings/file/properties |
| `liblog` | `platform/system/logging/liblog` | `android/log.h`, `__android_log_*` |
| `libutils` | `platform/system/core/libutils` | `RefBase`, `StrongPointer`, `NativeHandle`, `Errors`, `misc` |
| `libcutils` | `platform/system/core/libcutils` | `cutils/native_handle.h`, trace |
| `libbinder` | `platform/frameworks/native/libs/binder` | `binder/ProcessState.h`; real `service.cpp` calls `initWithDriver("/dev/vndbinder")` |
| `libhardware` | `platform/hardware/libhardware` | `hw_get_module` for the passthrough HWC loader |
| `libsync` | `platform/system/core/libsync` | sync-fence primitives (`sync_wait`, etc.) |
| `libhwc2on1adapter` | staged `graphics/composer/2.1/utils/hwc2on1adapter` | HWC2-on-HWC1 adapter |
| `libhwc2onfbadapter` | staged `graphics/composer/2.1/utils/hwc2onfbadapter` | framebuffer adapter |
| `android.hardware.graphics.composer@2.1` | generated from staged `graphics/composer/2.1/*.hal` (only headers generated so far; `-L c++-sources` not yet run) | the Composer 2.1 HIDL interface library |
| `android.hardware.graphics.composer@2.1-resources` | staged `graphics/composer/2.1/utils/resources` | `ComposerResources` |
| `android.hardware.graphics.composer@2.1-passthrough` (header lib) | staged `graphics/composer/2.1/utils/passthrough` | `HwcLoader.h` |
| `android.hardware.graphics.common@1.0` | `platform/hardware/interfaces/graphics/common/1.0` | `PixelFormat`, `ColorMode`, `Hdr`, … |
| `android.hidl.base@1.0` | `platform/system/libhidl/transport/base/1.0` | `IBase` |
| `android.hidl.manager@1.0` | `platform/system/libhidl/transport/manager/1.0` | `IServiceManager`, `IServiceNotification` |
| target libc++ / bionic | Android NDK sysroot, or a full Soong platform tree | the object above is a **host** x86-64 object; a real vendor service links against Android bionic, not glibc |

Also required by the real `default/service.cpp` but not exercised here:
`libdl`, `libpthread`, `librt` (host) or their bionic equivalents (target).

The following lane closes this gap by generating the interface sources
(`hidl-gen -L c++-sources`), building the libraries above, and linking with an
Android target toolchain. **T680 makes no link claim.**

---

## Standing-rule compliance

- **TEN-PASSES:** no reproducibility claim is made. T680 records one clean
  build, one clean header generation, and one clean compile. Ten consecutive
  passes were not attempted and are not claimed.
- **GUEST-MARKER-ONLY / no guest claim:** nothing was booted.
- **NO-MISSING-INPUTS:** every missing include was satisfied by a fetched AOSP
  tree and recorded above; no input was guessed.
- **GUEST-CONSUMES-ARTIFACT:** not applicable; no injection in this lane.
- **QEMU-REGRESSION-VECTORS:** not applicable; no decoder/emulator work.
- **INITRAMFS-DECISION:** the T679 `vendor.img` CPIO-member install route is
  carried forward, **not executed** in T680.
- **No repo writes:** nothing pushed, nothing committed, no Mac access, no
  deletion outside `/home/muse/t680-toolchain`.

## Required status lines

```
sys.boot_completed: NOT observed in T680 — this lane flies nothing; no guest claim is made.
WALL-TYPE-TAG: none — toolchain proven
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.
```

## Bottom line

The minimal AOSP tree set that yields a service-capable Composer 2.1 toolchain
is **~25 MiB**: `system/tools/hidl` plus `libbase`, `liblog`, `libhwbinder`,
`libhidl`, `libfmq`, `system/core` (libutils/libcutils headers), `external/jsoncpp`,
and `hardware/interfaces` (graphics/common). From it, `hidl-gen` was built and
proven runnable, all 36 Composer 2.1 + support C++ headers were generated, and a
service-shaped translation unit compiled to a real x86-64 ELF object against the
unmodified generated headers. The link gap is enumerated above. **PASS.**
