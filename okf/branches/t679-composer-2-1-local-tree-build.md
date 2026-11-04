# T679 — Composer 2.1 local-tree build branch

**State: FAIL at Step 3 (service-capable Android build environment).**

This branch continues T678 with the driver-staged `platform/hardware/interfaces` tree. The T670 base CPIO, canonical kernel, embedded system image, QEMU 11.1.2 executable, and T673 reference UART matched their pinned hashes; guest inputs are hardlinks from T678. The staged tree HEAD is `40e9f1537e308ed49e3b561ce333e3f2bb64f31e`, with the Composer 2.1 HAL contract and utility headers present.

The service build stopped because only a generic NDK and host `clang++` are available. `m`, `mm`, `lunch`, Soong, `hidl-gen`, and generated Composer HIDL C++ headers are absent. The service `Android.bp` links Composer 2.1, FMQ, and HIDL platform libraries. Static init/fstab inspection identified the embedded read-only `vendor.img` as the replacement install target; no image was changed.

A brief-requested local `git show` of supplementary f06d4203 blobs triggered an automatic lazy-fetch attempt from the partial clone's promisor remote. DNS failed before any blob was obtained; no further network calls were made. The execution deviation and complete evidence are recorded in `/Users/Shared/agyx-track679-composer-2-1-local-tree-build.md`.

No service, fix initramfs, or QEMU flight was produced. `sys.boot_completed`, guest-origin frame commit, first-cycle health, and T664–T678 regression status remain unobserved. The full Step 0–4 record is in the findings file.
