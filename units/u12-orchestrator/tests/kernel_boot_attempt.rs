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
    let max_steps = 8192;
    let mut final_halt = None;

    for step in 0..max_steps {
        let pc = orch.machine().cpu[0].pc;
        let word = u32::from_le_bytes(
            orch.machine().ram[(pc - RAM_BASE) as usize..(pc - RAM_BASE + 4) as usize]
                .try_into()
                .unwrap(),
        );

        trace.push((step, pc, word));

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
    // step 7467: pc=0x40c03678 word=0xd518a205 (msr mair_el1, x5)        -> HALT: Unsupported (system semantics not lifted)
    assert!(trace.len() >= 7467);
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
    assert_eq!(trace[7467].2, 0xd518_a205); // MSR MAIR_EL1, X5 (S3_0_C10_C2_0) -> HALT

    // U1 rejects MSR MAIR_EL1 with the honest system-semantics trap.
    // Pin the trapping address; the reason string is not pinned.
    match final_halt {
        Some(HaltReason::Unsupported { addr, .. }) => assert_eq!(addr, 0x40c0_3678),
        other => panic!("expected Unsupported at 0x40c03678, got {other:?}"),
    }
}
