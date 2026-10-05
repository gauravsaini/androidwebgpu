// Simple debug: run 15M steps, print PC every 1M
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const INITRD_PA: u64 = 0x4800_0000;

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

    println!("Running 15M steps...");
    for _ in 0..15_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        if orch.steps() % 1_000_000 == 0 {
            let pc = orch.machine().cpu[0].pc;
            let daif = orch.machine().cpu[0].sysregs.daif;
            let pending = orch.machine().irq.pending;
            let vbar = orch.machine().cpu[0].sysregs.vbar_el1;
            println!("  Step {}M: PC=0x{:x} DAIF=0x{:x} pending=0x{:x} VBAR=0x{:x}",
                     orch.steps()/1_000_000, pc, daif, pending, vbar);
        }
    }
    println!("Done at step {}, PC=0x{:x}", orch.steps(), orch.machine().cpu[0].pc);
}
