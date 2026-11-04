# T678 — DNS-cleared Composer 2.1 acquisition branch

**State: FAIL at Step 2 (source acquisition).**

This branch resumed the Composer 2.1 reimplementation question after T677. The disk gate passed with 11 GiB available. T677's base CPIO, kernel, and UART were hardlinked into this worktree and matched their expected SHA-256 values. The embedded `ramdisk-images/system.img` also matched when streamed from the CPIO; QEMU 11.1.2 was present.

The acquisition route was not available in this execution environment. `dscacheutil -q host -a name android.googlesource.com` returned no host record; `nslookup` failed to bind its resolver socket (`Operation not permitted`); HTTPS to googlesource failed with `Could not resolve host`; the allowed GitHub mirror lookup also failed with `Could not resolve host: github.com`.

No `platform/hardware/interfaces` tree or pinned revision was obtained. Therefore the full `graphics/composer/2.1` listing and the existence of `default/ComposerClient.cpp` remain unadjudicated. The exact acquisition record is in `/Users/Shared/agyx-track678-composer-2-1-dns-cleared-acquisition.md`.

No service implementation/build, guest install-mechanism analysis, fix initramfs, or flight was started. `sys.boot_completed`, guest-origin framebuffer commit, first-cycle health, and the T664–T677 regression checks were not observed in this lane.
