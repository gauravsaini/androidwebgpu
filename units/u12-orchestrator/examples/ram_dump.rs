// Probe: Dump 200 instructions from RAM around spin PC to identify function
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const INITRD_PA: u64 = 0x4800_0000;
const SPIN_PC: u64 = 0xffffff80085c96a0;

fn va_to_ram_offset(va: u64) -> usize {
    // VA 0xffffff8008000000 -> PA 0x40080000
    // PA = VA - 0xffffff8008000000 + 0x40080000
    // RAM offset = PA - RAM_BASE
    (va - 0xffffff8008000000 + 0x40080000 - RAM_BASE) as usize
}

fn main() {
    let mut orch = Orchestrator::new();
    let kernel = std::fs::read("guest-image/Image").expect("kernel");
    let dtb = std::fs::read("guest-image/minimal-virt.dtb").expect("dtb");
    let initrd = std::fs::read("guest-image/initramfs.cpio").expect("initrd");
    {
        let k_off = (KERNEL_LOAD_PA - RAM_BASE) as usize;
        orch.machine_mut().ram[k_off..k_off+kernel.len()].copy_from_slice(&kernel);
        let d_off = (DTB_PA - RAM_BASE) as usize;
        orch.machine_mut().ram[d_off..d_off+dtb.len()].copy_from_slice(&dtb);
        let i_off = (INITRD_PA - RAM_BASE) as usize;
        orch.machine_mut().ram[i_off..i_off+initrd.len()].copy_from_slice(&initrd);
    }
    {
        let cpu = &mut orch.machine_mut().cpu[0];
        cpu.pc = KERNEL_LOAD_PA;
        cpu.regs[0] = DTB_PA;
        cpu.sp = 0x4830_0000;
    }

    println!("Running to spin PC...");
    for _ in 0..100_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED: {:?}", h);
                break;
            }
            _ => {}
        }
        if orch.machine().cpu[0].pc == SPIN_PC {
            println!("\n=== Hit spin PC at step {} ===", orch.steps());
            println!("Dumping 100 instructions before and after from RAM:\n");
            
            let start_va = SPIN_PC - 400;
            for i in 0..200 {
                let va = start_va + (i * 4);
                let off = va_to_ram_offset(va);
                if off + 4 <= orch.machine().ram.len() {
                    let insn = u32::from_le_bytes([
                        orch.machine().ram[off],
                        orch.machine().ram[off+1],
                        orch.machine().ram[off+2],
                        orch.machine().ram[off+3],
                    ]);
                    let marker = if va == SPIN_PC { " <-- SPIN" } 
                        else if va == SPIN_PC - 4 { " <-- LDRB?" } 
                        else { "" };
                    println!("0x{:x}: 0x{:08x}{}", va, insn, marker);
                }
            }
            break;
        }
    }
}
