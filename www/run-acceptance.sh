#!/usr/bin/env bash
# Path N browser acceptance: the REAL pathn-sh guest in headless Chromium.
#
# Pipeline: cargo wasm build -> wasm-bindgen JS glue -> rsync www/ to the
# gsai box -> python http.server + headless chromium --dump-dom on
# acceptance.html -> exact TX pins asserted through the real DOM event path.
#
# Needs: wasm32-unknown-unknown target, wasm-bindgen-cli (same 0.2.x as the
# wasm-bindgen crate), Tailscale SSH route to gsai.
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

echo "== 3. push www/ to gsai =="
"$HOME/workspace/skills/box-sync/bin/box-sync.sh" push gsai "$REPO/www/" /mnt/sdb1/pathn-sh-www/

echo "== 4. headless chromium acceptance on gsai =="
no_scheme="${HTTPS_PROXY#*://}"
hostport="${no_scheme#*@}"
ph="${hostport%%:*}"
[ -n "$ph" ] || { echo "HTTPS_PROXY not set - tailnet proxy unknown" >&2; exit 1; }
ssh -i /home/hatch/.ssh/id_ed25519 -o BatchMode=yes -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null -o ConnectTimeout=30 \
    -o ProxyCommand="nc -X connect -x ${ph}:3130 %h %p" \
    muse@100.104.140.2 \
    'set -u
     cd /mnt/sdb1/pathn-sh-www
     python3 -m http.server 8123 >/tmp/pathn-http.log 2>&1 &
     SRV=$!
     sleep 1
     CHROME_BIN="$(command -v chromium || command -v chromium-browser || ls /snap/bin/chromium 2>/dev/null)"
     [ -n "$CHROME_BIN" ] || { echo "no chromium on box" >&2; exit 1; }
     "$CHROME_BIN" --headless=new --no-sandbox --disable-gpu --dump-dom \
         --virtual-time-budget=60000 http://127.0.0.1:8123/acceptance.html \
         > /tmp/pathn-accept.html 2>/tmp/pathn-chromium.log || true
     # Kill the server by PID ($SRV) — never pkill -f, whose pattern would
     # match the command line of this script itself and kill the session.
     kill $SRV 2>/dev/null || true
     echo "--- title ---"
     grep -o "<title>[^<]*</title>" /tmp/pathn-accept.html || true
     echo "--- results ---"
     grep -o "<pre id=\"results\">[^<]*</pre>" /tmp/pathn-accept.html || tail -c 1500 /tmp/pathn-accept.html'
