//! U7 — virtio-gpu device model (`EXPLICIT-STATE`).
//!
//! Responsibility (LLD §7): a virtio-gpu control-command stream (as handed to
//! the device on `QueueNotify`) is decoded into typed [`GpuCmd`] events for the
//! GPU half (U8). All state is explicit in the signature; there is no hidden
//! state, no I/O, no clock, no threads.
//!
//! ## Contract flip vs Path E
//!
//! Path E rejected `SUBMIT_3D`. Path N **accepts** it: the 3D command payload
//! is parsed honestly out of the command buffer and emitted as
//! [`GpuCmd::Submit3D`] — opaque typed bytes, never dropped, never rejected.
//!
//! ## Wire format
//!
//! Commands follow the Linux `virtio_gpu.h` layout (little-endian):
//!
//! ```text
//! struct virtio_gpu_ctrl_hdr {   // 24 bytes
//!     u32 type; u32 flags; u64 fence_id; u32 ctx_id; u32 padding;
//! };
//! ```
//!
//! followed by a type-specific payload. Decoded command types:
//!
//! | id     | name                    | payload after header              | GpuCmd            |
//! |--------|-------------------------|-------------------------------------|-------------------|
//! | 0x0101 | RESOURCE_CREATE_2D      | id,fmt,w,h (16 B)                 | state update only |
//! | 0x0102 | RESOURCE_UNREF          | id,pad (8 B)                      | known no-op       |
//! | 0x0103 | SET_SCANOUT             | rect(16),scanout_id,res_id (24 B) | Scanout           |
//! | 0x0104 | RESOURCE_FLUSH          | rect(16),res_id,pad (24 B)        | known no-op       |
//! | 0x0105 | TRANSFER_TO_HOST_2D     | rect(16),offset(8),res,pad (32 B) | Transfer2D        |
//! | 0x0106 | RESOURCE_ATTACH_BACKING | res_id,nr_entries + entries       | known no-op       |
//! | 0x0207 | SUBMIT_3D               | size,pad (8 B) + size bytes       | Submit3D (accepted)|
//!
//! Any command carrying `VIRTIO_GPU_FLAG_FENCE` additionally emits
//! [`GpuCmd::Fence`] **after** the command's own output.
//!
//! ## Error signalling
//!
//! The frozen [`DevOut`] enum has no dedicated error variant, so decode
//! failures are reported explicitly (never panicked, never silently dropped)
//! as `DevOut::ConfigValue` with a class tag in the high 32 bits:
//!
//! - unknown/unsupported command id: `ERR_UNKNOWN_COMMAND | cmd_id`
//! - truncated stream: `ERR_TRUNCATED | needed_len` (low 32 bits)
//!
//! Decoding stops at the first error; commands decoded before it are still
//! emitted.

#![forbid(unsafe_code)]

use pathn_contracts::device::{DevEvent, DevOut, GpuCmd, GpuDevState};

// ---------------------------------------------------------------------------
// Wire constants (Linux virtio_gpu.h)
// ---------------------------------------------------------------------------

const HDR_LEN: u64 = 24;
const VIRTIO_GPU_FLAG_FENCE: u32 = 1 << 0;

const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
const CMD_RESOURCE_UNREF: u32 = 0x0102;
const CMD_SET_SCANOUT: u32 = 0x0103;
const CMD_RESOURCE_FLUSH: u32 = 0x0104;
const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const CMD_SUBMIT_3D: u32 = 0x0207;

// ---------------------------------------------------------------------------
// Error signalling (see module docs)
// ---------------------------------------------------------------------------

/// Class tag for "unknown/unsupported virtio-gpu command id" errors.
/// Low 32 bits carry the offending command id.
pub const ERR_UNKNOWN_COMMAND: u64 = 0x4750_5500_0000_0000; // "GPU\0"
/// Class tag for "command stream truncated" errors.
/// Low 32 bits carry the total bytes needed.
pub const ERR_TRUNCATED: u64 = 0x4750_5501_0000_0000;

/// Explicit decode failure. Returned internally and mapped to a
/// [`DevOut::ConfigValue`] error signal by [`step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Stream ended before `need` total bytes were available (`have` present).
    Truncated { need: u64, have: u64 },
    /// Command id is not one this device model decodes.
    UnknownCommand(u32),
}

fn error_out(err: &DecodeError) -> DevOut {
    match err {
        DecodeError::UnknownCommand(id) => {
            DevOut::ConfigValue(ERR_UNKNOWN_COMMAND | u64::from(*id))
        }
        DecodeError::Truncated { need, .. } => {
            DevOut::ConfigValue(ERR_TRUNCATED | (need & 0xFFFF_FFFF))
        }
    }
}

// ---------------------------------------------------------------------------
// Config space (virtio-gpu: events_read, events_clear, num_scanouts, num_capsets)
// ---------------------------------------------------------------------------

fn config_read(offset: u64) -> u64 {
    match offset {
        0 => 0,  // events_read: no pending events
        8 => 1,  // num_scanouts: exactly one scanout
        12 => 0, // num_capsets: none
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Little-endian field readers (callers bounds-check first)
// ---------------------------------------------------------------------------

fn le32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn le64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

// ---------------------------------------------------------------------------
// Stream decoder
// ---------------------------------------------------------------------------

/// Payload length (bytes after the 24-byte header) for a command type.
/// Validates the claimed length against the remaining buffer; variable-length
/// commands (`ATTACH_BACKING`, `SUBMIT_3D`) read their length fields with
/// overflow-safe arithmetic.
fn payload_len(cmd_type: u32, rest: &[u8]) -> Result<u64, DecodeError> {
    let have = rest.len() as u64;
    let fixed = |n: u64| -> Result<u64, DecodeError> {
        if have < n {
            Err(DecodeError::Truncated {
                need: HDR_LEN + n,
                have: HDR_LEN + have,
            })
        } else {
            Ok(n)
        }
    };
    match cmd_type {
        CMD_RESOURCE_CREATE_2D => fixed(16),
        CMD_RESOURCE_UNREF => fixed(8),
        CMD_SET_SCANOUT => fixed(24),
        CMD_RESOURCE_FLUSH => fixed(24),
        CMD_TRANSFER_TO_HOST_2D => fixed(32),
        CMD_RESOURCE_ATTACH_BACKING => {
            if have < 8 {
                return Err(DecodeError::Truncated {
                    need: HDR_LEN + 8,
                    have: HDR_LEN + have,
                });
            }
            let nr = u64::from(le32(rest, 4));
            // 8 + 16*nr computed in u64: no overflow possible for u32 nr.
            let n = 8u64.saturating_add(nr.saturating_mul(16));
            if have < n {
                Err(DecodeError::Truncated {
                    need: HDR_LEN + n,
                    have: HDR_LEN + have,
                })
            } else {
                Ok(n)
            }
        }
        CMD_SUBMIT_3D => {
            if have < 8 {
                return Err(DecodeError::Truncated {
                    need: HDR_LEN + 8,
                    have: HDR_LEN + have,
                });
            }
            let size = u64::from(le32(rest, 0));
            let n = 8u64.saturating_add(size);
            if have < n {
                Err(DecodeError::Truncated {
                    need: HDR_LEN + n,
                    have: HDR_LEN + have,
                })
            } else {
                Ok(n)
            }
        }
        other => Err(DecodeError::UnknownCommand(other)),
    }
}

/// Decode one command body (header already consumed, `body` is exactly the
/// payload). Appends to `out`; updates explicit `state` for resource/fence ids.
fn decode_one(
    cmd_type: u32,
    ctx_id: u32,
    body: &[u8],
    state: &mut GpuDevState,
    out: &mut Vec<GpuCmd>,
) {
    match cmd_type {
        CMD_RESOURCE_CREATE_2D => {
            let resource_id = le32(body, 0);
            state.next_resource_id = state.next_resource_id.max(resource_id.saturating_add(1));
        }
        CMD_RESOURCE_UNREF | CMD_RESOURCE_FLUSH | CMD_RESOURCE_ATTACH_BACKING => {
            // Known commands with no typed GpuCmd consumer: the device
            // acknowledges them by consuming their bytes; nothing is emitted.
        }
        CMD_SET_SCANOUT => {
            let w = le32(body, 8);
            let h = le32(body, 12);
            let resource_id = le32(body, 20);
            out.push(GpuCmd::Scanout { resource_id, w, h });
        }
        CMD_TRANSFER_TO_HOST_2D => {
            let x = le32(body, 0);
            let y = le32(body, 4);
            let w = le32(body, 8);
            let h = le32(body, 12);
            let resource_id = le32(body, 24);
            // The pixel bytes live in guest backing pages, not in the control
            // stream, so `data` is empty here by construction (not dropped).
            out.push(GpuCmd::Transfer2D {
                resource_id,
                x,
                y,
                w,
                h,
                data: Vec::new(),
            });
        }
        CMD_SUBMIT_3D => {
            let size = le32(body, 0) as usize;
            // Accepted as typed data: the 3D stream is opaque to this device
            // model and forwarded verbatim to the GPU half (U8).
            out.push(GpuCmd::Submit3D {
                ctx_id,
                commands: body[8..8 + size].to_vec(),
            });
        }
        _ => {
            // Unreachable: payload_len rejects unknown types first.
        }
    }
}

/// Decode a full command stream. Returns the typed commands and, on failure,
/// the first error encountered (decoding stops there).
fn decode_stream(state: &mut GpuDevState, buf: &[u8]) -> (Vec<GpuCmd>, Option<DecodeError>) {
    let mut out = Vec::new();
    let mut pos: usize = 0;
    loop {
        if pos == buf.len() {
            return (out, None);
        }
        if buf.len() - pos < HDR_LEN as usize {
            return (
                out,
                Some(DecodeError::Truncated {
                    need: pos as u64 + HDR_LEN,
                    have: buf.len() as u64,
                }),
            );
        }
        let cmd_type = le32(buf, pos);
        let flags = le32(buf, pos + 4);
        let fence_id = le64(buf, pos + 8);
        let ctx_id = le32(buf, pos + 16);
        pos += HDR_LEN as usize;
        let plen = match payload_len(cmd_type, &buf[pos..]) {
            Ok(n) => n as usize,
            Err(e) => return (out, Some(e)),
        };
        decode_one(cmd_type, ctx_id, &buf[pos..pos + plen], state, &mut out);
        pos += plen;
        if flags & VIRTIO_GPU_FLAG_FENCE != 0 {
            out.push(GpuCmd::Fence { id: fence_id });
            state.next_fence_id = state.next_fence_id.max(fence_id.saturating_add(1));
        }
    }
}

// ---------------------------------------------------------------------------
// Public step function (LLD §7)
// ---------------------------------------------------------------------------

/// Advance the virtio-gpu device model by one event.
///
/// - `QueueNotify` — decode the command stream in `ram`, emit
///   `DevOut::GpuCommands`; decode failures surface as explicit
///   `DevOut::ConfigValue` error signals (never panic, never silent).
/// - `ConfigRead` — return the config-space value.
/// - `ConfigWrite` — acknowledged, no output.
/// - `Reset` — ids return to their initial values.
pub fn step(state: &GpuDevState, ev: DevEvent, ram: &[u8]) -> (GpuDevState, Vec<DevOut>) {
    let mut next = state.clone();
    match ev {
        DevEvent::QueueNotify { .. } => {
            let (cmds, err) = decode_stream(&mut next, ram);
            let mut outs = vec![DevOut::GpuCommands(cmds)];
            if let Some(e) = err {
                outs.push(error_out(&e));
            }
            (next, outs)
        }
        DevEvent::ConfigRead { offset } => (next, vec![DevOut::ConfigValue(config_read(offset))]),
        DevEvent::ConfigWrite { .. } => (next, Vec::new()),
        DevEvent::Reset => (
            GpuDevState {
                next_resource_id: 0,
                next_fence_id: 0,
            },
            Vec::new(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests — real, named, hand-built command buffers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(cmd_type: u32, flags: u32, fence_id: u64, ctx_id: u32) -> Vec<u8> {
        let mut b = Vec::with_capacity(24);
        b.extend_from_slice(&cmd_type.to_le_bytes());
        b.extend_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(&fence_id.to_le_bytes());
        b.extend_from_slice(&ctx_id.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes()); // header padding
        b
    }

    fn initial_state() -> GpuDevState {
        GpuDevState {
            next_resource_id: 0,
            next_fence_id: 0,
        }
    }

    fn notify() -> DevEvent {
        DevEvent::QueueNotify { queue_idx: 0 }
    }

    #[test]
    fn submit3d_wellformed_decodes_exact_fields() {
        let mut buf = hdr(CMD_SUBMIT_3D, 0, 0, 5);
        buf.extend_from_slice(&4u32.to_le_bytes()); // size
        buf.extend_from_slice(&0u32.to_le_bytes()); // padding
        buf.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let (next, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![DevOut::GpuCommands(vec![GpuCmd::Submit3D {
                ctx_id: 5,
                commands: vec![0xDE, 0xAD, 0xBE, 0xEF],
            }])]
        );
        // No fence involved: ids untouched.
        assert_eq!(next.next_fence_id, 0);
        assert_eq!(next.next_resource_id, 0);
    }

    #[test]
    fn submit3d_truncated_size_overruns_buffer() {
        let mut buf = hdr(CMD_SUBMIT_3D, 0, 0, 1);
        buf.extend_from_slice(&64u32.to_le_bytes()); // size claims 64 bytes
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[1, 2, 3]); // only 3 present
        let (next, outs) = step(&initial_state(), notify(), &buf);
        // need = 24 (hdr) + 8 + 64 = 96; graceful, no panic.
        assert_eq!(
            outs,
            vec![
                DevOut::GpuCommands(vec![]),
                DevOut::ConfigValue(ERR_TRUNCATED | 96),
            ]
        );
        assert_eq!(next, initial_state());
    }

    #[test]
    fn submit3d_with_fence_flag_emits_fence_after() {
        let mut buf = hdr(CMD_SUBMIT_3D, VIRTIO_GPU_FLAG_FENCE, 7, 2);
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[9, 9]);
        let (next, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![DevOut::GpuCommands(vec![
                GpuCmd::Submit3D {
                    ctx_id: 2,
                    commands: vec![9, 9],
                },
                GpuCmd::Fence { id: 7 },
            ])]
        );
        assert_eq!(next.next_fence_id, 8);
    }

    #[test]
    fn submit3d_empty_payload_is_accepted() {
        let mut buf = hdr(CMD_SUBMIT_3D, 0, 0, 0);
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let (_, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![DevOut::GpuCommands(vec![GpuCmd::Submit3D {
                ctx_id: 0,
                commands: vec![],
            }])]
        );
    }

    #[test]
    fn unknown_command_id_yields_named_error() {
        let buf = hdr(0xDEAD, 0, 0, 0);
        let (next, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![
                DevOut::GpuCommands(vec![]),
                DevOut::ConfigValue(ERR_UNKNOWN_COMMAND | 0xDEAD),
            ]
        );
        // The offending id is named in the signal; state is untouched.
        assert_eq!(next, initial_state());
    }

    #[test]
    fn truncated_header_is_graceful() {
        let buf = [1u8, 2, 3]; // not even a full header
        let (_, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![
                DevOut::GpuCommands(vec![]),
                DevOut::ConfigValue(ERR_TRUNCATED | 24),
            ]
        );
    }

    #[test]
    fn attach_backing_huge_entry_count_does_not_overflow() {
        let mut buf = hdr(CMD_RESOURCE_ATTACH_BACKING, 0, 0, 0);
        buf.extend_from_slice(&1u32.to_le_bytes()); // resource_id
        buf.extend_from_slice(&u32::MAX.to_le_bytes()); // nr_entries: absurd
        let (_, outs) = step(&initial_state(), notify(), &buf);
        // u64 length math (8 + 16*nr) must not wrap; must error cleanly.
        assert_eq!(outs.len(), 2);
        match &outs[1] {
            DevOut::ConfigValue(v) => {
                assert_eq!(v & 0xFFFF_FFFF_0000_0000, ERR_TRUNCATED);
            }
            other => panic!("expected truncation signal, got {other:?}"),
        }
    }

    #[test]
    fn transfer_to_host_2d_decodes_rect() {
        let mut buf = hdr(CMD_TRANSFER_TO_HOST_2D, 0, 0, 0);
        for v in [10u32, 20, 30, 40] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&0u64.to_le_bytes()); // offset
        buf.extend_from_slice(&3u32.to_le_bytes()); // resource_id
        buf.extend_from_slice(&0u32.to_le_bytes()); // padding
        let (_, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![DevOut::GpuCommands(vec![GpuCmd::Transfer2D {
                resource_id: 3,
                x: 10,
                y: 20,
                w: 30,
                h: 40,
                data: vec![],
            }])]
        );
    }

    #[test]
    fn set_scanout_decodes_resource_and_size() {
        let mut buf = hdr(CMD_SET_SCANOUT, 0, 0, 0);
        for v in [0u32, 0, 800, 600] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&0u32.to_le_bytes()); // scanout_id
        buf.extend_from_slice(&7u32.to_le_bytes()); // resource_id
        let (_, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(
            outs,
            vec![DevOut::GpuCommands(vec![GpuCmd::Scanout {
                resource_id: 7,
                w: 800,
                h: 600,
            }])]
        );
    }

    #[test]
    fn create_2d_advances_next_resource_id() {
        let mut buf = hdr(CMD_RESOURCE_CREATE_2D, 0, 0, 0);
        for v in [9u32, 1, 64, 64] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        let state = GpuDevState {
            next_resource_id: 5,
            next_fence_id: 0,
        };
        let (next, outs) = step(&state, notify(), &buf);
        assert_eq!(next.next_resource_id, 10);
        assert_eq!(next.next_fence_id, 0);
        assert_eq!(outs, vec![DevOut::GpuCommands(vec![])]);
    }

    #[test]
    fn mixed_stream_decodes_in_order_then_errors() {
        let mut buf = hdr(CMD_RESOURCE_CREATE_2D, 0, 0, 0);
        for v in [2u32, 1, 16, 16] {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&hdr(0xBEEF, 0, 0, 0)); // unknown id follows
        let (next, outs) = step(&initial_state(), notify(), &buf);
        assert_eq!(next.next_resource_id, 3);
        assert_eq!(
            outs,
            vec![
                DevOut::GpuCommands(vec![]),
                DevOut::ConfigValue(ERR_UNKNOWN_COMMAND | 0xBEEF),
            ]
        );
    }

    #[test]
    fn queue_notify_empty_stream_emits_empty_commands() {
        let (next, outs) = step(&initial_state(), notify(), &[]);
        assert_eq!(outs, vec![DevOut::GpuCommands(vec![])]);
        assert_eq!(next, initial_state());
    }

    #[test]
    fn reset_restores_initial_ids() {
        let state = GpuDevState {
            next_resource_id: 42,
            next_fence_id: 7,
        };
        let (next, outs) = step(&state, DevEvent::Reset, &[]);
        assert_eq!(next, initial_state());
        assert!(outs.is_empty());
    }

    #[test]
    fn config_read_returns_scanout_count() {
        let (next, outs) = step(&initial_state(), DevEvent::ConfigRead { offset: 8 }, &[]);
        assert_eq!(outs, vec![DevOut::ConfigValue(1)]);
        assert_eq!(next, initial_state());
    }

    #[test]
    fn config_write_is_acknowledged_silently() {
        let (next, outs) = step(
            &initial_state(),
            DevEvent::ConfigWrite {
                offset: 4,
                value: 0,
            },
            &[],
        );
        assert!(outs.is_empty());
        assert_eq!(next, initial_state());
    }

    #[test]
    fn determinism_same_input_same_output() {
        let mut buf = hdr(CMD_SUBMIT_3D, VIRTIO_GPU_FLAG_FENCE, 3, 1);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.push(0xAA);
        let a = step(&initial_state(), notify(), &buf);
        let b = step(&initial_state(), notify(), &buf);
        assert_eq!(a, b);
    }
}
