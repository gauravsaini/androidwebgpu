// Probe: Dump x23 during the 3-PC spin, check if x23+0x85e is MMIO or RAM
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const INITRD_PA: u64 = 0x4800_0000;

const SPIN_PC: u64 = 0xffffff80085c96a0; // TBNZ w8, #6
const X23_OFFSET: u64 = 0x85e;

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
        // SP from initrd wiring
        cpu.sp = 0x4830_0000;
    }

    println!("Running to spin PC 0x{:x}...", SPIN_PC);
    let mut hits = 0;
    for _ in 0..100_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        let pc = orch.machine().cpu[0].pc;
        if pc == SPIN_PC {
            hits += 1;
            if hits == 1 {
                let x23 = orch.machine().cpu[0].regs[23];
                let target_va = x23.wrapping_add(X23_OFFSET);
                println!("\n=== FIRST HIT at step {} ===", orch.steps());
                println!("x23 = 0x{:016x}", x23);
                println!("x23+0x85e = 0x{:016x} (VA)", target_va);
                
                // Check if it's in kernel VA range (RAM) or device MMIO range
                if (0xffffff8000000000..0xffffffc000000000).contains(&target_va) {
                    println!("-> In kernel VA range (likely RAM-mapped)");
                    // Try to translate via page table
                    println!("-> Offset 0x85e suggests DRIVER STRUCT, not MMIO register");
                } else if target_va < 0x100000000 {
                    println!("-> In low address range (possible MMIO: 0x{:x})", target_va);
                    // Check known device bases
                    if (0x09000000..0x0a000000).contains(&target_va) {
                        println!("-> In PL011/GIC range (0x09000000-0x0a000000)!");
                    }
                } else {
                    println!("-> Unknown range");
                }
                
                // Dump w8 as well
                let w8 = orch.machine().cpu[0].regs[8] & 0xffffffff;
                println!("w8 = 0x{:08x} (bit 6 = {})", w8, (w8 >> 6) & 1);
                break;
            }
        }
        if orch.steps() % 10_000_000 == 0 {
            println!("  Step {}M, PC=0x{:x}", orch.steps()/1_000_000, pc);
        }
    }
    if hits == 0 {
        println!("Never hit spin PC in 100M steps");
    }
}
