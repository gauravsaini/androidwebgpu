// Probe: Does IRQ delivery work? Does the 3-PC spin exit?
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

    println!("Testing IRQ delivery and spin exit...");
    let mut irq_deliveries = 0;
    let mut spin_hits = 0;
    let mut last_vbar: u64 = 0;
    let mut exited_spin = false;
    let mut steps_after_spin = 0u64;

    for _ in 0..80_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        
        let pc = orch.machine().cpu[0].pc;
        let vbar = orch.machine().cpu[0].sysregs.vbar_el1;
        if vbar != 0 && vbar != last_vbar {
            println!("  Step {}: VBAR_EL1 programmed = 0x{:x}", orch.steps(), vbar);
            last_vbar = vbar;
        }
        
        // Check if we're at IRQ vector (VBAR+0x280)
        if last_vbar != 0 && pc == last_vbar.wrapping_add(0x280) {
            irq_deliveries += 1;
            if irq_deliveries <= 5 {
                println!("  Step {}: IRQ DELIVERED! PC=0x{:x} (VBAR+0x280)", orch.steps(), pc);
            }
        }
        
        // Track spin
        if pc == SPIN_PC {
            spin_hits += 1;
            if spin_hits == 1 {
                println!("  Step {}: Entered 3-PC spin", orch.steps());
            }
        } else if spin_hits > 0 && !exited_spin {
            // We were in spin, now we're elsewhere
            exited_spin = true;
            println!("  Step {}: EXITED spin! PC=0x{:x} (after {} spin hits)", 
                     orch.steps(), pc, spin_hits);
        }
        
        if exited_spin {
            steps_after_spin += 1;
            if steps_after_spin >= 1_000_000 {
                println!("  Ran 1M steps after spin exit, PC=0x{:x}", pc);
                break;
            }
        }
        
        if orch.steps() % 10_000_000 == 0 {
            println!("  Step {}M, PC=0x{:x}, IRQs delivered: {}, spin hits: {}", 
                     orch.steps()/1_000_000, pc, irq_deliveries, spin_hits);
        }
    }
    
    println!("\n=== RESULTS ===");
    println!("Total steps: {}", orch.steps());
    println!("IRQ deliveries: {}", irq_deliveries);
    println!("Spin hits: {}", spin_hits);
    println!("Exited spin: {}", exited_spin);
    println!("Final PC: 0x{:x}", orch.machine().cpu[0].pc);
}
