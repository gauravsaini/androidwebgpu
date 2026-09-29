//! Native test of the exact browser-host path: `PathnShell` boots the real
//! guest, DOM-style key events flow through `push_key`, and `step_frame`
//! drains TX — the same calls the JavaScript terminal page makes, minus
//! the DOM.

use web_host::PathnShell;

/// Type text the browser way: physical `code` + resolved `key` per
/// keystroke, keydown followed by keyup (releases must produce no bytes).
fn type_text(sh: &mut PathnShell, text: &str) {
    for ch in text.chars() {
        match ch {
            '\n' => {
                sh.push_key("Enter", "Enter", true);
                sh.push_key("Enter", "Enter", false);
            }
            ' ' => {
                sh.push_key("Space", " ", true);
                sh.push_key("Space", " ", false);
            }
            'a'..='z' => {
                let code = format!("Key{}", ch.to_ascii_uppercase());
                let key = ch.to_string();
                sh.push_key(&code, &key, true);
                sh.push_key(&code, &key, false);
            }
            // Non-ASCII: the DOM `code` is layout-dependent (here French
            // AZERTY, where the é key reports Digit2); the adapter keys off
            // `key`, so any non-special code exercises the UTF-8 path.
            'é' => {
                sh.push_key("Digit2", "é", true);
                sh.push_key("Digit2", "é", false);
            }
            _ => panic!("type_text: no DOM mapping for {ch:?}"),
        }
    }
}

/// Run frames until the guest parks with no new output (or a cap).
fn settle(sh: &mut PathnShell) -> String {
    let mut out = String::new();
    for _ in 0..10 {
        let chunk = sh.step_frame();
        let grew = !chunk.is_empty();
        out.push_str(&chunk);
        if !grew && sh.parked() {
            break;
        }
    }
    out
}

#[test]
fn shell_boots_to_exact_prompt() {
    let sh = PathnShell::new().expect("boot must succeed");
    assert!(sh.parked(), "guest parked at WFI after boot");
    assert_eq!(sh.tx_text(), "pathn-sh> ");
    assert!(sh.steps() > 0);
}

#[test]
fn shell_help_command_exact_tx() {
    let mut sh = PathnShell::new().unwrap();
    type_text(&mut sh, "help\n");
    let new = settle(&mut sh);
    assert!(
        new.contains("commands: echo <args> | help"),
        "help output in new bytes: {new:?}"
    );
    assert_eq!(
        sh.tx_text(),
        "pathn-sh> help\ncommands: echo <args> | help\npathn-sh> "
    );
}

#[test]
fn shell_echo_command_exact_tx() {
    let mut sh = PathnShell::new().unwrap();
    type_text(&mut sh, "echo hi\n");
    settle(&mut sh);
    assert_eq!(sh.tx_text(), "pathn-sh> echo hi\nhi\npathn-sh> ");
}

#[test]
fn shell_unknown_command_exact_tx() {
    let mut sh = PathnShell::new().unwrap();
    type_text(&mut sh, "bogus\n");
    settle(&mut sh);
    assert_eq!(
        sh.tx_text(),
        "pathn-sh> bogus\nunknown cmd: bogus\npathn-sh> "
    );
}

#[test]
fn shell_utf8_input_round_trips_byte_exact() {
    // End-to-end UTF-8: DOM key event ("é", 2 bytes C3 A9) -> adapter ->
    // guest RX -> guest echo + unknown-cmd echo -> TX drain. The guest is
    // byte-oriented, so the bytes must survive the whole trip untouched.
    let mut sh = PathnShell::new().unwrap();
    type_text(&mut sh, "é\n");
    settle(&mut sh);
    assert_eq!(
        sh.tx_text(),
        "pathn-sh> \u{e9}\nunknown cmd: \u{e9}\npathn-sh> ",
        "UTF-8 bytes must round-trip byte-exact through the guest"
    );
}

#[test]
fn shell_step_frame_returns_only_new_bytes() {
    let mut sh = PathnShell::new().unwrap();
    // Boot bytes were drained by the constructor: first frame is quiet.
    assert_eq!(sh.step_frame(), "");
    type_text(&mut sh, "help\n");
    let first = sh.step_frame();
    assert!(
        first.starts_with("help\n"),
        "frame returns bytes since last call: {first:?}"
    );
}
