// pathn-sh browser terminal.
// Rust owns the emulator (PathnShell); JS owns the DOM, the keyboard
// listeners, and the animation-frame stepping loop.

import init, { PathnShell } from './pkg/web_host.js';

let shell = null;
const term = document.getElementById('term');

function render(s) {
  term.textContent = s;
  window.scrollTo(0, document.body.scrollHeight);
}

async function main() {
  await init();
  try {
    shell = new PathnShell();
  } catch (e) {
    render('BOOT FAILED: ' + e);
    return;
  }
  render(shell.tx_text());

  // DOM key events -> console bytes. Key-up events and unmapped keys
  // produce no bytes (handled inside KeyboardInput, pinned by tests).
  window.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    shell.push_key(e.code, e.key, true);
    e.preventDefault();
  });
  window.addEventListener('keyup', (e) => {
    shell.push_key(e.code, e.key, false);
  });

  const frame = () => {
    const out = shell.step_frame();
    if (out) render(term.textContent + out);
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
}

main();
