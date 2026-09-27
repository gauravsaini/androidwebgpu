//! Browser-adapter contracts — the quarantined externals boundary (LLD U13).
//!
//! Added 2026-09-27 (Wave 3 amendment U13-G1, driver-approved): the four adapter
//! traits plus their value types were first defined in `u13-adapters`, but the
//! orchestrator (U12) may only touch units through frozen contracts (LLD U12),
//! so the traits live here. `u13-adapters` provides the mock implementations;
//! the real WebGPU/WebSocket/IndexedDB bindings (Wave 4, wasm-bindgen) implement
//! these same traits. Traits deliberately carry no `Send` bound: real wasm
//! impls are `!Send`.
//!
//! All types are pure data + pure constructors; no I/O, no threads.

/// One presented video frame: RGBA8 pixels, row-major, top-left first.
///
/// `rgba.len()` must equal `width * height * 4`; [`Frame::new`] enforces this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// `Frame::new` failure: the pixel buffer does not match the dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameError {
    pub expected: usize,
    pub got: usize,
}

impl Frame {
    /// Build a frame, validating the pixel buffer length.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, FrameError> {
        let expected = width as usize * height as usize * 4;
        if rgba.len() == expected {
            Ok(Self {
                width,
                height,
                rgba,
            })
        } else {
            Err(FrameError {
                expected,
                got: rgba.len(),
            })
        }
    }
}

/// Stable key identifier produced by input normalization.
///
/// Values follow the Linux input-event code space (`KEY_*`), so the guest
/// input driver consumes them without a second translation table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u32);

/// A sanitized guest input event. Raw DOM events never cross this boundary —
/// the real Wave-4 impl normalizes `KeyboardEvent`/`PointerEvent` into these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedInput {
    Key {
        code: KeyCode,
        pressed: bool,
    },
    PointerMove {
        x: i32,
        y: i32,
    },
    PointerButton {
        button: u8,
        pressed: bool,
        x: i32,
        y: i32,
    },
    Scroll {
        dx: i32,
        dy: i32,
    },
}

/// WebGPU canvas surface. The real impl uploads `frame.rgba` to a texture and
/// presents it; side effects stay behind this trait.
pub trait GpuSurface {
    fn present(&mut self, frame: &Frame);
}

/// Byte-stream socket (virtio-net / adb). Datagram boundaries are preserved:
/// each `send` is one datagram; `recv` returns `None` only when no datagram
/// is available — `Some(vec![])` is a real empty datagram.
pub trait NetSocket {
    fn send(&mut self, data: &[u8]);
    fn recv(&mut self) -> Option<Vec<u8>>;
}

/// Sanitized input event source. `poll` drains all events queued since the
/// last call; an empty `Vec` means "no input this tick".
pub trait InputSource {
    fn poll(&mut self) -> Vec<NormalizedInput>;
}

/// Persistent key/value blob storage (snapshot persist for the orchestrator).
pub trait BlobStore {
    fn save(&mut self, key: &str, data: &[u8]);
    fn load(&self, key: &str) -> Option<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_new_rejects_bad_length() {
        assert!(Frame::new(2, 2, vec![0u8; 15]).is_err());
        assert!(Frame::new(2, 2, vec![0u8; 16]).is_ok());
    }

    #[test]
    fn keycode_is_transparent_newtype() {
        assert_eq!(KeyCode(30).0, 30);
    }
}
