# Track T691 — Symbol gate on the Route A Image + re-flight (box)

Verdict: FAIL

RECONSTRUCTED-TAG: this file was reconstructed by the reviewer (driver certification, 2026-11-07 ~23:11 AEDT). The agent wrote no findings of its own; every claim below is from driver-verified box evidence.

## Lane
- Track T691 symbolgate-reflight-2, successor to driver-certified T690 honest FAIL.
- Route: box opencode. Agent PID 3465206, serve child PID 3465218 (`opencode serve --stdio --port 0`).
- Launched 2026-11-07 09:09 UTC (20:09 AEDT). Killed TERM 2026-11-07 12:11 UTC (23:11 AEDT) by the driver under the locked wedge rule (AGENTS.md, 2026-11-07) and the dated 23:09 AEDT trigger: zero progress at the 3h mark = WEDGED, kill + certify FAIL + successor launch autonomously.
- Elapsed at kill: 03:01:47. CPU 00:00:14 each (lifetime). Both PIDs gone after TERM; no orphan; log growth stopped (mtime 23:11:05, TUI teardown only).

## Progress (brief steps)
- NO brief step executed at all. Step 0 (disk gate) NOT started: no progress-t691.txt and no heartbeat.json were ever written.
- Step 1 (input SHAs) NOT started. Step 2 (symbol gate) NOT started. Step 3 (re-flight) NOT started. Step 4 (A/B control) NOT started. Step 5 (adjudicate) NOT started.
- Zero tool calls in 3h02m. No findings, outcome, or death note written by the agent. flights/ + diagnostics/ empty. Guest inputs were staged (hardlinked + SHA-verified at launch) but never read by the agent.

## Evidence
- Agent log opencode-t691.log: 26,201 bytes, SHA-256 b532f3b98fb0915bb4a4682737f07dd20e649b62d15d00d2dcdf9a767f0c00b3. Content: terminal capability probes at start (opentui-notifications, Capabilities query), then a frozen "New session" spinner — ANSI cursor-positioning + colour codes only. Size frozen at 26,080 B from 20:09:14 until the kill; the final 121 B are TUI teardown sequences on TERM (mouse-mode disable, cursor show). The single "tool" string in the log is the brief's own wedge-guard paragraph rendered in the terminal ("without a tool call while work remains") — not an executed tool call.
- No QEMU process ever ran (pgrep qemu empty after the kill; the only earlier match was the opencode cmdline itself, whose prompt text contains "QEMU").
- Box disk /dev/sda3 stayed 94%, 9.0 GiB avail throughout — the frozen log did not flood the disk (contrast T690's 8.3 GB flood).

## Markers (all NOT observed — no flight)
- `Freeing unused kernel memory`: NOT observed.
- `init: init first stage started!`: NOT observed.
- `simple-framebuffer ... fb0: simplefb registered!`: NOT observed.
- sys.boot_completed: NOT observed (no flight, no guest log).
- TEN-PASSES: not run. G691-1/2: not met (no gate evidence).

## First failing link
- No guest link failed — the lane failed in the agent harness before any work: the agent never got past the "New session" spinner and executed zero tool calls in 3h02m. The Route A kernel was never symbol-gated and never flown. This is the never-started stall pattern: log frozen at launch size, distinct from T690's TUI-loop flood (log growing ~44 MB/min).
- The Route A Image (SHA 11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367, 20,384,256 B) remains UNPROVEN and is reusable by the successor. Route A vmlinux (SHA 4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5, 307,656,840 B) likewise.

WALL-TYPE-TAG: agent/tooling — opencode never-started, zero work executed; not a guest wall.
Rust par bhi lagu: nahi — no guest evidence at all; the guest kernel was never flown.

## Successor
- T692 (symbol gate + re-flight, no rebuild), launched the same turn by the driver per the locked repeat-loop rule. Its launch is hardened against this failure mode: TERM=dumb + CI=true env (anti-TUI shape), and the driver verifies Step-0 progress within minutes of launch instead of waiting hours to notice a dead start.

## Note
- Mac route recovered ~21:49 AEDT after the 19:08–21:37 outage; Mac-side certification (registry, worktree commit, bundle, GitHub push) completed in this same cycle — unlike T690, whose Mac-side items were deferred.
