//! GB-26 probe (diagnostic, not committed as a product test):
//! Run the real AOSP kernel until halt, then capture the exact halt
//! instruction word through the real stage-1 MMU path.
//!
//! Usage: cargo run --release -p u12-orchestrator --example gb26_probe

use pathn_contracts::cpu::{Access, MmuState};
use u12_orchestrator::{HaltReason, Orchestrator, StepOutcome, RAM_BASE};

const KERNEL_PATH: &str = "/mnt/sdb1/aosp/Image";
const MAX_STEPS: u64 = 3_000_000;

fn read_word(ram: &[u8], pa: u64) -> Option<u32> {
    let off = pa.checked_sub(RAM_BASE)? as usize;
    let bytes = ram.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn main() {
    let kernel_bytes = std::fs::read(KERNEL_PATH).expect("kernel image missing");
    let mut orch = Orchestrator::new();
    {
        let m = orch.machine_mut();
        let load_len = kernel_bytes.len().min(m.ram.len());
        m.ram[..load_len].copy_from_slice(&kernel_bytes[..load_len]);
        m.cpu[0].pc = RAM_BASE;
        m.cpu[0].sp = RAM_BASE + 0x0800_0000;
        m.cpu[0].regs = [0; 31];
    }

    let mut halt: Option<(u64, u64, HaltReason)> = None;
    for step in 0..MAX_STEPS {
        match orch.step_vcpu() {
            StepOutcome::Continue => {}
            StepOutcome::WfiYield { addr } => {
                halt = Some((step, addr, HaltReason::Wfi { addr }));
                break;
            }
            StepOutcome::Halted(reason) => {
                let pc = orch.machine().cpu[0].pc;
                halt = Some((step, pc, reason));
                break;
            }
        }
        if step % 500_000 == 0 && step > 0 {
            eprintln!("... step {step}, pc={:#x}", orch.machine().cpu[0].pc);
        }
    }
    let (step, pc, reason) = halt.expect("no halt within step cap");

    // Translate the halt VA through the real stage-1 path (live sysregs,
    // same source the executor's own fetch uses).
    let m = orch.machine();
    let st = MmuState {
        sctlr: m.cpu[0].sysregs.sctlr_el1,
        tcr: m.cpu[0].sysregs.tcr_el1,
        ttbr0: m.cpu[0].sysregs.ttbr0_el1,
        ttbr1: m.cpu[0].sysregs.ttbr1_el1,
    };
    let word_line = if st.sctlr & 1 == 0 {
        match read_word(&m.ram, pc) {
            Some(w) => format!("{w:#010x} (identity map, MMU off)"),
            None => "<ram read OOB, MMU off>".to_string(),
        }
    } else {
        match u4_mmu::translate_with_base(&st, &m.ram, RAM_BASE, pc, Access::Execute) {
            Ok(pa) => match read_word(&m.ram, pa) {
                Some(w) => format!("{w:#010x} (va {pc:#x} -> pa {pa:#x})"),
                None => format!("<ram read OOB at pa {pa:#x}>"),
            },
            Err(e) => format!("<translation fault: {e:?}>"),
        }
    };

    println!("GB26_HALT step={step} pc={pc:#x}");
    println!("GB26_HALT word={word_line}");
    println!("GB26_HALT reason={reason:?}");
    println!("GB26_HALT sp={:#x}", m.cpu[0].sp);
    println!(
        "GB26_HALT sysregs sctlr={:#x} tcr={:#x} ttbr0={:#x} ttbr1={:#x}",
        st.sctlr, st.tcr, st.ttbr0, st.ttbr1
    );
    println!(
        "GB26_HALT x0={:#x} x1={:#x}",
        m.cpu[0].regs[0], m.cpu[0].regs[1]
    );
}
