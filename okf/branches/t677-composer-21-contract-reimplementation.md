# T677 — Composer 2.1 contract reimplementation and FMQ fix

**Verdict: FAIL — Step 2 source acquisition.**

**WALL-TYPE-TAG:** guest (type-1), Android Composer 2.1 contract/reference acquisition; host DNS resolution blocked the pinned source fetch.

**Rust par bhi lagu:** haan — the Composer 2.1 service runs inside the Android guest regardless of whether QEMU or the Rust emulator hosts that guest.

## Branch finding

T677 is a separate branch from T676. T676 searched for the guest-specific SimpleFB implementation; T677 began the public Composer 2.1 reimplementation route. The official project tree metadata was obtained at `android14-release` commit `40e9f1537e308ed49e3b561ce333e3f2bb64f31e` and `main` commit `1a56e38edc2f2f6189ef405ee1edce554e15cbc0`, but neither tree listed `graphics/composer/2.1/default/ComposerClient.cpp`. The reference commit recorded by T676, `f06d4203accc96302c5c6e7287bea5fd7a563f08`, could not be fetched because `android.googlesource.com` did not resolve. The complete Step 2 source set was therefore not acquired at one pinned revision.

## Evidence and stop

- Step 0 passed: `/System/Volumes/Data` had 12 GiB available initially, then 11 GiB before/after source attempts.
- Step 1 passed: T670 base, canonical kernel, embedded system image, QEMU 11.1.2 binary, and T673 UART hashes matched; the guest-input files share inodes with T676.
- Step 2 is the first failed link. Exact attempts, project paths, commit IDs, hashes, and DNS errors are in `/Users/Shared/agyx-track677-composer-2-1-contract-reimplementation.md`.
- Steps 3 and 4 were not started: no service build, init-script install analysis, fix initramfs, or QEMU flight. `sys.boot_completed` and guest-origin frame commit were not observed; T664–T676 regressions were not rerun.
- `DEATH_NOTE-T677.md` records why execution stopped. Outcome is `FAIL`.
