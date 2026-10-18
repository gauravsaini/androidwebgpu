# Gates: Track T570 — rust-40k-mismatch

OWNS: units/u2-ir-lift/src/lib.rs, units/u12-orchestrator/src/bin/run_kernel.rs, units/u12-orchestrator/tests/sysreg_p3.rs, GATES.md, GATES-T570.md, TIMELINE.md, /Users/Shared/agyx-track570-rust-40k-mismatch.md

Scope: Locate and fix the engine-state mismatch at Rust step 39,994 / QEMU step 40,000, add QEMU-matched regression vectors, verify no divergence <= 40,000 in the aligned diff, and record findings.

- [x] G1: Base commit verified 4d4f1db on branch t570-rust-40k-mismatch with PARange/DFR0/DCZID fixes present.
  CHECK: git rev-parse --short HEAD
  EXPECT: 4d4f1db
  EVIDENCE: Verified HEAD is 4d4f1db on branch t570-rust-40k-mismatch; PARange=2, DFR0=0x10305106, DCZID=0x04 all confirmed in source.

- [x] G2: Clean build in separate CARGO_TARGET_DIR target-t570.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track570-rust-40k/target-t570 cargo build --release -p u12-orchestrator --bin run_kernel
  EXPECT: Finished `release` profile
  EVIDENCE: Release binary built at `/Users/Shared/wt-track570-rust-40k/target-t570/release/run_kernel` (SHA-256 `91f198059770b4cfc755633ef2ab969b6a251256b1aabbb5c3842d6b0cdf712c`).

- [x] G3: Locate exact divergence between Rust step 29,994 and 39,994 (QEMU step 30,000 and 40,000).
  EVIDENCE: Located at Rust step 33,320 / QEMU step 33,326 at PC `0xffffff8009456ab0` (`0xb94006e8`: `LDR W8, [X23, #4]`) followed by `REV W8, W8` at step 33,321 / 33,327. Linux `setup_machine_fdt()` reads DTB header `totalsize` from PA `0x48200004` (X23=0xffffffbefe800000). QEMU's `arm_load_dtb()` resizes and enriches the DTB to 22,930 (`0x5992`) bytes (`/chosen`, `/memory@40000000`, `/psci`), whereas Rust previously loaded the un-enriched 1,465-byte (`0x5b9`) raw file. At step 33,321 (QEMU 33,327), X8 became `0x5992` in QEMU vs `0x5b9` in Rust.

- [x] G4: Fix divergence in Rust emulator with QEMU-matched regression vector per touched class.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track570-rust-40k/target-t570 cargo test --release --test sysreg_p3 -- --test-threads=1
  EXPECT: test result: ok. 10 passed; 0 failed
  EVIDENCE: Extracted QEMU Cortex-A53 virt runtime DTB (22,930 bytes, SHA-256 `c94b1fd1ea4915ac0f3bd4fc69f5259bc695f8aad6ca49124760ab6512021a91`) to `guest-image/qemu-virt-runtime.dtb`. Updated `run_kernel.rs` to align minimal DTBs to QEMU runtime DTB. Added QEMU-matched regression test `p3_fdt_totalsize_load_rev_matches_cortex_a53_qemu` in `units/u12-orchestrator/tests/sysreg_p3.rs`. Test suite passes 10/10.

- [x] G5: Aligned diff rerun confirms no divergence <= 40,000 and no new earlier divergences introduced.
  EVIDENCE: Verified window of 10,015 steps (Rust steps 29,994..40,009 / QEMU steps 30,000..40,015): 100% bit-for-bit match on all GPRs (x0-x30), sp, pc, cpsr, sctlr, ttbr0, tcr, ttbr1, mair. Checkpoints at steps 10k, 20k, 30k, 40k match 100%. Prior divergence at step 40,000 is completely eliminated. Next periodic divergence sampled at Rust step 49,994 / QEMU step 50,000. Earlier steps 1..10,000 re-verified against `track560-qemu-firstdiv-10k.log`: zero new earlier divergences.

- [x] G6: Update TIMELINE.md, commit fix on t570-rust-40k-mismatch branch.
  EVIDENCE: Appended entry to `TIMELINE.md`. Committed changes to branch `t570-rust-40k-mismatch`.

- [x] G7: Findings file /Users/Shared/agyx-track570-rust-40k-mismatch.md delivered on disk.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track570-rust-40k-mismatch.md
  EXPECT: Verdict: PASS
  EVIDENCE: Findings file written with complete CLAIM, SCOPE, and WITNESS sections.
