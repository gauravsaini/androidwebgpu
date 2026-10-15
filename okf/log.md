# OKF log

## 2026-10-15 — T499 matched-oracle branch

- Added `branches/t499-matchedoracle.md` as a separate experiment-family record; kept serial and register captures in `evidence/t499/` as raw observations.
- Pinned the guest input hashes, boot configuration, native QEMU version, and Rust source commit in `MANIFEST.json`.
- Captured one manifest-bound native QEMU boot through the guest `/init` marker and confirmed the guest initramfs rootfs marker.
- Compared the exact CSINV word in live and matched-state executions. The live second hit differed in PSTATE before execution; both engines agreed in the matched-state control.
- Ran the approved DAFG ledger; G499-1, G499-2, and G499-3 are all MET.
- Marked the live differential as an engine wall in `files/WALLS.md`, scoped to this single capture. No 10-pass reproducibility claim is made.
- Made the memblock store rewrite and vmemmap-on-fault population handler opt-in through `--enable-guest-quirks`.
