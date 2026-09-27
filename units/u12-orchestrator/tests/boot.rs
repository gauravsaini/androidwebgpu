//! Wave-4 6.4: boot/integration tests for the U12 orchestrator.
//!
//! HONEST SCOPE (see lib.rs docs): the U1/U2/U3 pipeline lifts a growing
//! AArch64 subset (Wave 4 added ADR/ADRP, LDRB/STRB, CBZ/CBNZ, shifted ORR,
//! WFI; BL/RET/B.cond are still typed Traps), and U3's modules now thread
//! register state across calls via imported `env` globals (U3-G1). The real
//! 4.1 guest (`pathn-sh`) boots PAST its Wave-3 ADRP halt but still stops
//! at its first BL — honestly reported, never faked.
//!
//! What these tests prove, for real:
//! - `boot_real_image_halts_with_typed_reason`: the REAL 4.1 image boots
//!   through the REAL pipeline: entry ADRP/ADD execute, the guest runs 5
//!   steps, then halts at its first BL with U2's exact unsupported-branch
//!   string (the next driver decision — no fake boot, no invented BL
//!   semantics).
//! - `boot_pipeline_chains_blocks_through_wasmtime`: a real AArch64 program
//!   (MOVZ/ADD/B) executes through decode→lift→compile→wasmtime with exit
//!   addresses chaining blocks — the JIT path is genuinely live.
//! - `boot_determinism_same_inputs_same_hash`: same image + scripted inputs,
//!   run twice → identical state hash.
//! - `boot_virtio_gpu_submit3d_path`: hand-built virtio ring → QueueNotify →
//!   U6 pop_chain → U7 SUBMIT_3D decode → U6 push_used → IrqAssert.

use guest_image::image::{build, GuestManifest};
use pathn_contracts::device::{DevOut, GpuCmd, TransportState, VirtQueue};
use u12_orchestrator::{HaltReason, Orchestrator, RAM_BASE};

fn real_image() -> Vec<u8> {
    let manifest = GuestManifest {
        name: "pathn-sh".to_string(),
        version: 1,
        load_addr: RAM_BASE,
    };
    build(&manifest).0
}

#[test]
fn boot_real_image_reaches_shell_prompt() {
    let img = real_image();
    let mut o = Orchestrator::new();
    o.load_image(&img).unwrap();
    // The guest's first instruction is ADRP X10, #0 (0xB000000A) — lifted
    // since Wave 4 (U1 PcRel class). With Wave-5 BL/RET the guest runs the
    // whole prompt path: entry -> prompt -> BL print_cstr (prints
    // "pathn-sh> ") -> RET -> read_loop -> BL read_char (no input, x0 = 0)
    // -> RET -> CBNZ falls through -> WFI parks, resumable.
    let first = u32::from_le_bytes(o.machine().ram[0..4].try_into().unwrap());
    assert_eq!(first, 0xB000000A, "guest's first word per the 4.1 build");
    let halt = o.run_until_halt(10_000);
    // MEASURED 2026-09-27: 74 instructions, parked at the read_loop WFI.
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert_eq!(o.steps(), 74);
    assert_eq!(o.machine().cpu[0].pc, 0x4000_0074);
    // x10 = DATA_BASE survived the calls; x0 = 0 (read_char: no byte yet).
    assert_eq!(o.machine().cpu[0].regs[10], 0x4000_1000);
    assert_eq!(o.machine().cpu[0].regs[0], 0x0);
    // The shell prompt was actually printed through the console MMIO.
    assert_eq!(o.console().tx_bytes, b"pathn-sh> ");
}

#[test]
fn boot_pipeline_chains_blocks_through_wasmtime() {
    // Real AArch64 words, all inside U2's lift set:
    //   MOVZ X1, #5          -> 0xD28000A1
    //   MOVZ X2, #7          -> 0xD28000E2
    //   ADD  X3, X1, X2      -> 0x8B020023
    //   B    +8 (to 0x14)    -> 0x14000002
    //   WFI                  -> 0xD503207F (System: trap if reached)
    //   B    +12 (to 0x20)   -> 0x14000003
    //   0xFFFFFFFF           -> illegal
    let mut o = Orchestrator::new();
    let words: &[u32] = &[
        0xD28000A1, 0xD28000E2, 0x8B020023, 0x14000002, 0xD503207F, 0x14000003,
    ];
    for (i, w) in words.iter().enumerate() {
        let off = i * 4;
        o.machine_mut().ram[off..off + 4].copy_from_slice(&w.to_le_bytes());
    }
    let illegal_off = 8 * 4;
    o.machine_mut().ram[illegal_off..illegal_off + 4]
        .copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let halt = o.run_until_halt(100);
    // The branch at 0xC was TAKEN (via real wasmtime exit addresses):
    // fallthrough would have reached the WFI at 0x10 and yielded.
    assert_eq!(
        halt,
        HaltReason::IllegalInstruction {
            addr: 0x4000_0020,
            word: 0xFFFF_FFFF
        }
    );
    // 5 blocks executed through real wasmtime (one per lifted
    // instruction); the 6th step halted at decode and never executed.
    assert_eq!(o.steps(), 5);
    assert_eq!(
        o.machine().irq.timer_count,
        5 * u12_orchestrator::TIMER_CYCLES_PER_STEP
    );
    // The block cache actually cached: 5 distinct pcs compiled.
    assert_eq!(o.block_cache_len(), 5);
}

#[test]
fn boot_determinism_same_inputs_same_hash() {
    let img = real_image();
    let run_once = |input: &[u8]| {
        let mut o = Orchestrator::new();
        o.load_image(&img).unwrap();
        o.console_mut().feed_rx(input);
        let halt = o.run_until_halt(1_000);
        (halt, o.state_hash())
    };
    let (halt_a, hash_a) = run_once(b"echo hi\n");
    let (halt_b, hash_b) = run_once(b"echo hi\n");
    assert_eq!(halt_a, halt_b);
    assert_eq!(
        hash_a, hash_b,
        "same image + same inputs must hash identically"
    );
    // Different input → different hash (the hash covers input state).
    let (_, hash_c) = run_once(b"help\n");
    assert_ne!(hash_a, hash_c);
}

#[test]
fn boot_virtio_gpu_submit3d_path() {
    let mut o = Orchestrator::new();

    // --- hand-built virtio ring in guest RAM ---
    // U6 works in ram-slice offsets (PA - RAM_BASE); the queue addresses
    // stored in VirtQueue follow that convention (see queue_notify docs).
    let desc_off: u64 = 0x1_0000;
    let avail_off: u64 = 0x2_0000;
    let used_off: u64 = 0x3_0000;
    let cmd_off: u64 = 0x4_0000;

    // SUBMIT_3D wire bytes: hdr(cmd=0x0207, flags=0, fence=0, ctx=42, pad) +
    // size u32 + pad u32 + 4 payload bytes.
    let mut cmd = Vec::new();
    cmd.extend_from_slice(&0x0207u32.to_le_bytes());
    cmd.extend_from_slice(&0u32.to_le_bytes());
    cmd.extend_from_slice(&0u64.to_le_bytes());
    cmd.extend_from_slice(&42u32.to_le_bytes());
    cmd.extend_from_slice(&0u32.to_le_bytes());
    cmd.extend_from_slice(&4u32.to_le_bytes());
    cmd.extend_from_slice(&0u32.to_le_bytes());
    cmd.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    let w = |o: &mut Orchestrator, off: u64, bytes: &[u8]| {
        let off = off as usize;
        o.machine_mut().ram[off..off + bytes.len()].copy_from_slice(bytes);
    };
    w(&mut o, cmd_off, &cmd);
    // Descriptor 0: addr=cmd_pa, len=cmd.len(), flags=0 (device-readable), next=0.
    let mut desc = Vec::new();
    desc.extend_from_slice(&cmd_off.to_le_bytes());
    desc.extend_from_slice(&(cmd.len() as u32).to_le_bytes());
    desc.extend_from_slice(&0u16.to_le_bytes());
    desc.extend_from_slice(&0u16.to_le_bytes());
    w(&mut o, desc_off, &desc);
    // Avail ring: flags=0, idx=1, ring[0]=0.
    let mut avail = Vec::new();
    avail.extend_from_slice(&0u16.to_le_bytes());
    avail.extend_from_slice(&1u16.to_le_bytes());
    avail.extend_from_slice(&0u16.to_le_bytes());
    w(&mut o, avail_off, &avail);
    // Used ring: flags=0, idx=0 (avail_event left 0).
    w(&mut o, used_off, &[0u8; 8]);

    *o.transport_mut() = TransportState {
        queue_count: 1,
        features: 0,
        status: 4, // DRIVER_OK
        queues: vec![VirtQueue {
            desc_addr: desc_off,
            avail_addr: avail_off,
            used_addr: used_off,
            size: 16,
            ready: true,
            last_avail_idx: 0,
            last_used_idx: 0,
        }],
    };

    let outs = o.queue_notify(0).unwrap();
    // U7 decoded the SUBMIT_3D from the chain's bytes: ctx_id=42.
    assert!(
        outs.iter().any(|d| matches!(
            d,
            DevOut::GpuCommands(cmds)
                if cmds.iter().any(|c| matches!(
                    c,
                    GpuCmd::Submit3D { ctx_id: 42, commands }
                    if commands == &vec![0xDE, 0xAD, 0xBE, 0xEF]
                ))
        )),
        "expected Submit3D ctx=42 in {outs:?}"
    );
    // Used ring committed: idx advanced to 1.
    let used_off = used_off as usize;
    assert_eq!(
        u16::from_le_bytes(
            o.machine().ram[used_off + 2..used_off + 4]
                .try_into()
                .unwrap()
        ),
        1
    );
    // Queue cursors advanced.
    assert_eq!(o.transport().queues[0].last_avail_idx, 1);
    assert_eq!(o.transport().queues[0].last_used_idx, 1);
    // IRQ: need_event_idx(1, event=0, old=0) → 1-0-1 < 1-0 → 0 < 1 → true.
    assert!(
        outs.iter()
            .any(|d| matches!(d, DevOut::IrqAssert { num: 48 })),
        "expected IrqAssert in {outs:?}"
    );
    assert_ne!(o.machine().irq.pending & (1u64 << 48), 0);
    // Second notify: nothing new available → no work, no error.
    let outs2 = o.queue_notify(0).unwrap();
    assert!(outs2.is_empty());
}

// Wave-6 8.1: console RX acceptance — the REAL 4.1 guest command parser
// executes scripted input. Input path under test (nothing faked):
// `ScriptedInput` (u13) -> `Orchestrator::pump_input` -> console RX FIFO
// -> guest `read_char` (real LDRB CONSOLE_RX) -> parser -> TX bytes.

use pathn_contracts::adapters::{KeyCode, NormalizedInput};
use u13_adapters::{KeyboardInput, ScriptedInput};

/// Boot the real image to the first shell prompt (parks at the read-loop WFI).
fn boot_to_prompt() -> Orchestrator {
    let img = real_image();
    let mut o = Orchestrator::new();
    o.load_image(&img).unwrap();
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert_eq!(o.console().tx_bytes, b"pathn-sh> ");
    o
}

/// One `NormalizedInput::Key` press event per byte, as a single poll script.
fn scripted_keys(s: &str) -> ScriptedInput {
    let evs = s
        .bytes()
        .map(|b| NormalizedInput::Key {
            code: KeyCode(b as u32),
            pressed: true,
        })
        .collect();
    ScriptedInput::new(vec![evs])
}

#[test]
fn rx_help_command_prints_help_text() {
    let mut o = boot_to_prompt();
    // Host seam: drain the scripted source into the RX FIFO, then resume.
    o.pump_input(&mut scripted_keys("help\n"));
    let halt = o.run_until_halt(10_000);
    // Guest echoed each byte, ran do_help, re-printed the prompt, parked.
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> help\ncommands: echo <args> | help\npathn-sh> "
    );
}

#[test]
fn rx_echo_command_prints_args() {
    let mut o = boot_to_prompt();
    o.pump_input(&mut scripted_keys("echo hi\n"));
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    // do_echo prints line+5 ("hi") then a newline — from the guest's parser.
    assert_eq!(o.console().tx_bytes, b"pathn-sh> echo hi\nhi\npathn-sh> ");
}

#[test]
fn rx_unknown_command_reports_word() {
    let mut o = boot_to_prompt();
    o.pump_input(&mut scripted_keys("bogus\n"));
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> bogus\nunknown cmd: bogus\npathn-sh> "
    );
}

// ---------------------------------------------------------------------------
// Wave 7 (9.2): DOM key events -> KeyboardInput -> pump_input -> real guest.
//
// Faithful simulated DOM event source: every keystroke is pushed exactly
// as a browser keydown/keyup listener pair would deliver it (physical
// `code` + resolved `key`). Releases are interleaved to prove they
// produce no bytes; an F1 press proves unmapped keys are dropped at the
// boundary. The REAL 4.1 guest parser executes; exact TX bytes pinned.
// ---------------------------------------------------------------------------

/// One DOM key press + release pair, as keydown/keyup listeners deliver.
fn dom_key(kb: &mut KeyboardInput, code: &str, key: &str) {
    kb.push_event(code, key, true); // keydown
    kb.push_event(code, key, false); // keyup: must produce no byte
}

/// Type lowercase ASCII text the browser way: physical KeyX codes with the
/// resolved lowercase `key` character per keystroke.
fn dom_type_lower(kb: &mut KeyboardInput, text: &str) {
    for ch in text.chars() {
        match ch {
            '\n' => dom_key(kb, "Enter", "Enter"),
            ' ' => dom_key(kb, "Space", " "),
            'a'..='z' => {
                let code = format!("Key{}", ch.to_ascii_uppercase());
                dom_key(kb, &code, &ch.to_string());
            }
            _ => panic!("dom_type_lower: no DOM mapping for {ch:?}"),
        }
    }
}

#[test]
fn key_help_command_prints_help_text() {
    let mut o = boot_to_prompt();
    let mut kb = KeyboardInput::new();
    dom_type_lower(&mut kb, "help\n");
    // Unmapped key at the boundary: dropped, disturbs nothing.
    kb.push_event("F1", "F1", true);
    kb.push_event("F1", "F1", false);
    o.pump_input(&mut kb);
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    // The real do_help builtin printed; the F1 press left no trace.
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> help\ncommands: echo <args> | help\npathn-sh> "
    );
}

#[test]
fn key_echo_command_prints_args() {
    let mut o = boot_to_prompt();
    let mut kb = KeyboardInput::new();
    dom_type_lower(&mut kb, "echo hi\n");
    o.pump_input(&mut kb);
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(o.console().tx_bytes, b"pathn-sh> echo hi\nhi\npathn-sh> ");
}

#[test]
fn key_echo_uppercase_bytes_survive() {
    let mut o = boot_to_prompt();
    let mut kb = KeyboardInput::new();
    // Shift-held typing: the browser resolves each key to its shifted
    // character; the adapter never tracks modifiers. The guest matches
    // lowercase builtins byte-exactly, so "ECHO" must arrive verbatim and
    // dispatch to `unknown` — proving byte fidelity, not case folding.
    for (code, key) in [
        ("KeyE", "E"),
        ("KeyC", "C"),
        ("KeyH", "H"),
        ("KeyO", "O"),
        ("Space", " "),
        ("KeyH", "H"),
        ("KeyI", "I"),
        ("Enter", "Enter"),
    ] {
        dom_key(&mut kb, code, key);
    }
    o.pump_input(&mut kb);
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> ECHO HI\nunknown cmd: ECHO\npathn-sh> "
    );
}

#[test]
fn key_unknown_command_reports_word() {
    let mut o = boot_to_prompt();
    let mut kb = KeyboardInput::new();
    dom_type_lower(&mut kb, "bogus\n");
    o.pump_input(&mut kb);
    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0070 });
    assert!(
        o.console().rx_queue.is_empty(),
        "guest consumed every input byte"
    );
    assert_eq!(
        o.console().tx_bytes,
        b"pathn-sh> bogus\nunknown cmd: bogus\npathn-sh> "
    );
}
