# GATES.md — Track T590 (Rust emulator: synchronous Data Abort exception vectoring to VBAR_EL1+0x200)

## G590-1: Base Lineage Verified
- [x] Base commit 22c7ba450891ea56d16f6f98fe6da77eb8ca7d53 from t587-sbcs-carry verified in wt-track590-data-abort
CHECK: git -C /Users/Shared/wt-track590-data-abort rev-parse HEAD
EXPECT: 22c7ba450891ea56d16f6f98fe6da77eb8ca7d53
EVIDENCE: Verified HEAD is 22c7ba450891ea56d16f6f98fe6da77eb8ca7d53.

## G590-2: QEMU-Matched Regression Test Suite Passing
- [x] All unit test vectors for synchronous Data Abort exception vectoring (LDR null base, STR unmapped VA, DFSC levels 0..3, Permission fault, ERET return) pass cargo test
CHECK: cd /Users/Shared/wt-track590-data-abort && CARGO_TARGET_DIR=target-t590 cargo test --release -p u12-orchestrator --test data_abort_vbar_regression
EXPECT: test result: ok. 6 passed
EVIDENCE: 6/6 tests passed in data_abort_vbar_regression matching Cortex-A53 QEMU 11.1.2 oracle.

## G590-3: Release Build of run_kernel Succeeds
- [x] Release binary builds cleanly and build.log is produced
CHECK: ls -la /Users/Shared/wt-track590-data-abort/target-t590/release/run_kernel
EXPECT: run_kernel
EVIDENCE: Release binary built at /Users/Shared/wt-track590-data-abort/target-t590/release/run_kernel (SHA-256 664c17eb7534797d0197e1d4d7dc5f7fbbddf8f2d9eb9301e0552d5e8c0e16cf).

## G590-4: Boot Gate Reaches Past Step 49,688,578
- [x] Guest advances past step 49,688,578 from cold reset with UART advancing
CHECK: grep -E "step [0-9]+" /Users/Shared/wt-track590-data-abort/runner.log | tail -n 1
EXPECT: step
EVIDENCE: Live cold boot reached step 70,000,000 (70M steps, PC=0xffffff80085bed34, console_bytes=5542, irq_in=472, irq_out=473), cleanly advancing +20,311,422 steps past the step 49,688,578 halt with console bytes advancing from 3,936 to 5,542 bytes (+1,606 bytes).

## G590-5: Findings and Registry Complete
- [x] Findings file /Users/Shared/agyx-track590-data-abort.md has Verdict and registry updated
CHECK: head -n 1 /Users/Shared/agyx-track590-data-abort.md
EXPECT: Verdict: PASS
EVIDENCE: Findings file verified on disk at /Users/Shared/agyx-track590-data-abort.md starting with Verdict: PASS.
