# DEATH_NOTE — T681

- Lane: T681 — Composer 2.1 link gap: bionic vendor ELF on Linux (gsai box)
- Date of death: 2026-11-04
- Cause of death: closure by completion (PASS). No wall killed this lane.
- Workdir: `/home/muse/t681-link`
- Verdict: PASS (`outcome-t681.txt`)

## What this lane proved

A Composer 2.1 HIDL vendor service exists as a linked AArch64 Android binary built
entirely against bionic, not as a host x86-64 object:

- 12 generated interface sources + 51 headers from the T680 `hidl-gen`, regenerated
  byte-identically once into a scratch tree.
- NDK r29 AArch64 compiler + bionic sysroot (API 34), checksum
  `4abbbcdc842f3d4879206e9695d52709603e52dd68d3c1fff04b3b5e7a308ecf`.
- 15 static AOSP libraries rebuilt for AArch64 (liblog, libbase, libcutils, libsync,
  libvndksupport, libutils, libhwbinder, libfmq, libhidlbase, the three generated
  interface libraries, and hidl-manager 1.0/1.1/1.2).
- One linked PIE `ET_DYN` vendor ELF, 19,941,040 bytes,
  sha256 `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132`,
  `Machine: AArch64`, `PT_INTERP /system/bin/linker64`, `NEEDED` only
  `libdl.so`/`libm.so`/`libc.so`, all strong imports bionic-versioned
  (`LIBC`, `LIBC_O`, `LIBC_Q`, `LIBC_R`), no host glibc.
- Entry points `main`, `IComposer::registerAsService`,
  `ComposerImpl::createClient`, `ComposerClientImpl::executeCommands` present.

## What this lane did NOT prove

- No boot, no flight, no execution on a device or emulator.
- `sys.boot_completed` was NOT observed; no guest claim is made.
- No frame commit, no display output, no service registration on a live hwservicemanager.
- The T679 vendor.img CPIO-member install route was carried forward to T682, not run.
- Runtime linking against the device's bionic was not exercised. The NDK r29 sysroot
  targets API 34; the device `linker64` must be at least API 30 for `LIBC_R` unwinder
  tags and API 34-compatible for the rest.
- simplefb geometry is an assumption (1280x720, stride = width, 32 bpp,
  phys `0x5F000000`) taken from the T673/T674 diagnosis; it is env-overridable but
  unverified on the target panel.
- Vendor sepolicy for `/dev/mem` access was not written or tested.

## Assumptions that keep this result alive

1. `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service` is the correct
   install path on the target (T679 path).
2. The target kernel exposes the panel framebuffer at `0x5F000000` with the assumed
   geometry, and `/dev/mem` is readable by the service under the device sepolicy.
3. The device's bionic exports the API 30+ property symbols and the `LIBC_R`
   unwinder symbols used by the static libc++.
4. The FMQ command wire format is the staged
   `composer-command-buffer/2.1/ComposerCommandBuffer.h` format; a different
   composer client would invalidate the executeCommands decoder.
5. HIDL interface version `android.hardware.graphics.composer@2.1` matches what the
   target's graphics stack asks for.

## What would invalidate this result

- Executing the ELF on the target and getting a wrong-ELF/`ENOEXEC`, missing symbol,
  or linker-version failure.
- Device bionic lacking `__system_property_wait`, `__system_property_read_callback`,
  `_Unwind_*@LIBC_R`, or the `LIBC_Q` fdsan symbols.
- A composer client that negotiates a different interface version or FMQ layout.
- Framebuffer geometry differing from the env defaults without override.

## Evidence kept on disk

`findings-t681.md`, `outcome-t681.txt`, `logs/` (disk probe, generation, toolchain,
build, link, and verification outputs), `libs/`, `generated-sources/`,
`generated-headers/`, `t681-service.cpp`, `t681-dl-shim.cpp`,
`build-libs.sh`, `build-service.sh`, and the linked ELF under `out/`.

## Hand-off

T682 on the Mac: take the ELF through the T679 vendor.img CPIO-member replacement
route, boot, and prove guest consumption (`sys.boot_completed` plus a real frame
commit). No repo commit or push was performed by this lane; bookkeeping belongs to
the driver at certification.

— lane closed, 2026-11-04
