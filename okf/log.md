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
