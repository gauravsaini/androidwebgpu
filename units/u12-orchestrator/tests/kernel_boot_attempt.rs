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
    let max_steps = 100;
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
    // step 14: pc=0x40004d68 word=0xd3504c63 (ubfx x3, x3, #16, #4) -> HALT: IllegalInstruction
    assert!(trace.len() >= 14);
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
    assert_eq!(trace[14].2, 0xd350_4c63); // UBFX

    assert_eq!(
        final_halt,
        Some(HaltReason::IllegalInstruction {
            addr: 0x4000_4d68,
            word: 0xd350_4c63,
        })
    );
}
