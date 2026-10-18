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
fn p3_mmfr0_parange_mrs_to_tcr_matches_pinned_qemu() {
    // Exact Linux Image instructions at 0x40c03694..0x40c036bc:
    // MRS ID_AA64MMFR0_EL1; extract PARange; cap at 5; insert into TCR.IPS;
    // then write TCR_EL1. The pinned QEMU trace records the resulting TCR.
    const WORDS: [u32; 7] = [
        0xd538_0705, // mrs x5, id_aa64mmfr0_el1
        0xd340_08a5, // ubfx x5, x5, #0, #3
        0xd280_00a6, // mov x6, #5
        0xeb06_00bf, // cmp x5, x6
        0x9a85_80c5, // csel x5, x6, x5, hi
        0xb360_08aa, // bfi x10, x5, #32, #3
        0xd518_204a, // msr tcr_el1, x10
    ];
    const QEMU_TCR_EL1: u64 = 0x0040_0032_b559_3519;
    const PRE_IPS_TCR_EL1: u64 = 0x0040_0030_b559_3519;

    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    o.machine_mut().cpu[0].regs[10] = PRE_IPS_TCR_EL1;
    for (i, word) in WORDS.iter().enumerate() {
        let offset = i * 4;
        o.machine_mut().ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    for (i, word) in WORDS.iter().enumerate() {
        let outcome = o.step_vcpu();
        assert_eq!(
            outcome,
            StepOutcome::Continue,
            "guest vector word {i} ({word:#010x}) did not Continue: {outcome:?}"
        );
        if i == 0 {
            assert_eq!(
                o.machine().cpu[0].regs[5],
                2,
                "Cortex-A53 MRS must expose PARange=2 (40-bit PA)"
            );
        }
    }

    assert_eq!(
        o.machine().cpu[0].regs[5],
        2,
        "Linux's PARange extraction/cap must retain the Cortex-A53 value"
    );
    assert_eq!(
        o.machine().cpu[0].sysregs.tcr_el1,
        QEMU_TCR_EL1,
        "Linux's exact MRS->TCR sequence must match the pinned QEMU trace"
    );
}

#[test]
fn p3_ctr_el0_matches_cortex_a53_qemu() {
    // The pinned Cortex-A53 QEMU trace reaches MRS X3, CTR_EL0 at step 19
    // (0x40084d60); its next state has X3=0x84448004.
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    o.machine_mut().ram[..4].copy_from_slice(&0xd53b_0023u32.to_le_bytes());

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(o.machine().cpu[0].regs[3], 0x8444_8004);
}

#[test]
fn p3_dfr0_el1_matches_cortex_a53_qemu() {
    // The pinned Cortex-A53 QEMU trace reaches MRS X0, ID_AA64DFR0_EL1 at
    // step 7469 (0x40c83660); its next state at step 7470 has X0=0x10305106.
    // In Linux Image at 0x40c03660:
    // MRS X0, ID_AA64DFR0_EL1 (0xd538_0500)
    // SBFX X0, X0, #8, #4      (0x9348_2c00)
    // CMP X0, #1               (0xf100_041f)
    // B.LT 0x40c03674 (+8)     (0x5400_004b)
    // With DFR0=0x10305106, bits[11:8] = 0x1 -> SBFX gives 1.
    // CMP 1, 1 gives Z=1, so B.LT is NOT taken (branch fallthrough).
    const WORDS: [u32; 4] = [
        0xd538_0500, // mrs x0, id_aa64dfr0_el1
        0x9348_2c00, // sbfx x0, x0, #8, #4
        0xf100_041f, // cmp x0, #1
        0x5400_004b, // b.lt +8 (to RAM_BASE + 20)
    ];
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    for (i, word) in WORDS.iter().enumerate() {
        let offset = i * 4;
        o.machine_mut().ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[0],
        0x1030_5106,
        "Cortex-A53 MRS ID_AA64DFR0_EL1 must match QEMU value 0x10305106"
    );

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[0],
        1,
        "SBFX bits [11:8] must yield 1"
    );

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    // CMP 1, 1 sets Zero flag (Z=1, bit 30)
    assert_eq!(
        (o.machine().cpu[0].pstate >> 30) & 1,
        1,
        "CMP 1, 1 must set Z flag"
    );

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    // B.LT is NOT taken, PC advances to RAM_BASE + 16 (0x4000_0010), NOT RAM_BASE + 20
    assert_eq!(
        o.machine().cpu[0].pc,
        RAM_BASE + 16,
        "B.LT must not be taken with DFR0=0x10305106; must fall through to next insn"
    );
}

#[test]
fn p3_dczid_el0_matches_cortex_a53_qemu() {
    // The dense Cortex-A53 QEMU trace returns DCZID_EL0=0x04 for this MRS.
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    o.machine_mut().ram[..4].copy_from_slice(&0xd53b_00e3u32.to_le_bytes());

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(o.machine().cpu[0].regs[3], 0x04);
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

#[test]
fn p3_fdt_totalsize_load_rev_matches_cortex_a53_qemu() {
    // Exact instruction sequence from Linux setup_machine_fdt() at step 33,326:
    // LDR W8, [X23, #4] (word 0xb940_06e8)
    // REV W8, W8        (word 0x5ac0_0908)
    // In Cortex-A53 virt QEMU oracle, the DTB totalsize field at offset 4 is
    // 0x00005992 (22,930 bytes in big-endian, matching QEMU arm_load_dtb).
    // The load produces 0x92590000 and REV converts it to 0x5992.
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    let dtb_addr = RAM_BASE + 0x1000;
    o.machine_mut().cpu[0].regs[23] = dtb_addr;
    let dtb_off = (dtb_addr - RAM_BASE) as usize;
    o.machine_mut().ram[dtb_off..dtb_off + 4].copy_from_slice(&0xd00d_feedu32.to_be_bytes());
    o.machine_mut().ram[dtb_off + 4..dtb_off + 8].copy_from_slice(&0x0000_5992u32.to_be_bytes());

    const WORDS: [u32; 2] = [
        0xb940_06e8, // ldr w8, [x23, #4]
        0x5ac0_0908, // rev w8, w8
    ];
    for (i, word) in WORDS.iter().enumerate() {
        let offset = i * 4;
        o.machine_mut().ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[8],
        0x9259_0000,
        "LDR W8, [X23, #4] must load big-endian 0x00005992 as 0x92590000"
    );

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[8],
        0x5992,
        "REV W8, W8 must yield totalsize 0x5992 matching QEMU step 33,327"
    );
}

#[test]
fn p3_mpidr_midr_el1_matches_cortex_a53_qemu() {
    // Exact instruction sequence from Linux setup_arch() at QEMU steps 41,104 / 41,112:
    // MRS X9, MPIDR_EL1 (word 0xd538_00a9)
    // MRS X2, MIDR_EL1  (word 0xd538_0002)
    // In Cortex-A53 virt QEMU oracle:
    // MPIDR_EL1 has RES1 bit 31 = 1, U bit 30 = 0 (multiprocessor-capable), Aff0 = 0 -> 0x8000_0000.
    // MIDR_EL1 is Cortex-A53 r0p4 -> 0x410f_d034.
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;

    const WORDS: [u32; 2] = [
        0xd538_00a9, // mrs x9, mpidr_el1
        0xd538_0002, // mrs x2, midr_el1
    ];
    for (i, word) in WORDS.iter().enumerate() {
        let offset = i * 4;
        o.machine_mut().ram[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[9],
        0x8000_0000,
        "MRS X9, MPIDR_EL1 must return 0x8000_0000 matching QEMU step 41,105"
    );

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[2],
        0x410f_d034,
        "MRS X2, MIDR_EL1 must return 0x410fd034 matching QEMU step 41,113"
    );
}

#[test]
fn p3_psci_version_hvc_matches_cortex_a53_qemu() {
    // Exact instruction from Linux psci probe at QEMU step 36,828,786:
    // HVC #0 (word 0xd400_0002) with X0 = 0x8400_0000 (PSCI_0_2_FN_PSCI_VERSION).
    // In Cortex-A53 virt QEMU 11.1.2 oracle (target/arm/tcg/psci.c:95-96):
    // PSCI_VERSION returns 0x0001_0001 (PSCI v1.1).
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    o.machine_mut().cpu[0].regs[0] = 0x8400_0000;

    const WORDS: [u32; 1] = [
        0xd400_0002, // hvc #0
    ];
    o.machine_mut().ram[0..4].copy_from_slice(&WORDS[0].to_le_bytes());

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[0],
        0x0001_0001,
        "HVC #0 with X0=0x8400_0000 (PSCI_VERSION) must return 0x0001_0001 (v1.1) matching QEMU step 36,828,787"
    );
}

#[test]
fn p3_psci_migrate_info_type_matches_cortex_a53_qemu() {
    // In Cortex-A53 virt QEMU 11.1.2 oracle (target/arm/tcg/psci.c:98-99):
    // MIGRATE_INFO_TYPE (0x8400_0006) returns 2 (PSCI_0_2_RET_TOS_MIGRATION_NOT_REQUIRED).
    let mut o = Orchestrator::new();
    o.machine_mut().cpu[0].pc = RAM_BASE;
    o.machine_mut().cpu[0].regs[0] = 0x8400_0006;

    const WORDS: [u32; 1] = [
        0xd400_0002, // hvc #0
    ];
    o.machine_mut().ram[0..4].copy_from_slice(&WORDS[0].to_le_bytes());

    assert_eq!(o.step_vcpu(), StepOutcome::Continue);
    assert_eq!(
        o.machine().cpu[0].regs[0],
        2,
        "HVC #0 with X0=0x8400_0006 (MIGRATE_INFO_TYPE) must return 2 (TOS_MIGRATION_NOT_REQUIRED) matching QEMU"
    );
}


