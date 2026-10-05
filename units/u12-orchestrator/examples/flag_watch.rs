// Watchpoint probe: find WHO writes to the 3-PC spin flag.
// The spin does LDRB w8, [x23, #0x85e]; TBNZ w8, #6.
// This probe:
//  1. Runs to the spin PC (~41M steps)
//  2. Reads x23, computes flag VA, translates VA->PA via u4_mmu
//  3. Sets a byte-watchpoint on the PA (checks after every step)
//  4. Records EVERY change: step, PC-before, PC-after, old val, new val
//  5. Runs 10M steps past spin; reports "no writes" if none occur.
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE};
use u4_mmu::translate_with_base;
use pathn_contracts::cpu::{Access, MmuState};

const KERNEL_LOAD_PA: u64 = 0x4008_0000;
const DTB_PA: u64 = 0x4820_0000;
const INITRD_PA: u64 = 0x4800_0000;
const SP: u64 = 0x4830_0000;
const SPIN_PC: u64 = 0xffffff80085c96a0; // TBNZ w8, #6
const X23_OFFSET: u64 = 0x85e;
const POST_SPIN_STEPS: u64 = 10_000_000;

fn translate_va(orch: &Orchestrator, va: u64, access: Access) -> Result<u64, String> {
    let sys = &orch.machine().cpu[0].sysregs;
    let st = MmuState {
        sctlr: sys.sctlr_el1,
        tcr: sys.tcr_el1,
        ttbr0: sys.ttbr0_el1,
        ttbr1: sys.ttbr1_el1,
    };
    translate_with_base(&st, &orch.machine().ram, RAM_BASE, va, access)
        .map_err(|e| format!("{:?}", e))
}

fn main() {
    let mut orch = Orchestrator::new();
    let kernel = std::fs::read("guest-image/Image").expect("kernel Image");
    let dtb = std::fs::read("guest-image/minimal-virt.dtb").expect("dtb");
    let initrd = std::fs::read("guest-image/initramfs.cpio").expect("initrd");
    {
        let m = orch.machine_mut();
        let k_off = (KERNEL_LOAD_PA - RAM_BASE) as usize;
        m.ram[k_off..k_off + kernel.len()].copy_from_slice(&kernel);
        let d_off = (DTB_PA - RAM_BASE) as usize;
        m.ram[d_off..d_off + dtb.len()].copy_from_slice(&dtb);
        let i_off = (INITRD_PA - RAM_BASE) as usize;
        m.ram[i_off..i_off + initrd.len()].copy_from_slice(&initrd);
        m.cpu[0].pc = KERNEL_LOAD_PA;
        m.cpu[0].regs[0] = DTB_PA;
        m.cpu[0].sp = SP;
    }

    println!("=== Phase 1: run to spin PC 0x{:x} ===", SPIN_PC);
    let mut hit_step: Option<u64> = None;
    for _ in 0..120_000_000 {
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                return;
            }
            _ => {}
        }
        if orch.machine().cpu[0].pc == SPIN_PC {
            hit_step = Some(orch.steps());
            break;
        }
        if orch.steps() % 10_000_000 == 0 {
            println!("  step {}M pc=0x{:x}", orch.steps() / 1_000_000, orch.machine().cpu[0].pc);
        }
    }
    let spin_step = match hit_step {
        Some(s) => s,
        None => {
            println!("RESULT: never reached spin PC in 120M steps");
            return;
        }
    };
    println!("Spin reached at step {}", spin_step);

    // Resolve the flag address dynamically from x23.
    let x23 = orch.machine().cpu[0].regs[23];
    let flag_va = x23.wrapping_add(X23_OFFSET);
    println!("x23=0x{:016x} flag_va=0x{:016x}", x23, flag_va);

    // Page-mapping check.
    let pa = match translate_va(&orch, flag_va, Access::Read) {
        Ok(pa) => pa,
        Err(e) => {
            println!("RESULT: flag VA not mapped for read: {}", e);
            println!("VERDICT: PAGE NOT MAPPED (translation fault) -- setter cannot be running via normal stores; check vmemmap fault handling");
            return;
        }
    };
    // Also check write permission explicitly.
    match translate_va(&orch, flag_va, Access::Write) {
        Ok(_) => println!("page: mapped, WRITE allowed, pa=0x{:x}", pa),
        Err(e) => {
            println!("page: mapped for read (pa=0x{:x}) but WRITE fault: {}", pa, e);
            println!("VERDICT: PAGE READ-ONLY -- no writer can set bit 6 via normal store");
            return;
        }
    }
    if pa < RAM_BASE {
        println!("RESULT: pa 0x{:x} below RAM_BASE -- not in emulated RAM (MMIO?)", pa);
        return;
    }
    let ram_off = (pa - RAM_BASE) as usize;
    if ram_off >= orch.machine().ram.len() {
        println!("RESULT: pa 0x{:x} outside ram slice", pa);
        return;
    }

    let mut last = orch.machine().ram[ram_off];
    println!(
        "watchpoint armed: pa=0x{:x} ram_off=0x{:x} initial=0x{:02x} (bit6={})",
        pa,
        ram_off,
        last,
        (last >> 6) & 1
    );

    println!("=== Phase 2: watch for {} steps ===", POST_SPIN_STEPS);
    let mut writes: Vec<(u64, u64, u64, u8, u8)> = Vec::new(); // step, pc_before, pc_after, old, new
    let target = POST_SPIN_STEPS;
    let mut done = 0u64;
    while done < target {
        let pc_before = orch.machine().cpu[0].pc;
        match orch.step_vcpu() {
            StepOutcome::Halted(h) => {
                println!("HALTED at step {}: {:?}", orch.steps(), h);
                break;
            }
            _ => {}
        }
        done += 1;
        let cur = orch.machine().ram[ram_off];
        if cur != last {
            let pc_after = orch.machine().cpu[0].pc;
            writes.push((orch.steps(), pc_before, pc_after, last, cur));
            println!(
                "WRITE #{} step={} pc_before=0x{:x} pc_after=0x{:x} 0x{:02x}->0x{:02x} (bit6 {}->{})",
                writes.len(),
                orch.steps(),
                pc_before,
                pc_after,
                last,
                cur,
                (last >> 6) & 1,
                (cur >> 6) & 1
            );
            last = cur;
            if writes.len() >= 50 {
                println!("(capped at 50 write records)");
                break;
            }
        }
        if done % 2_000_000 == 0 {
            println!("  watch progress: {}M/{}M steps, writes so far: {}", done / 1_000_000, target / 1_000_000, writes.len());
        }
    }

    println!("\n=== RESULT ===");
    println!("steps_watched: {}", done);
    println!("writes_observed: {}", writes.len());
    if writes.is_empty() {
        println!("VERDICT: NO WRITES -- the flag setter is NOT running.");
        println!("The kernel spins waiting for bit 6 at va 0x{:x} (pa 0x{:x}),", flag_va, pa);
        println!("but nothing stores to that byte in {}M steps.", done / 1_000_000);
        println!("Next: identify which driver owns x23+0x85e and what should set bit 6");
        println!("(likely IRQ handler / kthread / workqueue that never gets scheduled).");
    } else {
        let bit6_ever_set = writes.iter().any(|&(_, _, _, _, new)| (new >> 6) & 1 == 1);
        println!("VERDICT: {} write(s) observed; bit 6 ever set: {}", writes.len(), bit6_ever_set);
        if !bit6_ever_set {
            println!("Writes happen but bit 6 is NEVER set -- setter runs but sets wrong bits,");
            println!("or a different field than the poller expects.");
        }
        for (i, (step, pb, pa_, old, new)) in writes.iter().enumerate() {
            println!(
                "  [{}] step={} pc 0x{:x}->0x{:x} val 0x{:02x}->0x{:02x}",
                i, step, pb, pa_, old, new
            );
        }
    }
}
