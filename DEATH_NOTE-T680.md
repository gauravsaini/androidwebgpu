# DEATH_NOTE-T680

Lane: T680 — Composer 2.1 toolchain feasibility on Linux (gsai box)
Date: 2026-11-04
Status: **PASS** (lane closed cleanly; not a death/failure)

## What this lane set out to prove

That the gsai Linux box can host the minimal AOSP tree set needed to:
build `hidl-gen`, generate the Composer 2.1 C++ headers from the staged
`android14-release` HAL, and compile a service-shaped translation unit to
object files. A full service link was explicitly out of scope.

## What was proven

- Disk gate: 27 Gi available at `/home/muse` (>= 20 Gi).
- Staged tree intact: HEAD `40e9f1537e308ed49e3b561ce333e3f2bb64f31e`,
  branch `android14-release`; all five required files present.
- Egress: `android.googlesource.com` reachable (exit 0). GitHub root returns
  128 ("not a repository"); AOSP — the needed source — works. No retry spent.
- Minimal fetch: 10 trees / ~25 MiB total, under the 20 GiB cap. No full
  `repo sync`.
- `hidl-gen` built with g++ 15.2 + flex 2.6.4 + bison 3.8.2 at
  `/home/muse/t680-toolchain/build-t680/hidl-gen` (5,170,504 bytes,
  SHA-256 `e9b8f732ece1195f6f747762a57e256ef4ce03472a1c13203fdc1a622b92ab95`),
  proven runnable via `-h` and successful `-L hash`.
- 36 C++ headers generated, including all 17 Composer 2.1 headers
  (`IComposer.h`, `IComposerClient.h`, `types.h`, `hwtypes.h`, Bn/Bp/Bs, IHw*)
  and the 19 common/base/manager support headers they include.
- Service TU `t680-service-probe.cpp` compiled (compile-only, `-c`) to
  `build-t680/service-obj/t680-service-probe.o` (174,792 bytes, x86-64 ELF)
  using clang 20.1.8 + libc++ against unmodified AOSP headers. Object symbols
  reference `IComposer::getService` and `IComposer::registerAsService`.
- Link gap documented: `libhidlbase`, `libfmq`, `libhwbinder`, `libbase`,
  `liblog`, `libutils`, `libcutils`, `libbinder`, `libhardware`, `libsync`,
  `libhwc2on1 adapter`s, the generated Composer 2.1 interface lib, its
  `-resources` lib, and the Android target libc++/bionic sysroot.

## What was NOT done (and is not claimed)

- No service link. The object is compile-only and its HIDL entry points are
  still undefined.
- No full platform sync.
- No boot, no guest run, no injection. `sys.boot_completed` was NOT observed.
- No push, no commit, no Mac access, no deletion outside the workdir.
- The T679 `vendor.img` CPIO-member install route is carried forward, not
  executed.

## Handoff to the next lane

Generate the interface sources with `hidl-gen -L c++-sources`, build/link the
libraries listed in the link-gap section of `findings-t680.md`, and use an
Android target toolchain (or full Soong tree) so the final ELF is a bionic
vendor binary rather than a host x86-64 object. Then apply the T679
`vendor.img` CPIO-member replacement route.

## Required tags

```
sys.boot_completed: NOT observed in T680 — this lane flies nothing; no guest claim is made.
WALL-TYPE-TAG: none — toolchain proven
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.
```

## Artifacts on disk

- `/home/muse/t680-toolchain/findings-t680.md`
- `/home/muse/t680-toolchain/outcome-t680.txt` (contains only `PASS`)
- `/home/muse/t680-toolchain/DEATH_NOTE-T680.md`
- `/home/muse/t680-toolchain/build-t680/hidl-gen`
- `/home/muse/t680-toolchain/build-t680/build.sh`
- `/home/muse/t680-toolchain/generated-t680/` (36 headers)
- `/home/muse/t680-toolchain/t680-service-probe.cpp`
- `/home/muse/t680-toolchain/build-t680/service-obj/t680-service-probe.o`
