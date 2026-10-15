# T503 ORN fast-path acceptance gates

**Base:** T499 matched-oracle branch, starting commit `7c11e9d`.
**Fix commit:** `e29771b`.
**Trace evidence commit:** `788357a`.

- [x] **G503-1: ORN honors N in the ORR fast path.**
  - Check: `cargo test -p u12-orchestrator --lib gb25_orn_honors_inverted_shifted_operand -- --nocapture`
  - Expected: 64-bit ORN result `0xffffffffbfffffff`, ORR result `0x40000000`; 32-bit ORN/ORR and shifted ORN cases also pass.
  - Evidence: `orn-focused-test.log` in `/Users/Shared/bundles/t503-ornfix-20261015/evidence/` records the passing focused test. The same test failed before the fix with ORN producing `0x40000000`.
  - Audit: ANDS and BIC fast paths already apply N. U12 has no EOR/EON fast path; those instructions fall through to U2 `EorShift { invert: n }` and U3 complements the shifted operand when `invert` is true.

- [x] **G503-2: Pinned T499 live oracle matches with guest quirks disabled.**
  - Check: `scripts/capture_csinv_qemu.py --live` against the pinned manifest, then `target/release/run_kernel` with `--break-pc ffffff8008354018 --break-hit 2`; omit `--enable-guest-quirks`.
  - Expected: same CSINV word, PC, X8, architectural PSTATE, and X24 transition in QEMU and Rust.
  - Evidence: `qemu-live-reference.log` and `rust-live-oracle-architectural-pstate.log` in the bundle. Both show before PC `0xffffff8008354018`, X8 `0xffffffffbfffffff`, PSTATE `0xa00001c5`; both show after PC `0xffffff800835401c`, X24 `0x40000000`, and the same PSTATE. Rust logs `enabled=false`.
  - PSTATE note: U12 stores NZCV and DAIF separately. The runner diagnostic composes architectural PSTATE from NZCV, DAIF, and EL1h mode for a direct QEMU comparison.

- [x] **G503-3: Ten consecutive accepted pinned-oracle runs.**
  - Check: `10-pass-campaign.log`; each numbered pass freshly captures QEMU live state and runs the release Rust binary against the same pinned guest files with quirks omitted.
  - Expected: all ten QEMU/Rust before-and-after CSINV states match; any run failure or state mismatch ends the campaign.
  - Evidence: all ten pairs of raw logs under `/Users/Shared/bundles/t503-ornfix-20261015/evidence/t499/pass-01/` through `pass-10/`, with hashes in `10-pass-sha256.txt`.
  - Result: 10/10 consecutive passes; no remaining divergence at the pinned CSINV site.

## Additional validation

- Pinned manifest validation passes with `uv run --no-project python scripts/verify_manifest.py`.
- `cargo build --release -p u12-orchestrator --bin run_kernel` succeeds.
- `cargo test --workspace`: 349 passed, 1 failed across 350 tests. The failure is `tests::vmemmap_l1_gap_step_vcpu_instruction_retry`, at the LDRB/MMU translation path (`TranslationFault`); an isolated rerun reproduced the same failure. Full output: `evidence/workspace-test.log` in the bundle. This does not change the three T503 gate results.
- The supplied standing-rules file was not present at its requested home path. Its rules included in the task were followed. No OKF validator script exists in this checkout.
