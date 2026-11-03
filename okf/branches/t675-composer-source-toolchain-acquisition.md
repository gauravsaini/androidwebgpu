# T675 — SimpleFB composer source and toolchain acquisition

**Verdict: FAIL — acquisition gate, before image build or guest flight.**

**WALL-TYPE-TAG:** guest (type-1). The missing artifact is the Android SimpleFB Composer 2.1 service source/build environment.

**Rust par bhi lagu: nahi.** No Rust engine ran; this is scoped to guest-side Android source and toolchain availability.

## Gate outcome

All five Step 1 inputs matched their pinned hashes: the T670 initramfs, canonical kernel, embedded `system.img`, QEMU 11.1.2 executable, and T673 UART. The verified system/vendor fingerprint is `google/sdk_gphone64_arm64/emu64a:14/UE1A.230829.050/12077443:userdebug/dev-keys`; it does not identify a source checkout.

The first failed link is Step 2.1. The service at `vendor/bin/hw/android.hardware.graphics.composer@2.1-service` is only available as a 2,223,240-byte ELF (SHA-256 `5246e2844230e6884398de0845fc49a84e92085adfdbd050804ccddba12c0eca`). Repository/history and local source searches found no SimpleFB implementation source or build recipe. The AOSP Composer 2.1 `IComposerClient.hal` and Apache-2.0 `ComposerClient.cpp` provide the matching interface contract/reference: executeCommands reads FMQ commands, parses them, and writes result commands; the default implementation is not the missing SimpleFB source.

NDK r29 (`29.0.14206865`) passed a generic AArch64 Android shared-object build. A Composer interface header check failed because `<android/hardware/graphics/composer/2.1/IComposerClient.h>` is absent, and no AOSP platform build tree/prebuilts were found. Thus no service-capable toolchain was proved.

Local candidate ruling: `/private/tmp/simplefb_composer` and the T674 inspection path are duplicate service ELFs; `/private/tmp/simplefb_allocator` is an allocator ELF; the T585 candidate is an assembly-patched ELF; the local Composer 3 stub is AIDL/Composer 3; the Android 14 GSI archive contains images and build properties only; the main tree's `hwcomposer.virtio_gpu.cpp` is unrelated HWC1/WebGPU code. Exact source filename, AOSP checkout path, native source-content, and build-recipe searches found no matching source.

Full input hashes, command evidence, candidate sizes and hashes, source references, and search scope are in `/Users/Shared/agyx-track675-composer-source-and-toolchain-acquisition.md`.

## Stop record

No fix initramfs was made, no QEMU flight was started, `sys.boot_completed` is not observed for T675, and the first-cycle/display/regression checks were not measured. T673 evidence is only the verified reference input; it is not a T675 flight.

The requested commit was attempted with `user.name=Gaurav Saini` and `user.email=me@gauravsaini.com`, but Git could not create `/Users/Shared/androidwebgpu/androidwebgpu/.git/worktrees/wt-track675-composer-source-and-toolchain-acquisition/index.lock` (`Operation not permitted`). Changes remain staged; nothing was pushed.
