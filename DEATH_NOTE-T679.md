# T679 death note

**Stopped at:** Step 3, the service-capable Composer 2.1 build gate.

**Evidence:** The staged `android14-release` tree and required Composer 2.1 contract/utilities were verified at `40e9f1537e308ed49e3b561ce333e3f2bb64f31e`. The host has a generic NDK wrapper and host `clang++`, but no Soong entrypoint, `hidl-gen`, generated Composer HIDL C++ headers, or full Android platform source tree. The service `Android.bp` requires the Composer 2.1 interface and Android HIDL/FMQ libraries. No service build was possible under the brief's service-capable requirement.

Static init/fstab inspection identified an install route through the embedded read-only vendor ext4 image: replace `ramdisk-images/vendor.img` with an image containing the replacement at `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service`, then repack the initramfs. This would leave `system.img`, boot-classpath JARs, and the T667–T670 preparation script intact. No image was modified.

The brief-requested supplementary `git show` exposed a partial-clone lazy-fetch attempt for missing f06d4203 blobs. DNS failed before any blob was fetched; no further network calls were made. This deviation is recorded in the full findings.

**Effect:** No replacement service, fix initramfs, or flight exists. `sys.boot_completed`, guest-origin frame commit, first-cycle health, and T664–T678 regression status were not observed.

**Verdict:** FAIL — Android Composer 2.1 service build toolchain wall. Full evidence: `/Users/Shared/agyx-track679-composer-2-1-local-tree-build.md`.
