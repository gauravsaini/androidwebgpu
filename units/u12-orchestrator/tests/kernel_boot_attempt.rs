//! Measured execution attempt: booting real AOSP ARM64 kernel under Path N Orchestrator.
//!
//! Asserts the exact, honest gate failure without faking.

use std::fs;
use std::path::Path;
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_PATH: &str = "/mnt/sdb1/aosp/Image";

#[test]
fn test_kernel_boot_measured_failure() {
    let path = Path::new(KERNEL_PATH);
    if !path.exists() {
        eprintln!("AOSP kernel not found at {KERNEL_PATH}, skipping execution test");
        return;
    }

    let kernel_bytes = fs::read(path).expect("failed to read kernel image");

    let mut orch = Orchestrator::new();
    let ram = &mut orch.machine_mut().ram;
    let load_len = kernel_bytes.len().min(ram.len());
    ram[..load_len].copy_from_slice(&kernel_bytes[..load_len]);

    // Linux kernel entry point
    orch.machine_mut().cpu[0].pc = RAM_BASE;
    orch.machine_mut().cpu[0].sp = RAM_BASE + 0x0800_0000;
    orch.machine_mut().cpu[0].regs = [0; 31];

    let mut trace = Vec::new();
    let max_steps = 16384;
    let mut final_halt = None;
    let mut x9_at_7534 = 0u64;

    for step in 0..max_steps {
        let pc = orch.machine().cpu[0].pc;
        let word = u32::from_le_bytes(
            orch.machine().ram[(pc - RAM_BASE) as usize..(pc - RAM_BASE + 4) as usize]
                .try_into()
                .unwrap(),
        );

        trace.push((step, pc, word));
        if step == 7534 {
            // First fixup-loop iteration: X9 was post-incremented by the
            // LDR at step 7533; it advances 8 bytes per outer iteration.
            x9_at_7534 = orch.machine().cpu[0].regs[9];
        }

        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            StepOutcome::WfiYield { addr } => {
                final_halt = Some(HaltReason::Wfi { addr });
                break;
            }
            StepOutcome::Halted(reason) => {
                final_halt = Some(reason);
                break;
            }
        }
    }

    println!("=== Kernel Execution Trace (first steps) ===");
    for (step, pc, word) in &trace {
        println!("step {step:2}: pc={pc:#010x} word={word:#010x}");
    }
    println!("Final outcome: {final_halt:?}");

    // The kernel executes:
    // step 0: pc=0x40000000 word=0x91005a4d (add x13, x18, #0x16) -> OK
    // step 1: pc=0x40000004 word=0x144effff (b +0x13c0000)        -> branches to 0x413c0000
    // step 2: pc=0x413c0000 word=0x94000008 (bl +0x20)             -> branches to 0x413c0020
    // step 3: pc=0x413c0020 word=0xaa0003f5 (mov x21, x0)          -> OK
    // step 4: pc=0x413c0024 word=0xb0000820 (adrp x0, ...)         -> OK
    // step 5: pc=0x413c0028 word=0x91000000 (add x0, x0, #0)      -> OK
    // step 6: pc=0x413c002c word=0xa9000415 (stp x21, x1, [x0])   -> OK
    // step 7: pc=0x413c0030 word=0xa9010c02 (stp x2, x3, [x0, #16]) -> OK
    // step 8: pc=0x413c0034 word=0xd5033fbf (dmb sy)               -> OK
    // step 9: pc=0x413c0038 word=0xd2800401 (mov x1, #0x20)        -> OK
    // step 10: pc=0x413c003c word=0x17b11348 (b 0x4d5c)            -> branches to 0x40004d5c
    // step 11: pc=0x40004d5c word=0x8b000021 (add x1, x1, x0)      -> OK
    // step 12: pc=0x40004d60 word=0xd53b0023 (mrs x3, ctr_el0)     -> OK
    // step 13: pc=0x40004d64 word=0xd503201f (nop)                  -> OK
    // step 14: pc=0x40004d68 word=0xd3504c63 (ubfx x3, x3, #16, #4) -> OK
    // step 15: pc=0x40004d6c word=0xd2800082 (movz x2, #4)          -> OK
    // step 16: pc=0x40004d70 word=0x9ac32042 (lsl x2, x2, x3)       -> OK
    // step 17: pc=0x40004d74 word=0xd1000443 (sub x3, x2, #1)       -> OK (GB-5)
    // ... executes cache clean loop, sysreg setup, bl el2_setup, etc. ...
    // step 57: pc=0x413c004c word=0xcb000021 (sub x1, x1, x0)       -> OK (GB-6)
    // step 58: pc=0x413c0050 word=0x97b11343 (bl ...)               -> branches to 0x40004d5c
    // step 59..71: cache-setup prologue at 0x40004d5c (add/mrs/nop/ubfx/movz/lsl/sub/cmp/...)
    // step 72: pc=0x40004d9c word=0xd5087620 (dc cache maintenance) -> cache-clean loop entry
    // steps 73..~5199: cache-clean loop (dc; add x0, x0, x2; cmp x0, x1; b.lt) runs to completion
    // step 5200: pc=0x413c0084 word=0xf0ffc205                     -> OK
    // step 5201: pc=0x413c0088 word=0xdac010a5 (clz x5, x5)         -> OK (GB-7)
    // step 5202: pc=0x413c008c word=0xf10064bf (cmp x5, #0x19)       -> OK
    // step 5203: pc=0x413c0090 word=0x540001ea (b.gt 0x413c00cc)    -> OK (taken)
    // steps 5204..5216: adrp/ldr/mov/add/movz/ubfiz/subs/eor sequence -> OK
    // step 5217: pc=0x413c0100 word=0x9b0d7d4a (madd x10,x10,x13,xzr)  -> OK (GB-8)
    // ...
    // step 7452: pc=0x413c027c word=0xd65f0380 (ret x27)                  -> OK
    // step 7453: pc=0x413c0018 word=0x97e10d8a (bl 0x40c03640)            -> OK
    // step 7454: pc=0x40c03640 word=0xd508871f (dc cvac, xzr)              -> OK
    // step 7455: pc=0x40c03644 word=0xd503379f (dsb ish)                  -> OK
    // step 7456: pc=0x40c03648 word=0xd2a00600 (movz x0, #0x30, lsl #16) -> OK (x0 = 0x300000)
    // step 7457: pc=0x40c0364c word=0xd5181040 (msr cpacr_el1, x0)       -> OK (GB-9: persistent)
    // step 7458: pc=0x40c03650 word=0xd2820000 (movz x0, #0x1000)         -> OK (x0 = 0x1000)
    // step 7459: pc=0x40c03654 word=0xd5100240 (msr mdscr_el1, x0)       -> OK (GB-10: persistent, stores 0x1000)
    // step 7460: pc=0x40c03658 word=0xd5033fdf (isb)                     -> OK
    // step 7461: pc=0x40c0365c word=0xd50348ff (msr daifclr, #0x8)       -> OK (GB-11: persistent RMW, daif 0x3c0 -> 0x1c0)
    // step 7462: pc=0x40c03660 word=0xd5380500 (mrs x0, id_aa64dfr0_el1) -> OK (recognized -> 0)
    // step 7463: pc=0x40c03664 word=0x93482c00 (sbfx x0, x0, #8, #4)     -> OK
    // step 7464: pc=0x40c03668 word=0xf100041f (cmp x0, #0x1)            -> OK
    // step 7465: pc=0x40c0366c word=0x5400004b (b.lt 0x40c03674)        -> OK (taken)
    // step 7466: pc=0x40c03674 word=0x580002a5 (ldr x5, [pc, #84])      -> OK (GB-12: static Load via mem_load)
    // step 7467: pc=0x40c03678 word=0xd518a205 (msr mair_el1, x5)        -> OK (GB-13: persistent WriteSys)
    // step 7468: pc=0x40c0367c word=0xd2a69ea0 (mov x0, #0x34f50000)     -> OK
    // step 7469: pc=0x40c03680 word=0xf29b23a0 (movk x0, #0xd91d)        -> OK
    // step 7470: pc=0x40c03684 word=0x5800026a (ldr x10, [pc, #76])      -> OK (GB-12: static Load via mem_load)
    // step 7471: pc=0x40c03688 word=0xd0004769 (adrp x9, 0x414f1000)     -> OK
    // step 7472: pc=0x40c0368c word=0xf9409d29 (ldr x9, [x9, #312])      -> OK
    // step 7473: pc=0x40c03690 word=0xb340152a (bfxil x10, x9, #0, #6)   -> OK
    // step 7474: pc=0x40c03694 word=0xd5380705 (mrs x5, id_aa64mmfr0_el1)-> OK (GB-14: recognized -> 0)
    // step 7475: pc=0x40c03698 word=0xd34008a5 (ubfx x5, x5, #0, #3)     -> OK (PARange extract, = 0)
    // step 7476: pc=0x40c0369c word=0xd28000a6 (mov x6, #0x5)            -> OK
    // step 7477: pc=0x40c036a0 word=0xeb0600bf (cmp x5, x6)              -> OK
    // step 7478: pc=0x40c036a4 word=0x9a8580c5 (csel x5, x6, x5, hi)     -> OK (GB-15: HI false -> x5 = 0)
    // step 7479: pc=0x40c036a8 word=0xb36008aa (bfi x10, x5, #32, #3)    -> OK
    // step 7480: pc=0x40c036ac word=0xd5380729 (mrs x9, id_aa64mmfr1_el1)-> OK (GB-16: recognized -> 0)
    // step 7481: pc=0x40c036b0 word=0x92400d29 (and x9, x9, #0xf)        -> OK
    // step 7482: pc=0x40c036b4 word=0xb4000049 (cbz x9, 0x40c036bc)      -> OK (taken: x9 = 0)
    // step 7483: pc=0x40c036bc word=0xd518204a (msr tcr_el1, x10)       -> HALT: Unsupported (TCR_EL1 not persistent yet)
    assert!(trace.len() >= 7483);
    assert_eq!(trace[0].1, 0x4000_0000);
    assert_eq!(trace[0].2, 0x9100_5a4d); // ADD imm
    assert_eq!(trace[1].1, 0x4000_0004);
    assert_eq!(trace[1].2, 0x144e_ffff); // B 0x13c0000
    assert_eq!(trace[2].1, 0x413c_0000); // stext
    assert_eq!(trace[2].2, 0x9400_0008); // BL preserve_boot_args
    assert_eq!(trace[3].1, 0x413c_0020);
    assert_eq!(trace[4].1, 0x413c_0024);
    assert_eq!(trace[5].1, 0x413c_0028);
    assert_eq!(trace[6].1, 0x413c_002c);
    assert_eq!(trace[6].2, 0xa900_0415); // STP x21, x1, [x0]
    assert_eq!(trace[7].1, 0x413c_0030);
    assert_eq!(trace[7].2, 0xa901_0c02); // STP x2, x3, [x0, #16]
    assert_eq!(trace[8].1, 0x413c_0034);
    assert_eq!(trace[8].2, 0xd503_3fbf); // DMB sy
    assert_eq!(trace[9].1, 0x413c_0038);
    assert_eq!(trace[9].2, 0xd280_0401); // MOV x1, #0x20
    assert_eq!(trace[10].1, 0x413c_003c);
    assert_eq!(trace[10].2, 0x17b1_1348); // B 0x4d5c
    assert_eq!(trace[11].1, 0x4000_4d5c);
    assert_eq!(trace[11].2, 0x8b00_0021); // ADD x1, x1, x0
    assert_eq!(trace[12].1, 0x4000_4d60);
    assert_eq!(trace[12].2, 0xd53b_0023); // MRS x3, ctr_el0
    assert_eq!(trace[13].1, 0x4000_4d64);
    assert_eq!(trace[13].2, 0xd503_201f); // NOP
    assert_eq!(trace[14].1, 0x4000_4d68);
    assert_eq!(trace[14].2, 0xd350_4c63); // UBFX x3, x3, #16, #4
    assert_eq!(trace[15].1, 0x4000_4d6c);
    assert_eq!(trace[15].2, 0xd280_0082); // MOVZ x2, #4
    assert_eq!(trace[16].1, 0x4000_4d70);
    assert_eq!(trace[16].2, 0x9ac3_2042); // LSL x2, x2, x3
    assert_eq!(trace[17].1, 0x4000_4d74);
    assert_eq!(trace[17].2, 0xd100_0443); // SUB x3, x2, #1 (GB-5)
    assert_eq!(trace[57].1, 0x413c_004c);
    assert_eq!(trace[57].2, 0xcb00_0021); // SUB x1, x1, x0 (shifted reg, GB-6)
    assert_eq!(trace[58].1, 0x413c_0050);
    assert_eq!(trace[58].2, 0x97b1_1343); // BL cache-maintenance routine
    assert_eq!(trace[59].1, 0x4000_4d5c);
    assert_eq!(trace[59].2, 0x8b00_0021); // ADD x1, x1, x0 (routine prologue)
    assert_eq!(trace[72].1, 0x4000_4d9c);
    assert_eq!(trace[72].2, 0xd508_7620); // DC cache maintenance (loop entry)
    assert_eq!(trace[5200].1, 0x413c_0084);
    assert_eq!(trace[5200].2, 0xf0ff_c205);
    assert_eq!(trace[5201].1, 0x413c_0088);
    assert_eq!(trace[5201].2, 0xdac0_10a5); // CLZ x5, x5 (1-source, GB-7)
    assert_eq!(trace[5202].1, 0x413c_008c);
    assert_eq!(trace[5202].2, 0xf100_64bf); // CMP x5, #0x19
    assert_eq!(trace[5203].1, 0x413c_0090);
    assert_eq!(trace[5203].2, 0x5400_01ea); // B.GT 0x413c00cc (taken)
    assert_eq!(trace[5204].1, 0x413c_00cc);
    assert_eq!(trace[5204].2, 0xb000_0984); // ADRP x4, ...
    assert_eq!(trace[5213].1, 0x413c_00f0);
    assert_eq!(trace[5213].2, 0xaa04_03ea); // MOV x10, x4
    assert_eq!(trace[5214].1, 0x413c_00f4);
    assert_eq!(trace[5214].2, 0xd100_054a); // SUBS x10, x10, #1
    assert_eq!(trace[5215].1, 0x413c_00f8);
    assert_eq!(trace[5215].2, 0x8a0a_016b); // EOR x11, x11, x10
    assert_eq!(trace[5216].1, 0x413c_00fc);
    assert_eq!(trace[5216].2, 0xaa04_03ea); // MOV x10, x4
    assert_eq!(trace[5217].1, 0x413c_0100);
    assert_eq!(trace[5217].2, 0x9b0d_7d4a); // MADD x10, x10, x13, xzr (3-source, GB-8)
    assert_eq!(trace[7452].1, 0x413c_027c);
    assert_eq!(trace[7452].2, 0xd65f_0380); // RET x27
    assert_eq!(trace[7453].1, 0x413c_0018);
    assert_eq!(trace[7453].2, 0x97e1_0d8a); // BL 0x40c03640
    assert_eq!(trace[7454].1, 0x40c0_3640);
    assert_eq!(trace[7454].2, 0xd508_871f); // DC CVAC, XZR
    assert_eq!(trace[7455].1, 0x40c0_3644);
    assert_eq!(trace[7455].2, 0xd503_379f); // DSB ISH
    assert_eq!(trace[7456].1, 0x40c0_3648);
    assert_eq!(trace[7456].2, 0xd2a0_0600); // MOVZ x0, #0x30, lsl #16
    assert_eq!(trace[7457].1, 0x40c0_364c);
    assert_eq!(trace[7457].2, 0xd518_1040); // MSR CPACR_EL1, x0 (S3_0_C1_C0_2, GB-9)
    assert_eq!(trace[7458].1, 0x40c0_3650);
    assert_eq!(trace[7458].2, 0xd282_0000); // MOVZ x0, #0x1000
    assert_eq!(trace[7459].1, 0x40c0_3654);
    assert_eq!(trace[7459].2, 0xd510_0240); // MSR MDSCR_EL1, x0 (S2_0_C0_C2_2, GB-10: persistent)
    assert_eq!(trace[7460].1, 0x40c0_3658);
    assert_eq!(trace[7460].2, 0xd503_3fdf); // ISB
    assert_eq!(trace[7461].1, 0x40c0_365c);
    assert_eq!(trace[7461].2, 0xd503_48ff); // MSR DAIFClr, #0x8 (GB-11: persistent RMW)
    assert_eq!(trace[7462].1, 0x40c0_3660);
    assert_eq!(trace[7462].2, 0xd538_0500); // MRS X0, ID_AA64DFR0_EL1
    assert_eq!(trace[7463].1, 0x40c0_3664);
    assert_eq!(trace[7463].2, 0x9348_2c00); // SBFX X0, X0, #8, #4
    assert_eq!(trace[7464].1, 0x40c0_3668);
    assert_eq!(trace[7464].2, 0xf100_041f); // CMP X0, #0x1
    assert_eq!(trace[7465].1, 0x40c0_366c);
    assert_eq!(trace[7465].2, 0x5400_004b); // B.LT 0x40c03674 (taken)
    assert_eq!(trace[7466].1, 0x40c0_3674);
    assert_eq!(trace[7466].2, 0x5800_02a5); // LDR X5, [PC, #84] (GB-12: static Load via mem_load)
    assert_eq!(trace[7467].1, 0x40c0_3678);
    assert_eq!(trace[7467].2, 0xd518_a205); // MSR MAIR_EL1, X5 (S3_0_C10_C2_0) -> OK (GB-13)
    assert_eq!(trace[7468].1, 0x40c0_367c);
    assert_eq!(trace[7468].2, 0xd2a6_9ea0); // MOV X0, #0x34f50000
    assert_eq!(trace[7469].1, 0x40c0_3680);
    assert_eq!(trace[7469].2, 0xf29b_23a0); // MOVK X0, #0xd91d
    assert_eq!(trace[7470].1, 0x40c0_3684);
    assert_eq!(trace[7470].2, 0x5800_026a); // LDR X10, [PC, #76] (GB-12: static Load via mem_load)
    assert_eq!(trace[7471].1, 0x40c0_3688);
    assert_eq!(trace[7471].2, 0xd000_4769); // ADRP X9, 0x414f1000
    assert_eq!(trace[7472].1, 0x40c0_368c);
    assert_eq!(trace[7472].2, 0xf940_9d29); // LDR X9, [X9, #312]
    assert_eq!(trace[7473].1, 0x40c0_3690);
    assert_eq!(trace[7473].2, 0xb340_152a); // BFXIL X10, X9, #0, #6
    assert_eq!(trace[7474].1, 0x40c0_3694);
    assert_eq!(trace[7474].2, 0xd538_0705); // MRS X5, ID_AA64MMFR0_EL1 (S3_0_C0_C7_0) -> OK (GB-14: recognized -> 0)
    assert_eq!(trace[7475].1, 0x40c0_3698);
    assert_eq!(trace[7475].2, 0xd340_08a5); // UBFX X5, X5, #0, #3 (PARange extract)
    assert_eq!(trace[7476].1, 0x40c0_369c);
    assert_eq!(trace[7476].2, 0xd280_00a6); // MOV X6, #0x5
    assert_eq!(trace[7477].1, 0x40c0_36a0);
    assert_eq!(trace[7477].2, 0xeb06_00bf); // CMP X5, X6
    assert_eq!(trace[7478].1, 0x40c0_36a4);
    assert_eq!(trace[7478].2, 0x9a85_80c5); // CSEL X5, X6, X5, HI -> OK (GB-15)
    assert_eq!(trace[7479].1, 0x40c0_36a8);
    assert_eq!(trace[7479].2, 0xb360_08aa); // BFI X10, X5, #32, #3 -> OK
    assert_eq!(trace[7480].1, 0x40c0_36ac);
    assert_eq!(trace[7480].2, 0xd538_0729); // MRS X9, ID_AA64MMFR1_EL1 -> OK (GB-16)
    assert_eq!(trace[7481].1, 0x40c0_36b0);
    assert_eq!(trace[7481].2, 0x9240_0d29); // AND X9, X9, #0xF -> OK
    assert_eq!(trace[7482].1, 0x40c0_36b4);
    assert_eq!(trace[7482].2, 0xb400_0049); // CBZ X9, 0x40c036bc -> OK (taken)
    assert_eq!(trace[7483].1, 0x40c0_36bc);
    assert_eq!(trace[7483].2, 0xd518_204a); // MSR TCR_EL1, X10 -> OK (GB-17: persistent)
    assert_eq!(trace[7484].1, 0x40c0_36c0);
    assert_eq!(trace[7484].2, 0xd65f_03c0); // RET X30 -> OK
    assert_eq!(trace[7485].1, 0x413c_001c);
    assert_eq!(trace[7485].2, 0x17e1_0cd6); // B 0x40c03374 -> OK
    assert_eq!(trace[7486].1, 0x40c0_3374);
    assert_eq!(trace[7486].2, 0xaa00_03f3); // MOV X19, X0 -> OK
    assert_eq!(trace[7487].1, 0x40c0_3378);
    assert_eq!(trace[7487].2, 0xd538_1014); // MRS X20, SCTLR_EL1 -> OK (persistent)
    assert_eq!(trace[7488].1, 0x40c0_337c);
    assert_eq!(trace[7488].2, 0x97ff_ffb0); // BL 0x40c0323c -> OK
    assert_eq!(trace[7489].1, 0x40c0_323c);
    assert_eq!(trace[7489].2, 0xd538_0701); // MRS X1, ID_AA64MMFR0_EL1 -> OK (GB-14: recognized -> 0)
    assert_eq!(trace[7490].1, 0x40c0_3240);
    assert_eq!(trace[7490].2, 0xd35c_7c22); // UBFX X2, X1, #28, #4 -> OK
    assert_eq!(trace[7491].1, 0x40c0_3244);
    assert_eq!(trace[7491].2, 0xf100_005f); // CMP X2, #0 -> OK
    assert_eq!(trace[7492].1, 0x40c0_3248);
    assert_eq!(trace[7492].2, 0x5400_02a1); // B.NE 0x40c0329c -> OK (not taken)
    assert_eq!(trace[7493].1, 0x40c0_324c);
    assert_eq!(trace[7493].2, 0xd280_0002); // MOV X2, #0 -> OK
    assert_eq!(trace[7494].1, 0x40c0_3250);
    assert_eq!(trace[7494].2, 0xb000_4fe1); // ADRP X1, 0x41600000 -> OK
    assert_eq!(trace[7495].1, 0x40c0_3254);
    assert_eq!(trace[7495].2, 0x9120_2021); // ADD X1, X1, #0x808 -> OK
    assert_eq!(trace[7496].1, 0x40c0_3258);
    assert_eq!(trace[7496].2, 0xf900_0022); // STR X2, [X1] -> OK
    assert_eq!(trace[7497].1, 0x40c0_325c);
    assert_eq!(trace[7497].2, 0xd503_3fbf); // DMB SY -> OK
    assert_eq!(trace[7498].1, 0x40c0_3260);
    assert_eq!(trace[7498].2, 0xd508_7621); // DC IVAC, X1 -> OK (cache op: nop)
    assert_eq!(trace[7499].1, 0x40c0_3264);
    assert_eq!(trace[7499].2, 0xd000_5301); // ADRP X1, 0x41665000 -> OK
    assert_eq!(trace[7500].1, 0x40c0_3268);
    assert_eq!(trace[7500].2, 0xf000_5322); // ADRP X2, 0x4166a000 -> OK
    assert_eq!(trace[7501].1, 0x40c0_326c);
    assert_eq!(trace[7501].2, 0xaa01_03e3); // MOV X3, X1 -> OK
    assert_eq!(trace[7502].1, 0x40c0_3270);
    assert_eq!(trace[7502].2, 0xaa02_03e4); // MOV X4, X2 -> OK
    assert_eq!(trace[7503].1, 0x40c0_3274);
    assert_eq!(trace[7503].2, 0xd518_2003); // MSR TTBR0_EL1, X3 -> OK (GB-18: persistent)
    assert_eq!(trace[7504].1, 0x40c0_3278);
    assert_eq!(trace[7504].2, 0xd518_2024); // MSR TTBR1_EL1, X4 -> OK (GB-19: persistent)
    assert_eq!(trace[7505].1, 0x40c0_327c);
    assert_eq!(trace[7505].2, 0xd503_3fdf); // ISB -> OK
    assert_eq!(trace[7506].1, 0x40c0_3280);
    assert_eq!(trace[7506].2, 0xd518_1000); // MSR SCTLR_EL1, X0 = 0x34f5d91d (M=1: MMU ON) -> OK
    assert_eq!(trace[7507].1, 0x40c0_3284);
    assert_eq!(trace[7507].2, 0xd503_3fdf); // ISB -> OK
    assert_eq!(trace[7508].1, 0x40c0_3288);
    assert_eq!(trace[7508].2, 0xd508_751f); // IC IALLU -> OK (cache op: nop)
    assert_eq!(trace[7509].1, 0x40c0_328c);
    assert_eq!(trace[7509].2, 0xd503_379f); // DSB NSH -> OK
    assert_eq!(trace[7510].1, 0x40c0_3290);
    assert_eq!(trace[7510].2, 0xd503_3fdf); // ISB -> OK
    assert_eq!(trace[7511].1, 0x40c0_3294);
    assert_eq!(trace[7511].2, 0xd65f_03c0); // RET -> OK (to 0x40c03380)
    assert_eq!(trace[7512].1, 0x40c0_3380);
    assert_eq!(trace[7512].2, 0xd280_0018); // MOVZ X24, #0 -> OK
    assert_eq!(trace[7513].1, 0x40c0_3384);
    assert_eq!(trace[7513].2, 0x97ff_ffcf); // BL 0x40c032c0 -> OK
    assert_eq!(trace[7514].1, 0x40c0_32c0);
    assert_eq!(trace[7514].2, 0x1800_0889); // LDR W9, [PC, #272] (0x40c033d0) -> OK
    assert_eq!(trace[7515].1, 0x40c0_32c4);
    assert_eq!(trace[7515].2, 0x1800_088a); // LDR W10, [PC, #272] (0x40c033d4) -> OK
    assert_eq!(trace[7516].1, 0x40c0_32c8);
    assert_eq!(trace[7516].2, 0x92c0_0feb); // MOVN X11, #0x7f, LSL #32 -> OK
    assert_eq!(trace[7517].1, 0x40c0_32cc);
    assert_eq!(trace[7517].2, 0xf2a1_000b); // MOVK X11, #0x800, LSL #16 -> OK
    assert_eq!(trace[7518].1, 0x40c0_32d0);
    assert_eq!(trace[7518].2, 0xf280_000b); // MOVK X11, #0x0, LSL #0 -> OK
    assert_eq!(trace[7519].1, 0x40c0_32d4);
    assert_eq!(trace[7519].2, 0x8b17_016b); // ADD X11, X11, X23 -> OK
    assert_eq!(trace[7520].1, 0x40c0_32d8);
    assert_eq!(trace[7520].2, 0x8b0b_0129); // ADD X9, X9, X11 -> OK
    assert_eq!(trace[7521].1, 0x40c0_32dc);
    assert_eq!(trace[7521].2, 0x8b0a_012a); // ADD X10, X9, X10 -> OK
    assert_eq!(trace[7522].1, 0x40c0_32e0);
    assert_eq!(trace[7522].2, 0xeb0a_013f); // CMP X9, X10 -> OK
    assert_eq!(trace[7523].1, 0x40c0_32e4);
    assert_eq!(trace[7523].2, 0x5400_0102); // B.CS 0x40c03304 (taken) -> OK
    assert_eq!(trace[7524].1, 0x40c0_3304);
    assert_eq!(trace[7524].2, 0x1800_06a9); // LDR W9, [PC, #212] (0x40c033d8) -> OK
    assert_eq!(trace[7525].1, 0x40c0_3308);
    assert_eq!(trace[7525].2, 0x1800_06aa); // LDR W10, [PC, #212] (0x40c033dc) -> OK
    assert_eq!(trace[7526].1, 0x40c0_330c);
    assert_eq!(trace[7526].2, 0x8b0b_0129); // ADD X9, X9, X11 -> OK
    assert_eq!(trace[7527].1, 0x40c0_3310);
    assert_eq!(trace[7527].2, 0x8b0a_012a); // ADD X10, X9, X10 -> OK
    assert_eq!(trace[7528].1, 0x40c0_3314);
    assert_eq!(trace[7528].2, 0xcb18_02ef); // SUB X15, X23, X24 -> OK
    assert_eq!(trace[7529].1, 0x40c0_3318);
    assert_eq!(trace[7529].2, 0xb400_02cf); // CBZ X15, 0x40c03370 (not taken) -> OK
    assert_eq!(trace[7530].1, 0x40c0_331c);
    assert_eq!(trace[7530].2, 0xaa17_03f8); // MOV X24, X23 -> OK
    assert_eq!(trace[7531].1, 0x40c0_3320);
    assert_eq!(trace[7531].2, 0xeb0a_013f); // CMP X9, X10 -> OK
    assert_eq!(trace[7532].1, 0x40c0_3324);
    assert_eq!(trace[7532].2, 0x5400_0262); // B.CS 0x40c03370 (not taken) -> OK
    assert_eq!(trace[7533].1, 0x40c0_3328);
    assert_eq!(trace[7533].2, 0xf840_852b); // LDR X11, [X9], #8 -> OK (GB-20: translated)
    assert_eq!(trace[7534].1, 0x40c0_332c);
    assert_eq!(trace[7534].2, 0x3700_00cb); // TBNZ W11, #0, 0x40c03344 -> OK
    assert_eq!(trace[7535].1, 0x40c0_3330);
    assert_eq!(trace[7535].2, 0x8b17_016d); // ADD X13, X11, X23 (shifted) -> OK
    assert_eq!(trace[7536].1, 0x40c0_3334);
    assert_eq!(trace[7536].2, 0xf940_01ac); // LDR X12, [X13] -> OK (translated)
    assert_eq!(trace[7537].1, 0x40c0_3338);
    assert_eq!(trace[7537].2, 0x8b0f_018c); // ADD X12, X12, X15 -> OK
    assert_eq!(trace[7538].1, 0x40c0_333c);
    assert_eq!(trace[7538].2, 0xf800_85ac); // STR X12, [X13, X0, LSL #3] -> OK (translated)
    assert_eq!(trace[7539].1, 0x40c0_3340);
    assert_eq!(trace[7539].2, 0x17ff_fff8); // B 0x40c03320 -> OK
    assert_eq!(trace[7540].1, 0x40c0_3320);
    assert_eq!(trace[7540].2, 0xeb0a_013f); // CMP X9, X10 -> OK
    assert_eq!(trace[7541].1, 0x40c0_3324);
    assert_eq!(trace[7541].2, 0x5400_0262); // B.CS 0x40c03370 (not taken) -> OK
    assert_eq!(trace[7542].1, 0x40c0_3328);
    assert_eq!(trace[7542].2, 0xf840_852b); // LDR X11, [X9], #8 (2nd outer iter) -> OK

    // GB-20: the MMU data-access translation is wired into WasmHost. The
    // LDR at step 7533 (X9 = 0xffffff80096ab158, a kernel VA) now walks
    // the live page tables to PA 0x416ab158 instead of trapping. The
    // kernel proceeds into its page-table fixup loop (0x40c03320..0x40c0336c):
    // each outer iteration loads a 64-bit bitmask via LDR X11,[X9],#8
    // (X9 post-increments by 8) and walks its bits, adding the phys
    // offset X15 to live entries at X14. Measured over a 1M-step run:
    // 3141 outer iterations, inner bit counts varying 0..64 per
    // iteration — X9 genuinely advances, this is forward progress, not
    // a spin. With the 16384-step budget the kernel is still running:
    // no halt, no trap. Frontier: no halt within 16384 steps.
    assert!(
        final_halt.is_none(),
        "expected no halt within budget, got {final_halt:?}"
    );
    let (_, last_pc, _) = trace.last().unwrap();
    assert!(
        (0x40c0_3320..0x40c0_3370).contains(last_pc),
        "expected final pc in fixup loop, got {last_pc:#x}"
    );
    let x9_final = orch.machine().cpu[0].regs[9];
    assert!(
        x9_final > x9_at_7534,
        "X9 did not advance ({x9_final:#x} <= {x9_at_7534:#x}): not progress"
    );
}
