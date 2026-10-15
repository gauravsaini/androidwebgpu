//! Execute the exact CSINV word from the pinned guest kernel Image.
//!
//! The initial x1/x2/x8/x24/PSTATE values are the second live QEMU hit at the
//! target PC. QEMU and this probe therefore execute the same guest word with
//! the same instruction inputs; the surrounding bytes are read from Image.

use std::fs;
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const TEXT_OFFSET: usize = 0x80000;
const INSN_FILE_OFFSET: usize = 0x2d4018;
const INSN_WORD: u32 = 0xda81_8058;
const TARGET_PA: u64 = RAM_BASE + TEXT_OFFSET as u64 + INSN_FILE_OFFSET as u64;
const REGION_BASE: u64 = 0x4000_0000;
const QEMU_CPSR: u64 = 0xa000_01c5;

fn main() {
    let image_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Users/Shared/guest-inputs/Image".to_string());
    let image = fs::read(&image_path).expect("could not read pinned guest Image");
    let instruction_offset = INSN_FILE_OFFSET;
    let word = u32::from_le_bytes(
        image[instruction_offset..instruction_offset + 4]
            .try_into()
            .expect("CSINV word outside guest Image"),
    );
    assert_eq!(word, INSN_WORD, "guest Image bytes changed");

    let mut orchestrator = Orchestrator::new();
    orchestrator.machine_mut().ram[TEXT_OFFSET..TEXT_OFFSET + image.len()]
        .copy_from_slice(&image);
    let cpu = &mut orchestrator.machine_mut().cpu[0];
    cpu.pc = TARGET_PA;
    cpu.regs[1] = REGION_BASE;
    cpu.regs[2] = REGION_BASE;
    cpu.regs[8] = !REGION_BASE;
    cpu.regs[24] = 0xffff_ff80_0951_2000;
    cpu.pstate = QEMU_CPSR;

    println!("Image path: {image_path}");
    println!("Image file offset: {instruction_offset:#x}");
    println!("Guest PC alias (QEMU): {:#018x}", 0xffff_ff80_0800_0000 + INSN_FILE_OFFSET as u64 + TEXT_OFFSET as u64);
    println!("Rust physical PC: {TARGET_PA:#018x}");
    println!("Guest bytes around CSINV: {:02x?}", &image[instruction_offset - 16..instruction_offset + 8]);
    println!("Rust BEFORE: pc={:#018x} x1={:#018x} x2={:#018x} x8={:#018x} x24={:#018x} pstate={:#010x}",
        cpu.pc, cpu.regs[1], cpu.regs[2], cpu.regs[8], cpu.regs[24], cpu.pstate);

    match orchestrator.step_vcpu() {
        StepOutcome::Continue => {}
        other => panic!("CSINV probe did not continue: {other:?}"),
    }
    let cpu = &orchestrator.machine().cpu[0];
    println!("Rust AFTER: pc={:#018x} x1={:#018x} x2={:#018x} x8={:#018x} x24={:#018x} pstate={:#010x}",
        cpu.pc, cpu.regs[1], cpu.regs[2], cpu.regs[8], cpu.regs[24], cpu.pstate);
}
