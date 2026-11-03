# OKF log

## 2026-10-15 — T499 matched-oracle branch

- Added `branches/t499-matchedoracle.md` as a separate experiment-family record; kept serial and register captures in `evidence/t499/` as raw observations.
- Pinned the guest input hashes, boot configuration, native QEMU version, and Rust source commit in `MANIFEST.json`.
- Captured one manifest-bound native QEMU boot through the guest `/init` marker and confirmed the guest initramfs rootfs marker.
- Compared the exact CSINV word in live and matched-state executions. The live second hit differed in PSTATE before execution; both engines agreed in the matched-state control.
- Ran the approved DAFG ledger; G499-1, G499-2, and G499-3 are all MET.
- Marked the live differential as an engine wall in `files/WALLS.md`, scoped to this single capture. No 10-pass reproducibility claim is made.
- Made the memblock store rewrite and vmemmap-on-fault population handler opt-in through `--enable-guest-quirks`.

## 2026-10-15 — T503 ORN fast-path branch

- Added `branches/t503-ornfix.md` as a separate experiment-family record; retained T499's original single-capture findings and its raw observations.
- Root-caused the prior live-state mismatch to ORN word `0xaa2103e8`: the U12 ORR fast path decoded but ignored N. Fixed 32-bit and 64-bit ORR/ORN handling and added direct ORN/ORR plus shifted-form coverage.
- Audited logical-register fast paths: ANDS and BIC already honored N; EOR/EON had no U12 fast path and the U2/U3 slow path applies inversion.
- Rebuilt the Rust runner and matched the pinned QEMU live CSINV state with guest quirks disabled; captured ten consecutive QEMU/Rust matching runs.
- Added `GATES-T503.md`; G503-1, G503-2, and G503-3 are MET. The full workspace test has one separate LDRB/MMU translation-path failure, recorded with the run output.

## 2026-10-16 — T506 fresh Rust PID-1 boot branch

- Started a separate full-boot experiment from T503's fixed revision; recorded the manifest-pinned Image, DTB, and initramfs hashes and verified all three files are present and matching.
- Kept T503's 10/10 oracle result scoped to the ORN/CSINV state gate; T506 requires a guest-originating PID-1 UART marker and ten fresh Rust runs before any reproducibility claim.
- Preserved the T503 full-suite result: 349 passed and one unrelated LDRB/MMU translation test failed.
- Added `GATES-T506.md`; boot outcome remains pending at this checkpoint.

## 2026-10-23 — T626 M3 runner fix (Chain B, engine)

- The runner now ignores the kernel's `/init` handoff printk and watches for PID-1 marker bytes in guest UART output. Marker-gate regression ran red then green; the focused marker and futex-vector tests pass. Commits: `1af4334`, `051c85c`, and `9d55dc2`.
- QEMU's `0xffffff8008c85a8c` futex resume value is Linux's exception-table fixup PC after ERET, not saved `x30`. Attempt 1 captured ESR `0x96000005`, FAR `0`, ELR `0xffffff80082591e4` at step `49,693,869`, with LR `0xffffff800945f488`; it ended at 52M before PID-1 and does not establish that T612 `/init` is silent. Gzip UART and outcome are kept under `/Users/Shared/t626-flight-attempt1/`.
- The corrected runner records and pairs a visit to the fixup PC with the exact abort. Its final cold boot uses the pinned T612 Image, DTB, and initramfs at their verified SHA-256 values; no replacement initramfs has been created. PID-1 marker, fixup resume, trap inventory, and verdict are still pending under `/Users/Shared/t626-flight/`.

## 2026-10-23 — T626 M3 runner-fix branch

- Opened T626 as a separate Chain B engine branch. The runner marker predicate now ignores the kernel handoff printk and watches guest PID-1 UART markers; the red-green regression is complete.
- Cold-boot evidence is complete under `/Users/Shared/t626-flight/`. T612's Image, DTB, and initramfs hashes matched preflight, and the runner remains isolated from the pinned T612 worktree. The runner continued past the kernel handoff but halted before PID 1 at step `147106780`, PC `0xffffff800808460c`, on unsupported `LDP Q0,Q1,[X0]` (`0xad400400`). Verdict is FAIL for PID-1 attestation; do not call the initramfs silent because `/init` never ran.
- The futex vector passed exactly once, with its Linux fixup PC observed 500 steps later; counts were `da_in=1`, `brk_in=0`. UART was gzip-compressed at flight end. No replacement initramfs was created because it could not be consumed before resolving the Q-pair engine blocker. Full evidence and hashes are in `okf/branches/t626-m3-runner-fix.md` and `/Users/Shared/codex-track626-m3-runner-fix.md`.

## 2026-11-03 — T674 composer FMQ recovery branch

- Added `branches/t674-composer-fmq-blit.md` as a separate display-wall recovery branch. Verified the T670 base, kernel, embedded system image, QEMU, and T670/T672/T673 reference log hashes before any boot; no T674 boot occurred.
- Rechecked T673's first `presentOrValidate` failure at raw line 43788 and its 555-failure series. Audited the composer ELF inside `vendor.img`: the empty-handle path skips both FMQs and reports zero output words. The ELF has one conditional static `blitFromBuffer` call at `0x688a8`, correcting T673's “zero callers” static claim without rewriting that historical record.
- No fix image was built because the SimpleFB implementation and Android build dependencies are absent. T674's flight and first-cycle checks remain unobserved; its failure and death note are recorded separately.
- The required Git commit could not be created: the workspace sandbox denied the shared worktree `index.lock` write. T674 notes remain uncommitted in the worktree.

## 2026-11-03 — T675 composer source/toolchain acquisition branch

- Verified all five pinned Step 1 input hashes, including the embedded `system.img` streamed from the T670 base CPIO and the T673 UART from its reference worktree.
- Confirmed the Android 14 emulator build fingerprint, then exhausted repository/history and local source searches. The only SimpleFB service artifacts found are duplicate ELF binaries; AOSP's licensed Composer 2.1 code is a contract/reference, not the SimpleFB source.
- NDK r29 built a generic AArch64 Android shared object, but a Composer 2.1 header check failed because generated HIDL platform headers are absent; no AOSP platform build tree was found.
- Stopped at Step 2 with an evidenced FAIL. No fix image or T675 flight exists; boot, frame commit, first-cycle, and regression vectors remain unobserved. Full search record is in the T675 findings file.
- Commit with the requested author identity was attempted and blocked by `Operation not permitted` creating the shared worktree's `.git/worktrees/.../index.lock`; changes remain staged and nothing was pushed.
