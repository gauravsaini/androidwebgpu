//! P3 system-register end-to-end fixtures (2026-10-03, feat/emu-p3-impl).
//!
//! Executes small real instruction sequences (the exact MRS/MSR words the
//! AOSP kernel's GICv3/PMU/timer/FP/ID probe paths use) through the real
//! decode -> lift -> compile -> execute pipeline (`Orchestrator::step_vcpu`)
//! and asserts the semantic outcome: register values AND the persistent
//! sysreg state they write. Fixtures live in `fixtures/sysreg_p3_*.json`
//! (one per slice); this file is the runner.
//!
//! Memory: one `Orchestrator` (~1GiB) per sequence, sequential within each
//! test. Run with `-- --test-threads=1` on the sandbox.

use pathn_contracts::cpu::SysReg;
use std::collections::HashMap;
use u12_orchestrator::{Orchestrator, StepOutcome, RAM_BASE, TIMER_CYCLES_PER_STEP};

fn sysreg_by_name(name: &str) -> SysReg {
    match name {
        "icc_sre_el1" => SysReg::IccSreEl1,
        "icc_ctlr_el1" => SysReg::IccCtlrEl1,
        "icc_igrpen1_el1" => SysReg::IccIgrpen1El1,
        "icc_pmr_el1" => SysReg::IccPmrEl1,
        "icc_eoir1_el1" => SysReg::IccEoir1El1,
        "icc_dir_el1" => SysReg::IccDirEl1,
        "pmcntenset_el0" => SysReg::PmcntensetEl0,
        "pmcnt_enclr_el0" => SysReg::PmcntEnClrEl0,
        "pmovsclr_el0" => SysReg::PmovsclrEl0,
        "pmxevtyper_el0" => SysReg::PmxevtyperEl0,
        "pmxevcntr_el0" => SysReg::PmxevcntrEl0,
        "pmuserenr_el0" => SysReg::PmuserenrEl0,
        "cntkctl_el1" => SysReg::CntkctlEl1,
        "tpidr_el2" => SysReg::TpidrEl2,
        "fpcr" => SysReg::Fpcr,
        "fpsr" => SysReg::Fpsr,
        other => panic!("sysreg_p3 fixture: unknown sysreg {other}"),
    }
}

#[derive(Debug, serde::Deserialize)]
struct Step {
    word: u32,
    disasm: String,
}

#[derive(Debug, serde::Deserialize)]
struct TvalCheck {
    dst_reg: usize,
    written: u64,
    read_after_ticks: u64,
}

#[derive(Debug, serde::Deserialize)]
struct Sequence {
    name: String,
    steps: Vec<Step>,
    #[serde(default)]
    expected_regs: HashMap<String, u64>,
    #[serde(default)]
    expected_sysregs: HashMap<String, u64>,
    tval: Option<TvalCheck>,
}

fn run_sequences(json: &str) {
    let seqs: Vec<Sequence> = serde_json::from_str(json).expect("fixture parses");
    for seq in &seqs {
        let mut o = Orchestrator::new();
        o.machine_mut().cpu[0].pc = RAM_BASE;
        // Seed the physical counter so TVAL arithmetic is exact.
        o.machine_mut().irq.timer_count = 1_000_000;
        o.machine_mut().cpu[0].sysregs.cntpct_el0 = 1_000_000;
        for (i, step) in seq.steps.iter().enumerate() {
            let addr = RAM_BASE + (i as u64) * 4;
            let off = (addr - RAM_BASE) as usize;
            o.machine_mut().ram[off..off + 4].copy_from_slice(&step.word.to_le_bytes());
        }
        let mut tval_written_count: Option<u64> = None;
        for (i, step) in seq.steps.iter().enumerate() {
            let outcome = o.step_vcpu();
            assert_eq!(
                outcome,
                StepOutcome::Continue,
                "seq {} step {i} ({}) did not Continue: {outcome:?}",
                seq.name,
                step.disasm
            );
            // Capture the counter at the TVAL write so the later read can
            // be checked exactly (counter ticks TIMER_CYCLES_PER_STEP/step).
            if seq.tval.is_some() && step.disasm.starts_with("msr cntp_tval_el0") {
                tval_written_count = Some(o.machine().cpu[0].sysregs.cntpct_el0);
            }
        }
        let cpu = &o.machine().cpu[0];
        for (reg, want) in &seq.expected_regs {
            let idx: usize = reg.parse().expect("reg index");
            assert_eq!(
                cpu.regs[idx], *want,
                "seq {} x{idx}: got {:#x}, want {:#x}",
                seq.name, cpu.regs[idx], want
            );
        }
        for (name, want) in &seq.expected_sysregs {
            let got = o.machine().cpu[0].sysregs.load(sysreg_by_name(name));
            assert_eq!(
                got, *want,
                "seq {} sysreg {name}: got {:#x}, want {:#x}",
                seq.name, got, want
            );
        }
        if let Some(t) = &seq.tval {
            // TVAL read = written − ticks advanced since the write.
            let wrote_at = tval_written_count.expect("tval write observed");
            let want = t.written - TIMER_CYCLES_PER_STEP * t.read_after_ticks;
            let got = cpu.regs[t.dst_reg];
            assert_eq!(
                got, want,
                "seq {} tval: counter was {wrote_at:#x} at write, got {got:#x}, want {want:#x}",
                seq.name
            );
        }
        // Every step advanced PC by 4.
        assert_eq!(
            cpu.pc,
            RAM_BASE + (seq.steps.len() as u64) * 4,
            "seq {}: pc did not advance past all steps",
            seq.name
        );
    }
}

#[test]
fn p3_gic_cpu_interface_e2e() {
    run_sequences(include_str!("fixtures/sysreg_p3_gic.json"));
}

#[test]
fn p3_id_family_completion_e2e() {
    run_sequences(include_str!("fixtures/sysreg_p3_id.json"));
}

#[test]
fn p3_pmu_remainder_e2e() {
    run_sequences(include_str!("fixtures/sysreg_p3_pmu.json"));
}

#[test]
fn p3_timers_e2e() {
    run_sequences(include_str!("fixtures/sysreg_p3_timer.json"));
}

#[test]
fn p3_fpcr_fpsr_e2e() {
    run_sequences(include_str!("fixtures/sysreg_p3_fp.json"));
}
