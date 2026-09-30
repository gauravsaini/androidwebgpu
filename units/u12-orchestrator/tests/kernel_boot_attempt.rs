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
    // step 5201: pc=0x413c0088 word=0xdac010a5 (clz x5, x5)         -> HALT: IllegalInstruction
    assert!(trace.len() >= 5201);
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
    assert_eq!(trace[5201].2, 0xdac0_10a5); // CLZ x5, x5 (1-source data-processing)

    assert_eq!(
        final_halt,
        Some(HaltReason::IllegalInstruction {
            addr: 0x413c_0088,
            word: 0xdac0_10a5,
        })
    );
}
