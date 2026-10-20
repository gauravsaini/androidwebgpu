# GATES.md — Track T587 (Rust engine: NZCV carry propagation for ADC/SBC/ADCS/SBCS in u2-ir-lift)

## G587-1: Base Lineage Verified
- [x] Base commit 8b716040b25f91350d65e83513f4f58f60da9fd9 from t586-vbar-irq verified in wt-track587-sbcs-carry
CHECK: git -C /Users/Shared/wt-track587-sbcs-carry rev-parse HEAD
EXPECT: 8b716040b25f91350d65e83513f4f58f60da9fd9
EVIDENCE: Verified HEAD is 8b716040b25f91350d65e83513f4f58f60da9fd9.

## G587-2: QEMU-Matched Regression Test Suite Passing
- [x] All unit test vectors for ADC/SBC/ADCS/SBCS (carry-in 0/1, Rd=xzr discard vs write, csinv/cset/cbz idiom) pass cargo test
CHECK: cd /Users/Shared/wt-track587-sbcs-carry && cargo test -p u12-orchestrator --test adc_sbc_regression
EXPECT: test result: ok. 16 passed
EVIDENCE: 16/16 tests passed in adc_sbc_regression matching native QEMU 11.1.2 oracle.

## G587-3: Release Build of run_kernel Succeeds
- [x] Release binary builds cleanly and build.log is produced
CHECK: ls -la /Users/Shared/wt-track587-sbcs-carry/target-t587/release/run_kernel
EXPECT: run_kernel
EVIDENCE: Release binary built at /Users/Shared/wt-track587-sbcs-carry/target-t587/release/run_kernel (SHA-256 8cc695a0d04474cbd6c5c2c462784ef4ecdf334e753902cb75f9376260a524e4). Log in build.log.

## G587-4: Boot Gate Reaches Past Step 49,688,549
- [x] Guest advances past step 49,688,549 from cold reset with UART advancing
CHECK: grep -E "Executed [0-9]+ steps" /Users/Shared/wt-track587-sbcs-carry/runner.log
EXPECT: Executed
EVIDENCE: [done] Executed 49688578 steps. PC=0xffffff80082591e4, irq_in=74, irq_out=74 (advanced +29 steps past bypassed wall).

## G587-5: Findings and Registry Complete
- [x] Findings file /Users/Shared/agyx-track587-sbcs-carry.md has Verdict and registry updated
CHECK: head -n 1 /Users/Shared/agyx-track587-sbcs-carry.md
EXPECT: Verdict:
EVIDENCE: Findings file verified on disk at /Users/Shared/agyx-track587-sbcs-carry.md starting with Verdict: FAIL (Next Wall Identified).
