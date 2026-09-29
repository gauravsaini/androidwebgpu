#!/usr/bin/env bash
# Path N Track A GPU acceptance: the REAL pathn-sh guest's real GPU command
# stream rendered through the REAL WebGPU stack in headless Chromium.
#
# Pipeline: cargo wasm build -> wasm-bindgen JS glue -> rsync www/ to a
# DISTINCT gsai dir (/mnt/sdb1/pathn-gpu-www/) -> Playwright drives full
# Chrome-for-Testing (real time, NOT virtual-time) on gpu-acceptance.html ->
# waits for the page's own ACCEPT-PASS/FAIL title -> result JSON + screenshot.
#
# Why Playwright + real time (verified 2026-09-30, do NOT "simplify"):
# - `--virtual-time-budget` freezes GPU work: queue.submit returns but Dawn
#   never executes, so mapAsync/readback hangs forever.
# - `--use-vulkan=swiftshader` hangs `requestDevice()`; without it the
#   SwiftShader fallback device creates fine.
# - Headless `--screenshot` does not composite WebGPU canvases (pure-JS
#   triangle submits cleanly yet screenshots black); the page readbacks the
#   real render target and blits it to a 2D canvas for the screenshot.
# - Headless Dawn LOSES the device on getCurrentTexture() ("A valid external
#   Instance reference no longer exists"); the page uses execute_pending +
#   readback and never presents.
# - `--dump-dom` fires at page load in real time, before async JS finishes;
#   Playwright's wait_for_function is the reliable completion signal.
#
# Done = real headless Chromium + visible golden triangle + screenshot +
# pixel assertions green + acceptance committed under www/.
#
# Distinct box dir: /mnt/sdb1/pathn-gpu-www/ (never pathn-sh-www).
# Free port: 8125 (never 8124 = live demo, never 8123).
#
# Needs: wasm32-unknown-unknown target, wasm-bindgen-cli 0.2.129,
# Tailscale SSH route to gsai, Playwright venv on the box.
set -euo pipefail
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi
REPO="$(git rev-parse --show-toplevel)"
cd "$REPO"

echo "== 1. wasm build =="
cargo build --release --target wasm32-unknown-unknown -p web-host

echo "== 2. wasm-bindgen glue =="
WASM="$REPO/target/wasm32-unknown-unknown/release/web_host.wasm"
rm -rf "$REPO/www/pkg"
wasm-bindgen "$WASM" --out-dir "$REPO/www/pkg" --target web
ls "$REPO/www/pkg"

echo "== 3. push www/ to gsai (DISTINCT dir: pathn-gpu-www) =="
"$HOME/workspace/skills/box-sync/bin/box-sync.sh" push gsai "$REPO/www/" /mnt/sdb1/pathn-gpu-www/

echo "== 4. headless Chromium GPU acceptance on gsai (Playwright, real time) =="
no_scheme="${HTTPS_PROXY#*://}"
hostport="${no_scheme#*@}"
ph="${hostport%%:*}"
[ -n "$ph" ] || { echo "HTTPS_PROXY not set - tailnet proxy unknown" >&2; exit 1; }
# A function (not a string variable): the ProxyCommand's embedded spaces
# break when expanded from a variable ("invalid quotes").
box_ssh() {
    ssh -i /home/hatch/.ssh/id_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o ConnectTimeout=30 \
        -o ProxyCommand="nc -X connect -x ${ph}:3130 %h %p" \
        muse@100.104.140.2 "$@"
}
box_scp() {
    scp -i /home/hatch/.ssh/id_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null \
        -o ProxyCommand="nc -X connect -x ${ph}:3130 %h %p" "$@"
}

# The HTTP server must already be listening on 8125 (start it once per box
# boot; do NOT start/stop it here — a dead server is a hard failure).
box_ssh 'curl -s -m 5 -o /dev/null -w "http:%{http_code}\n" http://127.0.0.1:8125/gpu-acceptance.html' \
    | grep -q "http:200" || { echo "FATAL: no HTTP server on gsai:8125" >&2; exit 1; }

# Xvfb for headed-GPU-process support; harmless if already running.
box_ssh 'pgrep -f "Xvfb :99" >/dev/null || (Xvfb :99 -screen 0 1400x900x24 >/tmp/xvfb.log 2>&1 & sleep 2)'

box_ssh 'DISPLAY=:99 timeout 280 /home/muse/.venvs/xhunt/bin/python /mnt/sdb1/pathn-gpu-www/gpu-accept-driver.py' \
    > /tmp/pathn-gpu-accept-run.log 2>&1
echo "--- driver output ---"
cat /tmp/pathn-gpu-accept-run.log

echo "== 5. fetch evidence =="
EVID="$REPO/www/gpu-acceptance-evidence"
mkdir -p "$EVID"
box_scp muse@100.104.140.2:/mnt/sdb1/pathn-gpu-www/accept-out.png "$EVID/accept-out.png"
box_scp muse@100.104.140.2:/mnt/sdb1/pathn-gpu-www/accept-out.json "$EVID/accept-out.json"

echo "== 6. strict gate =="
[ -s "$EVID/accept-out.png" ] || { echo "FATAL: screenshot missing or empty" >&2; exit 1; }
python3 - "$EVID/accept-out.json" <<'PYEOF'
import json, sys
d = json.load(open(sys.argv[1]))
assert d["title"] == "ACCEPT-PASS gpu", f"title is {d['title']!r}, want 'ACCEPT-PASS gpu'"
fails = [r for r in d["results"] if not r["pass"]]
assert not fails, f"failing checks: {[r['name'] for r in fails]}"
print(f"GATE GREEN: {len(d['results'])}/{len(d['results'])} checks pass")
PYEOF
sha256sum "$EVID/accept-out.png"
echo "verify: GPU acceptance OK"
