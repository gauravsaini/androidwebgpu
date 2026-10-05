// Debug: Why is IRQ delivery not firing? Dump timer/IRQ state at spin.
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const INITRD_PA: u64 = 0x4800_0000;
const SPIN_PC: u64 = 0xffffff80085c96a0;

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

    println!("Running to spin, will dump IRQ state...");
    let mut spin_hits = 0;
    for _ in 0..50_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        let pc = orch.machine().cpu[0].pc;
        if pc == SPIN_PC {
            spin_hits += 1;
            if spin_hits == 1 {
                let cpu = &orch.machine().cpu[0];
                let irq = &orch.machine().irq;
                println!("\n=== SPIN REACHED at step {} ===", orch.steps());
                println!("irq.pending = 0x{:x}", irq.pending);
                println!("irq.enabled = 0x{:x}", irq.enabled);
                println!("DAIF = 0x{:x} (I bit masked: {})", cpu.sysregs.daif, (cpu.sysregs.daif & 0x80) != 0);
                println!("VBAR_EL1 = 0x{:x}", cpu.sysregs.vbar_el1);
                println!("CNTP_CTL_EL0 = 0x{:x} (ENABLE={})", cpu.sysregs.cntp_ctl_el0, cpu.sysregs.cntp_ctl_el0 & 1);
                println!("CNTP_CVAL_EL0 = 0x{:x}", cpu.sysregs.cntp_cval_el0);
                println!("CNTPCT_EL0 = 0x{:x}", cpu.sysregs.cntpct_el0);
                println!("timer_compare = 0x{:x}", irq.timer_compare);
                println!("timer_count = 0x{:x}", irq.timer_count);
                println!("\nDelivery check:");
                println!("  pending != 0: {}", irq.pending != 0);
                println!("  DAIF.I == 0 (unmasked): {}", (cpu.sysregs.daif & 0x80) == 0);
                println!("  VBAR != 0: {}", cpu.sysregs.vbar_el1 != 0);
                println!("  ALL TRUE (would deliver): {}", 
                    irq.pending != 0 && (cpu.sysregs.daif & 0x80) == 0 && cpu.sysregs.vbar_el1 != 0);
                break;
            }
        }
    }
    if spin_hits == 0 {
        println!("Never reached spin PC");
    }
}
