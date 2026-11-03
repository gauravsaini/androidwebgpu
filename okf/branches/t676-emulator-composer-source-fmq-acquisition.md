# T676 — Emulator composer source acquisition and FMQ fix

**Verdict: FAIL — Step 2.1 source identification/acquisition wall.**

**WALL-TYPE-TAG:** guest (type-1), Android Composer 2.1 source acquisition.

**Rust par bhi lagu: haan — Composer FMQ parsing and client-target dispatch run inside the shared Android guest service, regardless of whether QEMU or the Rust emulator hosts the guest.**

## Gate and inputs

Step 0 passed: `df -h /System/Volumes/Data` reported 10 GiB available (8 GiB required), 95% capacity. The lane proceeded.

Step 1 verified the already-staged `guest-inputs/` files in this worktree. The former T675 worktree was absent, so verification used the driver-staged files here. Every expected SHA matched:

| Input | Bytes | Expected SHA-256 | Verified SHA-256 | Result |
|---|---:|---|---|---|
| `guest-inputs/t670-base-image.cpio.gz` | 954,450,678 | `afb2991f59288d1de1b9855b6cbcb6e8050bde3e9a23d2391d3f17c892a3d022` | `afb2991f59288d1de1b9855b6cbcb6e8050bde3e9a23d2391d3f17c892a3d022` | MATCH |
| `guest-inputs/Image` | 23,073,280 | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | MATCH |
| Embedded `ramdisk-images/system.img` | streamed from CPIO | `7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08` | `7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08` | MATCH |
| `/opt/homebrew/bin/qemu-system-aarch64` | — | not pinned in brief | `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869` | QEMU 11.1.2 |
| `guest-inputs/t673-uart.log` | 15,700,153 | `67ce242c21f8d3471580c3d1a8e9cb4bce95d286747417c42b0efd6dad8f464d` | `67ce242c21f8d3471580c3d1a8e9cb4bce95d286747417c42b0efd6dad8f464d` | MATCH |

The embedded system image was extracted to stdout with `bsdtar -xOf` and hashed without creating a second disk copy. The runtime service at `vendor/bin/hw/android.hardware.graphics.composer@2.1-service` was also streamed from the base and matched T674's ELF SHA-256 `5246e2844230e6884398de0845fc49a84e92085adfdbd050804ccddba12c0eca`.

## Step 2.1 — AOSP and public-source search

The actual Android 14 guest fingerprint is `google/sdk_gphone64_arm64/emu64a:14/UE1A.230829.050/12077443:userdebug/dev-keys` (T675 record). I searched AOSP Gitiles and indexed public source for the exact symbols `SimpleFBClient`, `SimpleFBCarveout`, and `blitFromBuffer`, plus the Composer 2.1 service implementation. Exact public searches for `SimpleFBClient`, `SimpleFBCarveout`, and `SimpleFBClient::executeCommands` returned no relevant source. `cs.android.com`'s direct search endpoint returned a cache miss. The `blitFromBuffer` hit was unrelated SwiftShader code.

| Candidate | Project path and revision | Finding |
|---|---|---|
| Emulator Composer 2.1 package config | `device/generic/goldfish/vendor.mk`, commit `3a99469f38fa5567c550b42088a9905cc22f4ada` | `PRODUCT_PACKAGES` names `android.hardware.graphics.composer@2.1-service`, `android.hardware.graphics.composer@2.1-impl`, and `hwcomposer.ranchu`. This establishes the public package name, but does not expose the binary's `SimpleFBClient` implementation. [Gitiles](https://android.googlesource.com/device/generic/goldfish/%2B/3a99469f38fa5567c550b42088a9905cc22f4ada/vendor.mk) |
| Android 14 arm64 lunch product | `device/generic/goldfish/64bitonly/product/sdk_phone64_arm64.mk`, tag `android-14.0.0_r43`, blob `7fcb2fd9029fe0d827c417570bdfed6789b7e742` | Defines `PRODUCT_NAME := sdk_phone64_arm64` and `PRODUCT_DEVICE := emu64a`; inherits the emulator vendor config. [Gitiles](https://android.googlesource.com/device/generic/goldfish/%2B/refs/tags/android-14.0.0_r43/64bitonly/product/sdk_phone64_arm64.mk) |
| Android 14 arm64 vendor package chain | `device/generic/goldfish/vendor_common.mk`, same tag, blob `a0d5479f0e40fdbac46b904a9cce897664d2a657` | This public Android 14 release config installs `android.hardware.graphics.composer3-service.ranchu`, not `android.hardware.graphics.composer@2.1-service`. It therefore does not match the service in the T670 guest. [Gitiles](https://android.googlesource.com/device/generic/goldfish/%2B/refs/tags/android-14.0.0_r43/vendor_common.mk) |
| Generic Composer 2.1 service build | `platform/hardware/interfaces/graphics/composer/2.1/default/Android.bp`, `main`, blob `91d9b0d4dbf4457114f88c1ba3ff6b319aa9e513`; `service.cpp`, blob `1276d2df7ee940bf55f5833d11edeeceb4b81bd5` | Builds the same service module name from `service.cpp`; that launcher loads `HwcLoader`. It is a generic passthrough service, not the `SimpleFBClient` code identified in the guest ELF. [Android.bp](https://android.googlesource.com/platform/hardware/interfaces/%2B/refs/heads/main/graphics/composer/2.1/default/Android.bp), [service.cpp](https://android.googlesource.com/platform/hardware/interfaces/%2B/refs/heads/main/graphics/composer/2.1/default/service.cpp) |
| Generic Composer 2.1 command client | `platform/hardware/interfaces/graphics/composer/2.1/default/ComposerClient.cpp`, commit `f06d4203accc96302c5c6e7287bea5fd7a563f08` | Its `ComposerClient::executeCommands` reads the FMQ queue, parses commands, and writes output; it is the generic HIDL reference implementation. The class and implementation do not match the guest's `SimpleFBClient`. [Gitiles](https://android.googlesource.com/platform/hardware/interfaces/%2B/f06d4203accc96302c5c6e7287bea5fd7a563f08/graphics/composer/2.1/default/ComposerClient.cpp) |
| Composer 2.1 interface contract | `platform/hardware/interfaces/graphics/composer/2.1/IComposerClient.hal`, `master` | Specifies that `executeCommands` consumes the input queue and writes result commands to the output queue. It provides the contract, not the emulator's SimpleFB implementation. [Gitiles](https://android.googlesource.com/platform/hardware/interfaces/%2B/master/graphics/composer/2.1/IComposerClient.hal) |
| Goldfish emulator HWC implementation | `device/generic/goldfish-opengl/system/hwc2/Android.mk`, commit `8f36a52236e43d1a9d086978f62eef95707c6ef5` | Builds `hwcomposer.ranchu` from `Device.cpp`, `Display.cpp`, `GuestComposer.cpp`, `HostComposer.cpp`, and related HWC files. It is not the Composer 2.1 service source and contains no SimpleFB client/carveout implementation. [Gitiles](https://android.googlesource.com/device/generic/goldfish-opengl/%2B/8f36a52236e43d1a9d086978f62eef95707c6ef5/system/hwc2/Android.mk) |
| `blitFromBuffer` public-source hit | `SwiftShader/src/Device/Blitter.cpp`, commit `32f4be1d9bf37331852197657d0e0b1d5f413efb` | Unrelated graphics library; the change removes unused `blitFromBuffer` methods. Ruled out as the emulator composer source. [Gitiles](https://swiftshader.googlesource.com/SwiftShader/%2B/32f4be1d9bf37331852197657d0e0b1d5f413efb) |

## First failing link and stop

**Step 2.1 is the first failed link.** The public AOSP candidates explain the Composer 2.1 interface, generic passthrough service, and Goldfish HWC, but no searched AOSP/public-source project or revision contains the binary's `SimpleFBClient`, `SimpleFBCarveout`, and matching FMQ-to-client-target dispatch. The Android 14 public product config also names Composer 3 where this guest carries the Composer 2.1 ELF. This is an acquisition wall; it does not establish that the target source is absent from every non-public Google tree.

T674's local binary evidence pins the gap: `SimpleFBClient::executeCommands` is at `0x687ec`; the empty-handle branch returns success with zero output words without reading or writing either FMQ; the one `FramebufferDevice::blitFromBuffer` call at `0x688a8` is conditional on nonempty handles, not parsed `PRESENT_DISPLAY`. See [`evidence/t674_composer_binary_audit.md`](../../evidence/t674_composer_binary_audit.md), lines 20–41. The verified service ELF SHA is recorded at lines 10–16.

Step 2.2 did not download a tree because no matching implementation project/revision was identified. No download occurred, so no post-download disk check was applicable. Step 2.3 was not attempted: the acquired AOSP candidates do not satisfy the required source-plus-Composer-2.1-header set, and a generic NDK build would not prove that gate.

Steps 3 and 4 were not started. No fix initramfs was built, no QEMU flight or fresh UART growth/PID evidence exists, and `sys.boot_completed` is **not observed for T676**. No guest-origin frame commit or first-cycle bar was measured. T664–T675 regressions were not re-run. Inherited flight artifacts are not T676 evidence.

## Source search record

- AOSP Gitiles/public indexed exact searches: `SimpleFBClient`; `SimpleFBCarveout`; `blitFromBuffer` with composer/ranchu; `android.hardware.graphics.composer@2.1-service` with emulator; `sdk_gphone64_arm64` product config.
- Public-source exact queries: `"SimpleFBClient" Android`, `"SimpleFBCarveout"`, `"blitFromBuffer" "SimpleFB"`, `"SimpleFBClient::executeCommands"`, and `site:github.com "SimpleFBClient"`.
- Direct AOSP candidate paths/revisions are listed above. The exact `SimpleFB*` searches returned no relevant public source results; generic Composer 2.1 results were retained as ruled-out references rather than treated as the target implementation.
