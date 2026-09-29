//! Track A: guest→host GPU command-stream port tests.
//!
//! What these tests prove, for real:
//! - `gpu_port_push_submit_take`: byte accumulation + submit signalling +
//!   take semantics (take clears buffer and flag; second take → None).
//! - `drain_gpu_submit_none_when_idle`: no submit → None, no panic.
//! - `drain_gpu_submit_decodes_submit3d`: pre-loaded well-formed SUBMIT_3D
//!   stream → U7 decodes one `GpuCmd::Submit3D` (ctx 42), no errors.
//! - `drain_gpu_submit_surfaces_decode_errors`: garbage bytes → decode
//!   errors surfaced in `decode_errors`, never silently dropped.
//! - `guest_mmio_gpu_stream_roundtrip`: a REAL AArch64 guest (assembled with
//!   the real `guest-image` encoders) STRBs a well-formed 32-byte SUBMIT_3D
//!   stream to the GPU MMIO port through the REAL `WasmHost::mem_store`
//!   dispatch, signals submit, parks at WFI — then `drain_gpu_submit`
//!   decodes it to `GpuCmd::Submit3D { ctx_id: 42 }`.

use guest_image::asm::{
    enc_add_imm, enc_adrp, enc_b, enc_cbz, enc_ldrb, enc_movz, enc_orr_shift, enc_strb, enc_wfi,
};
use pathn_contracts::device::GpuCmd;
use u12_orchestrator::{HaltReason, Orchestrator, GPU_DATA, GPU_SIZE, GPU_SUBMIT, RAM_BASE};

/// Well-formed SUBMIT_3D wire bytes: 24-byte ctrl hdr
/// (type=0x0207, flags=0, fence=0, ctx=42, pad=0) + size u32 + pad u32
/// + empty opcode stream (size=0).
fn submit3d_stream() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&0x0207u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u64.to_le_bytes());
    v.extend_from_slice(&42u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes()); // size = 0
    v.extend_from_slice(&0u32.to_le_bytes()); // pad
    v
}

#[test]
fn gpu_port_push_submit_take() {
    let mut o = Orchestrator::new();
    assert!(!o.gpu_port().is_pending());
    assert_eq!(o.gpu_port().buffered_len(), 0);

    for b in [0xDE, 0xAD, 0xBE, 0xEF] {
        o.gpu_port.push_byte(b);
    }
    assert_eq!(o.gpu_port().buffered_len(), 4);
    // Not submitted yet: take → None, bytes retained.
    assert!(o.gpu_port.take_submit().is_none());
    assert_eq!(o.gpu_port().buffered_len(), 4);

    o.gpu_port.submit();
    assert!(o.gpu_port().is_pending());
    let taken = o.gpu_port.take_submit().expect("pending submit");
    assert_eq!(taken, vec![0xDE, 0xAD, 0xBE, 0xEF]);

    // Take clears everything: flag and buffer.
    assert!(!o.gpu_port().is_pending());
    assert_eq!(o.gpu_port().buffered_len(), 0);
    assert!(o.gpu_port.take_submit().is_none());
}

#[test]
fn gpu_port_submit_empty_stream_is_not_dropped() {
    let mut o = Orchestrator::new();
    o.gpu_port.submit();
    let taken = o.gpu_port.take_submit().expect("empty submit is pending");
    assert!(taken.is_empty());
}

#[test]
fn drain_gpu_submit_none_when_idle() {
    let mut o = Orchestrator::new();
    assert!(o.drain_gpu_submit().is_none());
}

#[test]
fn drain_gpu_submit_decodes_submit3d() {
    let mut o = Orchestrator::new();
    for b in submit3d_stream() {
        o.gpu_port.push_byte(b);
    }
    o.gpu_port.submit();

    let sub = o.drain_gpu_submit().expect("pending submit");
    assert!(sub.decode_errors.is_empty());
    assert_eq!(sub.commands.len(), 1);
    match &sub.commands[0] {
        GpuCmd::Submit3D { ctx_id, commands } => {
            assert_eq!(*ctx_id, 42);
            assert!(commands.is_empty());
        }
        other => panic!("expected Submit3D, got {other:?}"),
    }
    // Drained: second call → None.
    assert!(o.drain_gpu_submit().is_none());
}

#[test]
fn drain_gpu_submit_surfaces_decode_errors() {
    let mut o = Orchestrator::new();
    // Truncated header: U7 must signal, not swallow.
    for b in [0x07, 0x02] {
        o.gpu_port.push_byte(b);
    }
    o.gpu_port.submit();

    let sub = o.drain_gpu_submit().expect("pending submit");
    assert!(sub.commands.is_empty());
    assert!(
        !sub.decode_errors.is_empty(),
        "decode failure must be surfaced, never silently dropped"
    );
}

/// Assemble a real guest that writes the 32-byte SUBMIT_3D stream to the GPU
/// MMIO port one STRB at a time, then signals submit and parks at WFI.
///
/// Layout (all offsets in bytes from RAM base):
/// ```text
/// 0x00: MOVZ X0, #0xA00, LSL #16   ; x0 = GPU_DATA
/// 0x04: ADRP X1, #0
/// 0x08: ADD  X1, X1, #0x3C          ; x1 = stream bytes
/// 0x0C: MOVZ X2, #0                 ; count = 0
/// loop @0x10:
/// 0x10: LDRB W3, [X1]
/// 0x14: STRB W3, [X0]               ; GPU_DATA = byte
/// 0x18: ADD  X1, X1, #1
/// 0x1C: ADD  X2, X2, #1
/// 0x20: ADD  X3, X2, #224           ; eq-trick: count == 32 ?
/// 0x24: ORR  X3, XZR, X3, LSL #56
/// 0x28: CBZ  X3, done               ; taken iff count == 32
/// 0x2C: B    loop
/// done @0x30:
/// 0x30: ADD  X0, X0, #8             ; x0 = GPU_SUBMIT
/// 0x34: STRB WZR, [X0]              ; submit (value ignored)
/// 0x38: WFI
/// data @0x3C: 32 stream bytes
/// ```
/// The byte-equality trick is the same one `guest-image`'s shell uses
/// (`asm.rs` docs): for count in [0,255], `(count + 224) << 56 == 0`
/// (mod 2^64) iff `count + 224 == 256` iff `count == 32`.
#[test]
fn guest_mmio_gpu_stream_roundtrip() {
    let stream = submit3d_stream();
    assert_eq!(stream.len(), 32);

    let data_off: u16 = 0x3C;
    let words: Vec<u32> = vec![
        enc_movz(0, 0xA00, 1),
        enc_adrp(1, 0),
        enc_add_imm(1, 1, data_off, false),
        enc_movz(2, 0, 0),
        // loop @0x10
        enc_ldrb(3, 1, 0),
        enc_strb(3, 0, 0),
        enc_add_imm(1, 1, 1, false),
        enc_add_imm(2, 2, 1, false),
        enc_add_imm(3, 2, 224, false),
        enc_orr_shift(3, 31, 3, 0, 56),
        enc_cbz(3, 2), // 0x28 -> done @0x30: (0x30-0x28)/4 = 2
        enc_b(-7),     // 0x2C -> loop @0x10: (0x10-0x2C)/4 = -7
        // done @0x30
        enc_add_imm(0, 0, 8, false),
        enc_strb(31, 0, 0),
        enc_wfi(),
    ];
    assert_eq!(words.len() * 4, data_off as usize);

    let mut o = Orchestrator::new();
    for (i, w) in words.iter().enumerate() {
        let off = i * 4;
        o.machine_mut().ram[off..off + 4].copy_from_slice(&w.to_le_bytes());
    }
    let doff = data_off as usize;
    o.machine_mut().ram[doff..doff + stream.len()].copy_from_slice(&stream);

    let halt = o.run_until_halt(10_000);
    assert_eq!(halt, HaltReason::Wfi { addr: 0x4000_0038 });

    // The guest really STRB'd 32 bytes to GPU_DATA through mem_store…
    assert!(o.gpu_port().is_pending());
    assert_eq!(o.gpu_port().buffered_len(), 32);
    // …and the unknown MMIO bytes never touched RAM or the console.
    assert!(o.console().tx_bytes.is_empty());

    let sub = o.drain_gpu_submit().expect("pending submit");
    assert!(sub.decode_errors.is_empty());
    assert_eq!(sub.commands.len(), 1);
    match &sub.commands[0] {
        GpuCmd::Submit3D { ctx_id, commands } => {
            assert_eq!(*ctx_id, 42);
            assert!(commands.is_empty());
        }
        other => panic!("expected Submit3D, got {other:?}"),
    }
}

#[test]
fn gpu_mmio_constants_match_platform_map() {
    // The port lives at 0x0A00_0000, one page, outside RAM and clear of the
    // console page at 0x0900_0000.
    assert_eq!(GPU_DATA, 0x0A00_0000);
    assert_eq!(GPU_SUBMIT, 0x0A00_0008);
    assert_eq!(GPU_SIZE, 0x1000);
    assert!(GPU_DATA >= 0x0A00_0000 && GPU_DATA < 0x0A00_0000 + GPU_SIZE);
    assert!(GPU_SUBMIT >= 0x0A00_0000 && GPU_SUBMIT < 0x0A00_0000 + GPU_SIZE);
}

/// End to end through the REAL `pathn-sh` guest: feed `triangle\n` on the
/// console RX, run until the shell parks at WFI, then drain the GPU port.
/// The guest's own `do_triangle` streams the 104-byte SUBMIT_3D packet via
/// real STRBs; U7 must decode exactly one `Submit3D` with the 72-byte opcode
/// stream intact.
#[test]
fn shell_triangle_command_streams_submit3d() {
    use guest_image::image::{build, GuestManifest};

    let manifest = GuestManifest {
        name: "pathn-sh".to_string(),
        version: 1,
        load_addr: RAM_BASE,
    };
    let img = build(&manifest).0;
    let mut o = Orchestrator::new();
    o.load_image(&img).unwrap();
    o.console_mut().feed_rx(b"triangle\n");

    let halt = o.run_until_halt(100_000);
    assert!(
        matches!(halt, HaltReason::Wfi { .. }),
        "shell must park at WFI, got {halt:?}"
    );

    // The shell echoed the line and printed its confirmation.
    let tx = String::from_utf8_lossy(&o.console().tx_bytes);
    assert!(
        tx.contains("gpu: triangle submitted\n"),
        "confirmation missing; tx = {tx:?}"
    );

    // The port holds exactly the 104-byte packet; drain decodes it.
    assert!(o.gpu_port().is_pending());
    assert_eq!(o.gpu_port().buffered_len(), 104);
    let sub = o.drain_gpu_submit().expect("pending submit");
    assert!(sub.decode_errors.is_empty());
    assert_eq!(sub.commands.len(), 1);
    match &sub.commands[0] {
        GpuCmd::Submit3D { ctx_id, commands } => {
            assert_eq!(*ctx_id, 42);
            // 72-byte opcode stream: VIEWPORT + CLEAR + DRAW_ARRAYS.
            assert_eq!(commands.len(), 72);
            assert_eq!(&commands[0..4], &0x04u32.to_le_bytes()); // VIEWPORT
            assert_eq!(&commands[24..28], &0x01u32.to_le_bytes()); // CLEAR
            assert_eq!(&commands[52..56], &0x02u32.to_le_bytes()); // DRAW_ARRAYS
        }
        other => panic!("expected Submit3D, got {other:?}"),
    }
}
