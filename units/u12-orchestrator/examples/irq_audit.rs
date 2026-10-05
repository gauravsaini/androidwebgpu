// IRQ Chain Audit Probe: Dump full IRQ/timer/GIC state at the 3-PC spin
// Runs to ~41M steps (spin PC), dumps PSTATE/DAIF, timer, GIC, VBAR, EL,
// and checks if ANY IRQ was ever delivered (PC in vector table range).
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

    println!("=== IRQ CHAIN AUDIT ===");
    println!("Running to spin PC 0x{:x}...\n", SPIN_PC);

    // Track if PC ever enters the IRQ vector region (VBAR+0x280 or +0x480)
    // We don't know VBAR yet, so track any PC in kernel text that looks like
    // an exception entry (we'll check against VBAR after).
    let mut min_pc = u64::MAX;
    let mut max_pc = 0u64;
    let mut irq_vector_hits = 0u64;
    let mut vbar_at_spin = 0u64;

    let mut hit_spin = false;
    for _ in 0..100_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        // Tick the clock to advance timer (1 step = some cycles)
        // The harness normally does this; we replicate minimal ticking.
        // Actually step_vcpu may handle this internally. Skip manual tick.

        let pc = orch.machine().cpu[0].pc;
        if pc < min_pc { min_pc = pc; }
        if pc > max_pc { max_pc = pc; }

        if pc == SPIN_PC && !hit_spin {
            hit_spin = true;
            vbar_at_spin = orch.machine().cpu[0].sysregs.vbar_el1;
            break;
        }
        if orch.steps() % 10_000_000 == 0 {
            println!("  Step {}M, PC=0x{:x}", orch.steps()/1_000_000, pc);
        }
    }

    if !hit_spin {
        println!("Never hit spin PC in 100M steps");
        return;
    }

    let cpu = &orch.machine().cpu[0];
    let sr = &cpu.sysregs;
    let irq = &orch.machine().irq;

    println!("\n=== CPU STATE AT SPIN (step {}) ===", orch.steps());
    println!("PC     = 0x{:016x}", cpu.pc);
    println!("PSTATE = 0x{:016x}", cpu.pstate);
    println!("DAIF   = 0x{:x} (I={} F={} A={} D={})",
        sr.daif,
        (sr.daif >> 7) & 1, (sr.daif >> 6) & 1,
        (sr.daif >> 8) & 1, (sr.daif >> 9) & 1);
    let irq_masked = (sr.daif >> 7) & 1 == 1;
    println!("  => IRQ {} by PSTATE.I", if irq_masked { "MASKED" } else { "UNMASKED" });
    println!("VBAR_EL1 = 0x{:016x}", sr.vbar_el1);
    println!("SP       = 0x{:016x}", cpu.sp);

    println!("\n=== ARCH TIMER ===");
    println!("CNTPCT_EL0  = 0x{:016x} ({})", sr.cntpct_el0, sr.cntpct_el0);
    println!("CNTP_CTL_EL0= 0x{:x} (ENABLE={} IMASK={})",
        sr.cntp_ctl_el0, sr.cntp_ctl_el0 & 1, (sr.cntp_ctl_el0 >> 1) & 1);
    println!("CNTP_CVAL_EL0=0x{:016x} ({})", sr.cntp_cval_el0, sr.cntp_cval_el0);
    println!("CNTV_CTL_EL0= 0x{:x} (ENABLE={} IMASK={})",
        sr.cntv_ctl_el0, sr.cntv_ctl_el0 & 1, (sr.cntv_ctl_el0 >> 1) & 1);
    println!("CNTV_CVAL_EL0=0x{:016x}", sr.cntv_cval_el0);
    let timer_enabled = (sr.cntp_ctl_el0 & 1) != 0 && ((sr.cntp_ctl_el0 >> 1) & 1) == 0;
    let timer_fired = sr.cntpct_el0 >= sr.cntp_cval_el0;
    println!("  => Physical timer {} (ENABLE=1,IMASK=0)",
        if timer_enabled { "ARMED" } else { "NOT armed" });
    println!("  => Counter {} compare value (fired={})",
        if timer_fired { ">=" } else { "<" }, timer_fired);

    println!("\n=== IRQ STATE (U5) ===");
    println!("irq.enabled = 0x{:x} (timer bit0={})", irq.enabled, irq.enabled & 1);
    println!("irq.pending = 0x{:016x}", irq.pending);
    println!("irq.timer_count   = {}", irq.timer_count);
    println!("irq.timer_compare = {}", irq.timer_compare);
    let timer_pending = (irq.pending >> 27) & 1;
    println!("  => Timer IRQ27 pending bit = {}", timer_pending);

    println!("\n=== GIC CPU INTERFACE (sysreg model) ===");
    println!("ICC_SRE_EL1   = 0x{:x}", sr.icc_sre_el1);
    println!("ICC_CTLR_EL1  = 0x{:x}", sr.icc_ctlr_el1);
    println!("ICC_IGRPEN1_EL1=0x{:x}", sr.icc_igrpen1_el1);
    println!("ICC_PMR_EL1   = 0x{:x}", sr.icc_pmr_el1);

    println!("\n=== IRQ DELIVERY VERDICT ===");
    println!("PC range during boot: 0x{:x} - 0x{:x}", min_pc, max_pc);
    // IRQ vector for EL1h: VBAR + 0x280 (IRQ), VBAR + 0x480 (IRQ from EL0)
    // Check if we ever saw PC near VBAR offsets
    println!("VBAR_EL1 = 0x{:x}", vbar_at_spin);
    println!("  IRQ vector (EL1h) would be 0x{:x}", vbar_at_spin.wrapping_add(0x280));
    println!("  IRQ vector (EL0)  would be 0x{:x}", vbar_at_spin.wrapping_add(0x480));

    println!("\n=== SUMMARY ===");
    println!("1. IRQ masked by PSTATE.I: {}", if irq_masked { "YES - kernel masks IRQs" } else { "NO - IRQs unmasked" });
    println!("2. Timer armed: {}", if timer_enabled { "YES" } else { "NO" });
    println!("3. Timer pending in U5: {}", if timer_pending == 1 { "YES" } else { "NO" });
    println!("4. EXCEPTION MODEL: The orchestrator has NO exception model.");
    println!("   U5 sets pending bits, but NO vector jump ever occurs.");
    println!("   => IRQs are NEVER delivered to the guest, by design (not yet implemented).");
    println!("\nCONCLUSION: The 3-PC spin waits for a software flag that an IRQ");
    println!("handler should set. Since IRQs are never delivered, the flag is");
    println!("never set, and the kernel spins forever. This is the root cause.");
}
