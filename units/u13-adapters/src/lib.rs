//! U13 — browser adapters: the quarantined externals boundary.
//!
//! Every side effect the emulator needs (presenting a frame, network I/O,
//! input events, persistent storage) lives behind the four traits in this
//! module. The pure emulation units never touch the outside world; the
//! orchestrator (Wave 4, U12) will drive these traits. That is the quarantine:
//! all impurity is behind this one narrow, owned-data API surface.
//!
//! ## Honest bounds
//!
//! This crate contains **no real browser bindings**. The real implementations —
//! WebGPU canvas via `web-sys`/`wgpu`, WebSocket, `KeyboardEvent` listeners,
//! IndexedDB via `wasm-bindgen` JS glue — are Wave-4 product work. What this
//! crate provides is the trait contract those real impls will implement, plus
//! in-memory mocks so every consumer can be tested without a browser.
//!
//! The traits are designed wasm-bindgen-friendly: owned data crosses every
//! call (`&Frame`, `&[u8]`, `Vec<u8>`, `&str` — never borrowed handles that
//! outlive the call, never `dyn` across the FFI), so the real impls slot in
//! later without changing the emulation side.
//!
//! ## Where the traits live
//!
//! The four adapter traits plus `Frame`, `FrameError`, `KeyCode`, and
//! `NormalizedInput` live in `pathn_contracts::adapters` (Wave-3 amendment
//! U13-G1, driver-approved 2026-09-27): the orchestrator (U12) may only touch
//! units through frozen contracts (LLD U12), never a unit crate. This crate
//! provides the in-memory mocks + the key-normalization table; the real
//! Wave-4 browser bindings implement the same contract traits.
//!
//! ## Threading
//!
//! The traits deliberately have **no `Send` bound**: real wasm-bindgen
//! implementations are single-threaded (`!Send`) and must still implement
//! these traits. The mocks are in-memory only — no `std::net`, no `std::fs`,
//! no threads, ever (enforced by the `conform_quarantine_no_os_io`
//! source-scan test). `LoopbackSocket` is intentionally `!Send` (shared
//! in-memory queue via `Rc<RefCell<..>>`); that is documented, not a defect.

use pathn_contracts::adapters::{
    BlobStore, Frame, GpuSurface, InputSource, KeyCode, NetSocket, NormalizedInput,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Input normalization: KeyboardEvent.code -> NormalizedInput
//
// The 100-entry table maps the DOM `KeyboardEvent.code` space to Linux
// input-event codes (KEY_*), so the guest input driver consumes KeyCode
// values without a second translation table. Unknown codes -> None: raw
// junk is dropped at the boundary, never forwarded.
// ---------------------------------------------------------------------------

/// DOM `KeyboardEvent.code` -> Linux `KEY_*` code. Standard
/// `input-event-codes.h` values.
const KEY_TABLE: &[(&str, u32)] = &[
    ("Escape", 1),
    ("Digit1", 2),
    ("Digit2", 3),
    ("Digit3", 4),
    ("Digit4", 5),
    ("Digit5", 6),
    ("Digit6", 7),
    ("Digit7", 8),
    ("Digit8", 9),
    ("Digit9", 10),
    ("Digit0", 11),
    ("Minus", 12),
    ("Equal", 13),
    ("Backspace", 14),
    ("Tab", 15),
    ("KeyQ", 16),
    ("KeyW", 17),
    ("KeyE", 18),
    ("KeyR", 19),
    ("KeyT", 20),
    ("KeyY", 21),
    ("KeyU", 22),
    ("KeyI", 23),
    ("KeyO", 24),
    ("KeyP", 25),
    ("BracketLeft", 26),
    ("BracketRight", 27),
    ("Enter", 28),
    ("ControlLeft", 29),
    ("KeyA", 30),
    ("KeyS", 31),
    ("KeyD", 32),
    ("KeyF", 33),
    ("KeyG", 34),
    ("KeyH", 35),
    ("KeyJ", 36),
    ("KeyK", 37),
    ("KeyL", 38),
    ("Semicolon", 39),
    ("Quote", 40),
    ("Backquote", 41),
    ("ShiftLeft", 42),
    ("Backslash", 43),
    ("KeyZ", 44),
    ("KeyX", 45),
    ("KeyC", 46),
    ("KeyV", 47),
    ("KeyB", 48),
    ("KeyN", 49),
    ("KeyM", 50),
    ("Comma", 51),
    ("Period", 52),
    ("Slash", 53),
    ("ShiftRight", 54),
    ("NumpadMultiply", 55),
    ("AltLeft", 56),
    ("Space", 57),
    ("CapsLock", 58),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("NumLock", 69),
    ("ScrollLock", 70),
    ("Numpad7", 71),
    ("Numpad8", 72),
    ("Numpad9", 73),
    ("NumpadSubtract", 74),
    ("Numpad4", 75),
    ("Numpad5", 76),
    ("Numpad6", 77),
    ("NumpadAdd", 78),
    ("Numpad1", 79),
    ("Numpad2", 80),
    ("Numpad3", 81),
    ("Numpad0", 82),
    ("NumpadDecimal", 83),
    ("F11", 87),
    ("F12", 88),
    ("NumpadEnter", 96),
    ("ControlRight", 97),
    ("NumpadDivide", 98),
    ("AltRight", 100),
    ("Home", 102),
    ("ArrowUp", 103),
    ("PageUp", 104),
    ("ArrowLeft", 105),
    ("ArrowRight", 106),
    ("End", 107),
    ("ArrowDown", 108),
    ("PageDown", 109),
    ("Insert", 110),
    ("Delete", 111),
    ("MetaLeft", 125),
    ("MetaRight", 126),
    ("ContextMenu", 127),
];

/// Normalize one DOM key event. Returns `None` for unknown codes — the
/// caller drops those at the boundary.
pub fn normalize_key(code: &str, pressed: bool) -> Option<NormalizedInput> {
    KEY_TABLE
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, key)| NormalizedInput::Key {
            code: KeyCode(*key),
            pressed,
        })
}

// ---------------------------------------------------------------------------

/// Records every presented frame byte-exactly, in order.
#[derive(Debug, Default)]
pub struct MockSurface {
    pub frames: Vec<Frame>,
}

impl MockSurface {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_frame(&self) -> Option<&Frame> {
        self.frames.last()
    }
}

impl GpuSurface for MockSurface {
    fn present(&mut self, frame: &Frame) {
        self.frames.push(frame.clone());
    }
}

/// In-memory socket pair. What end A `send`s, end B `recv`s, FIFO,
/// datagram boundaries preserved. Created with [`LoopbackSocket::pair`].
///
/// Intentionally `!Send`: the shared queue is `Rc<RefCell<..>>` for the
/// single-threaded test/mock world. A threaded transport would be a new
/// adapter behind the same [`NetSocket`] trait.
#[derive(Debug, Clone)]
pub struct LoopbackSocket {
    inbound: Rc<RefCell<VecDeque<Vec<u8>>>>,
    outbound: Rc<RefCell<VecDeque<Vec<u8>>>>,
}

impl LoopbackSocket {
    /// A connected pair `(a, b)`: `a.send(..)` is readable via `b.recv()`.
    pub fn pair() -> (Self, Self) {
        let ab = Rc::new(RefCell::new(VecDeque::new()));
        let ba = Rc::new(RefCell::new(VecDeque::new()));
        (
            Self {
                inbound: ba.clone(),
                outbound: ab.clone(),
            },
            Self {
                inbound: ab,
                outbound: ba,
            },
        )
    }
}

impl NetSocket for LoopbackSocket {
    fn send(&mut self, data: &[u8]) {
        self.outbound.borrow_mut().push_back(data.to_vec());
    }

    fn recv(&mut self) -> Option<Vec<u8>> {
        self.inbound.borrow_mut().pop_front()
    }
}

/// Feeds scripted [`NormalizedInput`] vectors, one per [`InputSource::poll`].
/// After the script is exhausted, `poll` returns an empty `Vec`.
#[derive(Debug, Default)]
pub struct ScriptedInput {
    scripts: Vec<Vec<NormalizedInput>>,
    cursor: usize,
}

impl ScriptedInput {
    pub fn new(scripts: Vec<Vec<NormalizedInput>>) -> Self {
        Self { scripts, cursor: 0 }
    }
}

impl InputSource for ScriptedInput {
    fn poll(&mut self) -> Vec<NormalizedInput> {
        if self.cursor < self.scripts.len() {
            let out = self.scripts[self.cursor].clone();
            self.cursor += 1;
            out
        } else {
            Vec::new()
        }
    }
}

// ---------------------------------------------------------------------------
// Keyboard input: DOM key events -> console bytes
//
// The console is a byte stream, but `normalize_key` (above) emits Linux
// KEY_* codes — wiring those straight into `poll_input` would send control
// bytes (Enter → 0x1C FS). `KeyboardInput` is the host-side translator the
// real browser's keydown/keyup listeners feed: it converts DOM key events
// (`code` + `key` + pressed) into byte-carrying `NormalizedInput::Key`
// values using the byte-channel convention (`KeyCode(b)`, b < 256 =
// literal byte b), which is exactly what `ConsoleState::poll_input`
// consumes. No contract change; the translation is pure and deterministic.
//
// Byte-channel convention (console input path): a pressed
// `NormalizedInput::Key { code: KeyCode(b), pressed: true }` with `b < 256`
// carries the literal byte `b`. Host adapters (this one) produce these;
// `poll_input` forwards them into the RX FIFO.
// ---------------------------------------------------------------------------

/// One DOM key event, as a browser `keydown`/`keyup` listener delivers it:
/// `KeyboardEvent.code` (physical key, e.g. `"KeyH"`),
/// `KeyboardEvent.key` (resolved character, e.g. `"h"` / `"H"` / `"Enter"`),
/// and press state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomKeyEvent {
    pub code: String,
    pub key: String,
    pub pressed: bool,
}

/// Host-side keyboard adapter: DOM key events in, console bytes out.
///
/// The real browser calls [`KeyboardInput::push_event`] from its
/// `keydown`/`keyup` listeners; the orchestrator drains it through
/// [`InputSource::poll`] (via `pump_input`) once per input batch.
/// Releases and unmappable keys are dropped at the boundary — raw junk is
/// never forwarded, same discipline as `normalize_key`.
#[derive(Debug, Default)]
pub struct KeyboardInput {
    queue: VecDeque<DomKeyEvent>,
}

impl KeyboardInput {
    /// Empty adapter; events arrive via [`KeyboardInput::push_event`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one DOM key event — exactly what a `keydown`/`keyup` listener
    /// delivers (`event.code`, `event.key`, press state).
    pub fn push_event(&mut self, code: &str, key: &str, pressed: bool) {
        self.queue.push_back(DomKeyEvent {
            code: code.to_string(),
            key: key.to_string(),
            pressed,
        });
    }

    /// Translate one pressed event to a console byte. `None` means dropped
    /// at the boundary: releases, multi-char keys (`"F1"`, `"Shift"`,
    /// `"Dead"`, `"Unidentified"`), and non-ASCII input (MVP is ASCII-only).
    fn event_to_byte(ev: &DomKeyEvent) -> Option<u8> {
        if !ev.pressed {
            return None;
        }
        // Layout-independent specials, matched on physical `code`.
        match ev.code.as_str() {
            "Enter" | "NumpadEnter" => return Some(b'\n'),
            "Backspace" => return Some(0x7F), // DEL: standard terminal erase
            "Tab" => return Some(b'\t'),
            "Escape" => return Some(0x1B),
            _ => {}
        }
        // Printables: the browser already resolved shift/layout into `key`
        // ("h" vs "H"), so the adapter never tracks modifiers.
        let bytes = ev.key.as_bytes();
        if bytes.len() == 1 && (0x20..=0x7E).contains(&bytes[0]) {
            return Some(bytes[0]);
        }
        None
    }
}

impl InputSource for KeyboardInput {
    fn poll(&mut self) -> Vec<NormalizedInput> {
        let mut out = Vec::new();
        while let Some(ev) = self.queue.pop_front() {
            if let Some(b) = Self::event_to_byte(&ev) {
                out.push(NormalizedInput::Key {
                    code: KeyCode(b as u32),
                    pressed: true,
                });
            }
        }
        out
    }
}

/// HashMap-backed [`BlobStore`]. `save` overwrites; `load` of a missing key
/// returns `None`. An empty `Vec<u8>` value round-trips as `Some(vec![])`,
/// distinct from a missing key.
///
/// Used by leaf 5.1 for snapshot persist:
///
/// ```
/// use u13_adapters::MemStore;
/// use pathn_contracts::adapters::BlobStore;
/// use pathn_contracts::machine::Snapshot;
///
/// let mut store = MemStore::new();
/// let snap = Snapshot(vec![1, 2, 3]);
/// store.save("boot", &snap.0);
/// assert_eq!(store.load("boot"), Some(vec![1, 2, 3]));
/// ```
#[derive(Debug, Default)]
pub struct MemStore {
    map: HashMap<String, Vec<u8>>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl BlobStore for MemStore {
    fn save(&mut self, key: &str, data: &[u8]) {
        self.map.insert(key.to_string(), data.to_vec());
    }

    fn load(&self, key: &str) -> Option<Vec<u8>> {
        self.map.get(key).cloned()
    }
}

// ---------------------------------------------------------------------------
// Conformance tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time: the traits are object-safe, so `dyn` works for consumers
    // that need trait objects (e.g. the Wave-4 orchestrator's adapter slots).
    fn assert_obj_safe_gpusurface(_: &dyn GpuSurface) {}
    fn assert_obj_safe_netsocket(_: &dyn NetSocket) {}
    fn assert_obj_safe_inputsource(_: &dyn InputSource) {}
    fn assert_obj_safe_blobstore(_: &dyn BlobStore) {}

    fn assert_send<T: Send>() {}

    fn sample_frame() -> Frame {
        // 2x1 RGBA: red pixel, green pixel.
        Frame::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap()
    }

    #[test]
    fn conform_surface_records_frame_byte_exact() {
        let mut s = MockSurface::new();
        let f = sample_frame();
        s.present(&f);
        assert_eq!(s.frames.len(), 1);
        assert_eq!(s.frames[0], f);
        assert_eq!(s.last_frame(), Some(&f));
    }

    #[test]
    fn conform_surface_records_multiple_frames_in_order() {
        let mut s = MockSurface::new();
        let f1 = Frame::new(1, 1, vec![1, 2, 3, 4]).unwrap();
        let f2 = Frame::new(1, 1, vec![5, 6, 7, 8]).unwrap();
        s.present(&f1);
        s.present(&f2);
        assert_eq!(s.frames, vec![f1, f2]);
    }

    #[test]
    fn conform_frame_rejects_bad_length() {
        let err = Frame::new(2, 2, vec![0u8; 15]).unwrap_err();
        assert_eq!(err.expected, 16);
        assert_eq!(err.got, 15);
        assert!(Frame::new(0, 0, vec![]).is_ok());
    }

    #[test]
    fn conform_socket_loopback_preserves_order_and_bytes() {
        let (mut a, mut b) = LoopbackSocket::pair();
        a.send(b"first");
        a.send(b"second-payload-with-\x00-bytes");
        a.send(b"third");
        assert_eq!(b.recv(), Some(b"first".to_vec()));
        assert_eq!(b.recv(), Some(b"second-payload-with-\x00-bytes".to_vec()));
        assert_eq!(b.recv(), Some(b"third".to_vec()));
        assert_eq!(b.recv(), None);
    }

    #[test]
    fn conform_socket_empty_send_is_real_datagram() {
        let (mut a, mut b) = LoopbackSocket::pair();
        a.send(b"");
        // An empty send must come back as Some(empty), never collapse to None.
        assert_eq!(b.recv(), Some(vec![]));
        assert_eq!(b.recv(), None);
    }

    #[test]
    fn conform_socket_pair_has_no_self_loopback() {
        let (mut a, mut b) = LoopbackSocket::pair();
        a.send(b"for-b");
        // A's own queue is empty: pairs are directional, like real sockets.
        assert_eq!(a.recv(), None);
        assert_eq!(b.recv(), Some(b"for-b".to_vec()));
        // And the reverse direction works.
        b.send(b"for-a");
        assert_eq!(a.recv(), Some(b"for-a".to_vec()));
        assert_eq!(b.recv(), None);
    }

    #[test]
    fn conform_input_key_goldens() {
        let cases: &[(&str, u32)] = &[
            ("Escape", 1),
            ("Digit1", 2),
            ("Digit0", 11),
            ("Backspace", 14),
            ("Tab", 15),
            ("KeyQ", 16),
            ("KeyA", 30),
            ("Enter", 28),
            ("ShiftLeft", 42),
            ("Space", 57),
            ("F5", 63),
            ("F11", 87),
            ("ArrowUp", 103),
            ("ArrowLeft", 105),
            ("ArrowRight", 106),
            ("ArrowDown", 108),
            ("Delete", 111),
            ("ControlLeft", 29),
            ("AltLeft", 56),
            ("MetaLeft", 125),
        ];
        for (code, want) in cases {
            let ev =
                normalize_key(code, true).unwrap_or_else(|| panic!("code {code} should normalize"));
            assert_eq!(
                ev,
                NormalizedInput::Key {
                    code: KeyCode(*want),
                    pressed: true
                },
                "code {code}"
            );
            // Release carries the same code with pressed=false.
            let rel = normalize_key(code, false).unwrap();
            assert_eq!(
                rel,
                NormalizedInput::Key {
                    code: KeyCode(*want),
                    pressed: false
                },
                "release {code}"
            );
        }
    }

    #[test]
    fn conform_input_unknown_code_dropped() {
        assert_eq!(normalize_key("BogusKey", true), None);
        assert_eq!(normalize_key("", true), None);
        assert_eq!(normalize_key("keya", true), None); // case-sensitive: raw junk never passes
        assert_eq!(normalize_key("KeyA ", true), None);
    }

    #[test]
    fn conform_input_scripted_poll_order() {
        let k_a = normalize_key("KeyA", true).unwrap();
        let k_a_up = normalize_key("KeyA", false).unwrap();
        let mv = NormalizedInput::PointerMove { x: 10, y: -3 };
        let mut src = ScriptedInput::new(vec![vec![k_a], vec![k_a_up, mv], vec![]]);
        assert_eq!(src.poll(), vec![k_a]);
        assert_eq!(src.poll(), vec![k_a_up, mv]);
        assert_eq!(src.poll(), vec![]);
        // Exhausted: stays empty, never panics.
        assert_eq!(src.poll(), Vec::new());
    }

    // ---------- KeyboardInput: DOM key events -> console bytes ----------

    fn byte_key(b: u8) -> NormalizedInput {
        NormalizedInput::Key {
            code: KeyCode(b as u32),
            pressed: true,
        }
    }

    #[test]
    fn keyboard_printable_lowercase_maps_to_byte() {
        let mut kb = KeyboardInput::new();
        kb.push_event("KeyH", "h", true);
        assert_eq!(kb.poll(), vec![byte_key(b'h')]);
    }

    #[test]
    fn keyboard_shift_uppercase_comes_from_key_not_modifiers() {
        // The browser resolves Shift+KeyH into key="H"; the adapter never
        // tracks modifiers.
        let mut kb = KeyboardInput::new();
        kb.push_event("KeyH", "H", true);
        kb.push_event("Digit1", "!", true); // Shift+Digit1, browser-resolved
        kb.push_event("Space", " ", true);
        assert_eq!(
            kb.poll(),
            vec![byte_key(b'H'), byte_key(b'!'), byte_key(b' ')]
        );
    }

    #[test]
    fn keyboard_specials_matched_by_code() {
        let mut kb = KeyboardInput::new();
        kb.push_event("Enter", "Enter", true);
        kb.push_event("NumpadEnter", "Enter", true);
        kb.push_event("Backspace", "Backspace", true);
        kb.push_event("Tab", "Tab", true);
        kb.push_event("Escape", "Escape", true);
        assert_eq!(
            kb.poll(),
            vec![
                byte_key(b'\n'),
                byte_key(b'\n'),
                byte_key(0x7F),
                byte_key(b'\t'),
                byte_key(0x1B),
            ]
        );
    }

    #[test]
    fn keyboard_releases_produce_no_bytes() {
        let mut kb = KeyboardInput::new();
        kb.push_event("KeyH", "h", true);
        kb.push_event("KeyH", "h", false); // release: dropped
        kb.push_event("Enter", "Enter", false); // release: dropped
        assert_eq!(kb.poll(), vec![byte_key(b'h')]);
    }

    #[test]
    fn keyboard_unmapped_keys_dropped_at_boundary() {
        let mut kb = KeyboardInput::new();
        kb.push_event("F1", "F1", true);
        kb.push_event("ShiftLeft", "Shift", true);
        kb.push_event("KeyA", "Dead", true);
        kb.push_event("Unidentified", "Unidentified", true);
        kb.push_event("", "", true);
        assert_eq!(kb.poll(), Vec::new());
    }

    #[test]
    fn keyboard_non_ascii_dropped_mvp_is_ascii_only() {
        let mut kb = KeyboardInput::new();
        kb.push_event("KeyE", "\u{e9}", true); // 'é': 2-byte UTF-8
        assert_eq!(kb.poll(), Vec::new());
    }

    #[test]
    fn keyboard_poll_drains_in_order_then_empties() {
        let mut kb = KeyboardInput::new();
        kb.push_event("KeyH", "h", true);
        kb.push_event("KeyI", "i", true);
        kb.push_event("F5", "F5", true); // dropped, order of the rest kept
        kb.push_event("Enter", "Enter", true);
        assert_eq!(
            kb.poll(),
            vec![byte_key(b'h'), byte_key(b'i'), byte_key(b'\n')]
        );
        // Drained: stays empty, never panics.
        assert_eq!(kb.poll(), Vec::new());
    }

    #[test]
    fn keyboard_linux_keycodes_are_not_bytes() {
        // Regression guard for the Wave-6 lesson: normalize_key emits
        // Linux KEY_* codes (Enter -> 28), which are NOT console bytes.
        // KeyboardInput must not pass them through raw.
        let mut kb = KeyboardInput::new();
        kb.push_event("Enter", "Enter", true);
        let out = kb.poll();
        assert_eq!(out, vec![byte_key(b'\n')]);
        assert_ne!(out, vec![byte_key(28)]);
    }

    #[test]
    fn conform_blobstore_roundtrip() {
        let mut m = MemStore::new();
        m.save("snap/boot", &[0, 1, 2, 255]);
        assert_eq!(m.load("snap/boot"), Some(vec![0, 1, 2, 255]));
    }

    #[test]
    fn conform_blobstore_missing_key_is_none() {
        let m = MemStore::new();
        assert_eq!(m.load("nope"), None);
    }

    #[test]
    fn conform_blobstore_overwrite_replaces() {
        let mut m = MemStore::new();
        m.save("k", b"v1");
        m.save("k", b"v2-longer");
        assert_eq!(m.load("k"), Some(b"v2-longer".to_vec()));
    }

    #[test]
    fn conform_blobstore_empty_value_distinct_from_missing() {
        let mut m = MemStore::new();
        m.save("empty", b"");
        assert_eq!(m.load("empty"), Some(vec![]));
        assert_eq!(m.load("missing"), None);
    }

    #[test]
    fn conform_traits_are_object_safe_and_mocks_send() {
        let s = MockSurface::new();
        assert_obj_safe_gpusurface(&s);
        let (a, _b) = LoopbackSocket::pair();
        assert_obj_safe_netsocket(&a);
        let src = ScriptedInput::new(vec![]);
        assert_obj_safe_inputsource(&src);
        let kb = KeyboardInput::new();
        assert_obj_safe_inputsource(&kb);
        let m = MemStore::new();
        assert_obj_safe_blobstore(&m);
        // Owned-data mocks are Send (LoopbackSocket is documented !Send).
        assert_send::<MockSurface>();
        assert_send::<ScriptedInput>();
        assert_send::<KeyboardInput>();
        assert_send::<MemStore>();
        assert_send::<Frame>();
        assert_send::<NormalizedInput>();
    }

    #[test]
    fn conform_quarantine_no_os_io() {
        // Source-scan guard: this file must never grow OS/browser I/O.
        // Forbidden tokens are built via concat so this test's own source
        // does not self-trigger the scan; comment lines are excluded because
        // the module docs honestly *name* the forbidden crates.
        let code: String = include_str!("lib.rs")
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // Every token is assembled from fragments so no forbidden literal
        // appears in this test's own source.
        let frags: &[&[&str]] = &[
            &["std", "::", "net"],
            &["std", "::", "fs"],
            &["std", "::", "thread"],
            &["std", "::", "process"],
            &["std", "::", "os"],
            &["to", "kio"],
            &["m", "io"],
            &["win", "it"],
            &["web", "-", "sys"],
            &["wasm", "-", "bindgen"],
        ];
        let forbidden: Vec<String> = frags.iter().map(|f| f.concat()).collect();
        for tok in &forbidden {
            assert!(
                !code.contains(tok.as_str()),
                "quarantine breach: source contains `{tok}`"
            );
        }
        // `unsafe` as a leading keyword (comments already excluded above).
        for line in code.lines() {
            assert!(
                !line.trim_start().starts_with("unsafe"),
                "quarantine breach: `unsafe` block in {line}"
            );
        }
    }
}
