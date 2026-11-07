# Track T690 — Symbol gate on the Route A Image + re-flight (box)

Verdict: FAIL

RECONSTRUCTED-TAG: this file was reconstructed by the reviewer (mac-agent-liveness worker, driver certification, 2026-11-07 ~20:05 AEDT). The agent wrote no findings of its own; every claim below is from driver-verified box evidence.

## Lane
- Track T690 symbolgate-reflight, successor to driver-certified T689 honest FAIL.
- Route: box opencode. Agent PID 3300775, serve child PID 3300786 (`opencode serve --stdio --port 0`).
- Launched 2026-11-07 05:56 UTC (16:56 AEDT). Killed TERM 2026-11-07 08:58:36 UTC (19:58:36 AEDT) by the liveness worker under the locked wedge rule (AGENTS.md, 2026-11-07) and the dated 19:56 AEDT trigger: zero progress at the 3h mark = WEDGED, kill + certify FAIL + successor launch autonomously.
- Elapsed at kill: 03:02. CPU 1.9% (lifetime). Both PIDs gone after TERM; no orphan; log growth stopped (mtime frozen 19:58:36).

## Progress (brief steps)
- Step 0 PASS only: disk gate, 8.8 GiB avail at 05:57:37Z, recorded in progress-t690.txt + heartbeat.json.
- Step 1 (input SHAs) NOT started. Step 2 (symbol gate) NOT started. Step 3 (re-flight) NOT started. Step 4 (A/B control) NOT started. Step 5 (adjudicate) NOT started.
- Zero brief steps in 3h02m. No findings, outcome, or death note written by the agent. scripts/ empty (supervisor_t690.py never staged). flights/ + diagnostics/ empty.

## Evidence
- Agent log opencode-t690.log: 8,314,233,756 bytes, SHA-256 50fbbfdd918e5b762c36a4135799facc62ca0dc41b9911b82ddfd0d66a538ef0. Content: ANSI TUI repaint only (cursor-positioning + colour codes, spinner fragments). Zero tool-call keywords in the last 50 MB. Full-log grep (all 8.3 GB) for quota / usage-limit / rate-limit / free-messages / billing / upgrade: ZERO matches — the 19:20 "free messages/month/Resets" fragment is REFUTED; it does not occur anywhere in the log. Log tail ends with TUI teardown sequences on TERM.
- Log growth ~44 MB/min of ANSI churn; the flood took box disk /dev/sda3 from 8.8 GiB avail (Step 0) to 100% (962M avail) at kill. Log moved to the box vault /mnt/sdb1/kernel-critical/evidence/t690-opencode.log (same SHA) to free the disk for the successor; /mnt/sdb1 was 67%, 65G avail.
- No QEMU process ever ran: the only `pgrep qemu` match was the opencode cmdline itself (its prompt text contains "QEMU").

## Markers (all NOT observed — no flight)
- `Freeing unused kernel memory`: NOT observed.
- `init: init first stage started!`: NOT observed.
- `simple-framebuffer ... fb0: simplefb registered!`: NOT observed.
- sys.boot_completed: NOT observed (no flight, no guest log).
- TEN-PASSES: not run. G690-1/2: not met (no gate evidence).

## First failing link
- No guest link failed — the lane failed in the agent harness: the agent executed Step 0, then made no further tool call for 3h02m while the opencode TUI repainted. The Route A kernel was never symbol-gated and never flown.
- The Route A Image (SHA 11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367, 20,384,256 B) remains UNPROVEN and is reusable by the successor. Route A vmlinux (SHA 4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5, 307,656,840 B) likewise.

WALL-TYPE-TAG: agent/tooling — opencode TUI loop, zero work executed; not a guest wall.
Rust par bhi lagu: nahi — no guest evidence at all; the guest kernel was never flown.

## Successor
- T691 (symbol gate + re-flight, no rebuild), launched the same turn by the liveness worker per the locked repeat-loop rule. Its brief carries a wedge-guard paragraph drawn from this failure (progress + heartbeat after every step; never 10 minutes without a tool call; file FAIL and exit on a repeated tool failure or any usage/quota dialog).

## Note
- Mac route offline since ~19:08 AEDT (Tailscale: ektas-macbook-air offline, last seen ~19:10 AEDT); Mac-side registry/bundle steps deferred to route return. Box-side certification above is complete.
