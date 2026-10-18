# Gates: Track T574 — rust-50k-mismatch

OWNS: units/u2-ir-lift/src/**, units/u1-decode/src/**, units/u12-orchestrator/src/**, units/u12-orchestrator/tests/**, GATES.md, TIMELINE.md, /Users/Shared/agyx-track574-rust-50k-mismatch.md

Scope: Root-cause and eliminate the engine-state divergence at Rust step 49,994 / QEMU step 50,000, add QEMU-matched regression vector per touched class, verify no divergence <= 50,000 in the aligned diff, and record findings in /Users/Shared/agyx-track574-rust-50k-mismatch.md.

- [x] G1: Base commit verified d86db15 on branch t574-rust-50k-mismatch.
  CHECK: git rev-parse --short HEAD
  EXPECT: d86db15
  EVIDENCE: Verified HEAD is d86db15ada98bc8dfd114ad112ef975095e3cb01 on branch t574-rust-50k-mismatch before running any work.

- [x] G2: Clean build in separate CARGO_TARGET_DIR target-t574.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track574-rust-50k/target-t574 cargo build --release -p u12-orchestrator --bin run_kernel
  EXPECT: Finished `release` profile
  EVIDENCE: Release binary built at `/Users/Shared/wt-track574-rust-50k-mismatch/target-t574/release/run_kernel`.

- [x] G3: Capture dense trace around 40,000..55,000 in Rust and QEMU, locate exact first instruction and architectural divergence.
  EVIDENCE: Located first divergence at QEMU step 41,105 / Rust step 41,099 at PC 0xffffff8009452624 (opcode 0xf2a01fea `MOVK X10, #0xff, LSL #16`). In preceding step (QEMU 41,104 / Rust 41,098) at PC 0xffffff8009452620, Linux setup_arch() executed `MRS X9, MPIDR_EL1` (0xd53800a9). QEMU Cortex-A53 returned X9 = 0x8000_0000 (RES1 bit 31=1, U bit 30=0), whereas Rust returned X9 = 0x4000_0000. Eight steps later at QEMU 41,113 / Rust 41,107 (PC 0xffffff8009452640), `MRS X2, MIDR_EL1` (0xd5380002) returned X2 = 0x410f_d034 in QEMU vs 0x0 in Rust, leading to diverging vsnprintf formatting in early CPU boot banner ("Booting Linux on physical CPU 0x%010lx [0x%08x]").

- [x] G4: Root-cause against Cortex-A53 / QEMU oracle, fix in Rust engine, and add QEMU-matched regression vector to tests.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track574-rust-50k/target-t574 cargo test --release --test sysreg_p3 -- --test-threads=1
  EXPECT: test result: ok. 11 passed; 0 failed
  EVIDENCE: Updated units/u2-ir-lift/src/lib.rs: MPIDR_EL1 (3,0,0,0,5) aligned to 0x8000_0000; MIDR_EL1 (3,0,0,0,0) aligned to 0x410f_d034. Added QEMU-matched regression test `p3_mpidr_midr_el1_matches_cortex_a53_qemu` in units/u12-orchestrator/tests/sysreg_p3.rs. 11/11 tests pass.

- [x] G5: Aligned diff rerun confirms divergence is eliminated (window past 50,000 clean) and no earlier divergences introduced.
  EVIDENCE: Re-ran dense window from QEMU step 39,996 (Rust 39,990) to QEMU step 55,000 (Rust 54,994) — all 15,005 consecutive instructions match 100% bit-for-bit across all 31 GPRs, SP, CPSR, SCTLR, TTBR0, TCR, TTBR1, MAIR, insn_pc, and opcode word. Verified up to step 100,000 dense (all 60,000 instructions match 100%). In addition, 100/100 periodic 10k checkpoints from step 10k to 1,000,000 match QEMU 100%. Steps 1..10,000 and 30k..40k re-verified with zero earlier divergences.

- [x] G6: TEN-PASSES verified, TIMELINE.md updated, conventional commit on branch t574-rust-50k-mismatch.
  EVIDENCE: Executed 10 consecutive fresh runs of test suite (`TEN-PASSES OK`). Updated TIMELINE.md. Changes committed to branch t574-rust-50k-mismatch.

- [x] G7: Findings file /Users/Shared/agyx-track574-rust-50k-mismatch.md delivered on disk.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track574-rust-50k-mismatch.md
  EXPECT: Verdict: PASS
  EVIDENCE: Certified findings file written to /Users/Shared/agyx-track574-rust-50k-mismatch.md with Verdict: PASS, complete CLAIM, SCOPE, and WITNESS sections.
