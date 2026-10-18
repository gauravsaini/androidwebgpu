# Gates: Track T583 — rust-150m-combined

OWNS: units/**, contracts/**, GATES.md, TIMELINE.md

Scope: Run the 150M-step boot gate on the combined fixed lineage (PSCI_FEATURES HVC ABI fix from t579 + DC ZVA lifting fix from t577) from reset, determine if the Rust engine gets past step 46,172,331 toward /init, record halt telemetry, and deliver findings to /Users/Shared/agyx-track583-rust-150m-combined.md.

- [ ] G1: Both fix commits (39fcc56 from t579 and 11beaa16 from t577) present in branch t583-rust-150m-combined.
  CHECK: git -C /Users/Shared/wt-track583-rust-150m-combined log --oneline -4
  EXPECT: 11beaa1
  EVIDENCE: pending

- [ ] G2: Clean build of run_kernel in separate CARGO_TARGET_DIR target-t583.
  CHECK: test -f /Users/Shared/wt-track583-rust-150m-combined/target-t583/release/run_kernel && echo "EXISTS"
  EXPECT: EXISTS
  EVIDENCE: pending

- [ ] G3: Unit and regression tests pass for both DC ZVA and PSCI_FEATURES.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track583-rust-150m-combined/target-t583 cargo test -p u12-orchestrator --test sysreg_p3 -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: pending

- [ ] G4: Full 150M-step boot gate executed from reset with pinned guest inputs and quirks disabled.
  CHECK: test -f /Users/Shared/agyx-logs/track583-rust-150m.log && echo "LOG_EXISTS"
  EXPECT: LOG_EXISTS
  EVIDENCE: pending

- [ ] G5: Record halt telemetry: halt step, PC, UART byte count + SHA, 1 GiB guest RAM panic scan.
  CHECK: grep -E "Executed|halt|budget" /Users/Shared/agyx-logs/track583-rust-150m.log
  EXPECT: Executed
  EVIDENCE: pending

- [ ] G6: TEN-PASSES verified for regression vectors, TIMELINE.md updated with T583 milestone.
  CHECK: grep -E "Track T583" /Users/Shared/wt-track583-rust-150m-combined/TIMELINE.md
  EXPECT: Track T583
  EVIDENCE: pending

- [ ] G7: Certified findings file /Users/Shared/agyx-track583-rust-150m-combined.md delivered on disk with Verdict PASS/FAIL.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track583-rust-150m-combined.md
  EXPECT: Verdict:
  EVIDENCE: pending
