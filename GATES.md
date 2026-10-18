# Gates: Track T579 — rust-next-div2

OWNS: units/u2-ir-lift/src/**, units/u1-decode/src/**, units/u3-wasm-jit/src/**, units/u12-orchestrator/src/**, units/u12-orchestrator/tests/**, units/u15-exec-wasmtime/src/**, units/u15-exec-wasmi/src/**, contracts/src/**, GATES.md, TIMELINE.md, scripts/**

Scope: Run periodic aligned-diff sweep past step 36,830,000 on T575-fixed lineage, locate first engine-state divergence vs pinned QEMU oracle, root-cause and eliminate with QEMU-matched regression vector per touched class, verify with aligned-diff rerun, and deliver /Users/Shared/agyx-track579-rust-next-div2.md.

- [x] G1: Base commit verified 2369103 on branch t579-rust-next-div2.
  CHECK: git -C /Users/Shared/wt-track579-rust-next-div2 rev-parse --short HEAD
  EXPECT: 2369103
  EVIDENCE: Verified HEAD is 2369103049e6e25db8583a134a8a0cb521cfb8b8 on branch t579-rust-next-div2.

- [x] G2: Clean build of run_kernel in separate CARGO_TARGET_DIR target-t579.
  CHECK: test -f /Users/Shared/wt-track579-rust-next-div2/target-t579/release/run_kernel && echo "EXISTS"
  EXPECT: EXISTS
  EVIDENCE: Built /Users/Shared/wt-track579-rust-next-div2/target-t579/release/run_kernel in 2m 39s.

- [x] G3: Run periodic aligned-diff sweep from 36,830,000 upward (1M cadence, dense sweep around divergence) against pinned QEMU trace; locate first engine divergence.
  CHECK: python3 scripts/watch_diff.py
  EXPECT: Divergence step identified
  EVIDENCE: Checkpoints 36,830,000, 36,840,000, and 36,850,000 matched 100% bit-for-bit. Divergence identified at QEMU step 36,860,000. Dense trace window (36,849,990..36,860,010) pinned exact onset to QEMU step 36,852,812 / Rust step 36,852,806 at PC 0xffffff8008084874 (HVC #0, FID X0=0x8400000a PSCI_FEATURES, arg X1=0xc4000001 PSCI_0_2_FN64_CPU_SUSPEND): QEMU returned X0=0 (PSCI_RET_SUCCESS), pre-fix Rust returned X0=-1 (PSCI_RET_NOT_SUPPORTED).

- [x] G4: Root-cause against Cortex-A53 / QEMU oracle, fix in Rust engine, add QEMU-matched regression vector.
  CHECK: cargo test -p u12-orchestrator --test sysreg_p3 p3_psci
  EXPECT: passed
  EVIDENCE: Updated HostOps::hvc and FUNC_HVC ABI to accept (func_id, arg1). Aligned PSCI_FEATURES (0x8400000a) in units/u12-orchestrator/src/lib.rs, contracts, u3-wasm-jit, u15-exec-wasmtime, and u15-exec-wasmi to match Cortex-A53 QEMU virt oracle (target/arm/tcg/psci.c:178-204). Added QEMU regression tests p3_psci_features_cpu_suspend_hvc_matches_cortex_a53_qemu, p3_psci_features_unsupported_hvc_matches_cortex_a53_qemu, and p3_psci_features_unknown_64_hvc_matches_cortex_a53_qemu in units/u12-orchestrator/tests/sysreg_p3.rs.

- [x] G5: Aligned diff rerun confirms divergence is eliminated with no earlier regressions introduced.
  CHECK: python3 scripts/aligned_diff_50k.py /Users/Shared/agyx-logs/track579-qemu-36.83m-37.0m.log /Users/Shared/agyx-logs/track579-rust-36.85m-36.86m-fixed.log 6
  EXPECT: ALL 10021 ALIGNED STEPS MATCH 100% BIT-FOR-BIT!
  EVIDENCE: Ran fixed binary from snapshot snap-t579-pre.bin.36845000 across window 36,849,990..36,860,010: all 10,021 instructions match 100% bit-for-bit across all 31 GPRs, SP, CPSR, SCTLR, TTBR0, TCR, TTBR1, MAIR. Subsequent periodic check confirmed 100% match through step 36,930,000 (80k+ steps past divergence).

- [x] G6: TEN-PASSES verified, TIMELINE.md updated, conventional commit on branch t579-rust-next-div2.
  CHECK: 10 consecutive passes of cargo test -p u12-orchestrator --test sysreg_p3 p3_psci
  EXPECT: PASS 10/10
  EVIDENCE: Verified 10 consecutive test passes (PASS 10/10). TIMELINE.md updated with dated entry. Conventional commit created.

- [x] G7: Certified findings file /Users/Shared/agyx-track579-rust-next-div2.md delivered on disk with Verdict PASS/FAIL.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track579-rust-next-div2.md
  EXPECT: Verdict: PASS
  EVIDENCE: Findings file delivered at /Users/Shared/agyx-track579-rust-next-div2.md with Verdict: PASS, full claim, scope, witness, input hashes, WALL-TYPE-TAG, and "Rust par bhi lagu: nahi".
