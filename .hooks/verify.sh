#!/usr/bin/env bash
# Path N pre-push gate: workspace tests + the wasm32 browser-host build.
# The headless browser acceptance (www/run-acceptance.sh) needs the gsai
# box's chromium, so it stays a manual milestone step, not a push gate.
set -u
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

echo "verify: cargo test --workspace"
cargo test --workspace --quiet || { echo "verify FAILED: workspace tests" >&2; exit 1; }

echo "verify: wasm32 browser-host build"
cargo build --quiet --target wasm32-unknown-unknown -p web-host \
    || { echo "verify FAILED: wasm32 build" >&2; exit 1; }

echo "verify: OK"
