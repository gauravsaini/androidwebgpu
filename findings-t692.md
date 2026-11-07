# Track T692 — Symbol gate on the Route A Image + re-flight (box)

Verdict: FAIL

RECONSTRUCTED-TAG: this file was reconstructed by the reviewer (driver certification, 2026-11-08 ~02:00 AEDT). The agent wrote no findings of its own; every claim below is from driver-verified box evidence.

## Lane
- Track T692 symbolgate-reflight-3, successor to driver-certified T691 honest FAIL.
- Route: box opencode. Agent PID 3579456, serve child PID 3579465 (`opencode serve --stdio --port 0`).
- Launched 2026-11-07 12:18:44 UTC (23:18:44 AEDT), hardened launch TERM=dumb + CI=true.
- Agent EXITED on its own ~2026-11-08 01:50:28 AEDT (log mtime 01:50:28, +121 B TUI teardown sequences; final size 27,633 B). Elapsed 02:31:44. Both PIDs gone after exit; no orphan; no kill was needed — the process died by itself, ~29 min before the ~02:19 AEDT wedge trigger.
- This is the third consecutive box-opencode agent-start failure (T690 TUI loop, T691 never-started, T692 never-started). The hardened launch (TERM=dumb + CI=true) did NOT fix it — same never-started signature as T691, byte for byte in shape.

## Progress (brief steps)
- NO brief step executed at all. Step 0 (disk gate) NOT started: no progress-t692.txt and no heartbeat.json were ever written.
- Step 1 (input SHAs) NOT started by the agent — the driver verified all eight staged input SHAs itself at certification (see Evidence): all match the brief exactly.
- Step 2 (symbol gate) NOT started by the agent — the driver ran the gate itself at certification (see below): it PASSES.
- Step 3 (re-flight) NOT started. Step 4 (A/B control) NOT started. Step 5 (adjudicate) NOT started.
- Zero tool calls in 2h31m. No findings, outcome, or death note written by the agent. flights/ + diagnostics/ empty. Guest inputs were staged (hardlinked + SHA-verified at launch) but never read by the agent.

## Evidence
- Agent log opencode-t692.log: 27,633 bytes, SHA-256 b57b37f4742b88c0dfb569384049d3fcaff23b4cd49f5ea9c8348db339ee840c. Content: terminal capability probes at start (opentui-notifications, Capabilities query, state=idle:app=opencode), then a frozen "New session" spinner — ANSI cursor-positioning + colour codes only. Size frozen at 27,512 B from 23:18:49 until the exit; the final 121 B are TUI teardown sequences on exit (mouse-mode disable, cursor show). The single "tool" string in the log is the brief's own wedge-guard paragraph rendered in the terminal ("without a tool call while work remains") — not an executed tool call.
- No QEMU process ever ran (pgrep qemu empty after the exit; the only earlier match was the probe's own cmdline).
- Box disk /dev/sda3 stayed 94%, 9.1 GiB avail throughout — the frozen log did not flood the disk (contrast T690's 8.3 GB flood).
- Box opencode config hazard (found at certification, NOT the cause of this lane — the file was written 2026-11-08 01:55 AEDT, five minutes AFTER the agent exited): /home/muse/.config/opencode/opencode.json now sets the default model to qwen35-tpu/deepseek-v4-pro-qwen3.5-9b (Colab TPU endpoint via cloudflared). Any future box opencode lane launched without an explicit --model will route through that experimental endpoint, whose session q359b died ~01:55 AEDT and is being relaunched as q359c. Box lanes need an explicit model or a restored config before the next box launch. The T690–T692 never-start cause remains unidentified (opencode invocation shape, serve handshake, or whether the agent ever receives the prompt); it predates this config write.
- Staged input SHAs (driver-verified at certification, all match the brief): Route A Image 11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367 (20,384,256 B); Route A vmlinux 4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5 (307,656,840 B); canonical Image 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 (23,073,280 B); system.img 98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d (2,059,816,960 B); modified.dtb e83a6437908bead89bf8a062050639aa702d11c554397d2200bcf4104f6a7389 (1,920 B); extracted_kernel.config d840310a6579c6e4cd47e060c36fbe2a5905c9d969888758b0b04e1290994bd8 (133,733 B); base-t683-fix.cpio.gz 8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70 (958,780,579 B); T681 ELF ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132 (19,941,040 B); findings-t688.md 181a15807def6d9d2fd6f028b944342fc5578fc7043b5dcda9fc59bc599a1632 (23,120 B).

## Symbol gate — DRIVER-RUN, PASSES (the agent never ran it)
- Run by the driver at certification (2026-11-08 ~02:00 AEDT), NDK r29 llvm-nm on guest-inputs/routeA-vmlinux, brief Step 2 command verbatim.
- printk 0xffffff800811a4c4
- __die 0xffffff8008096bbc
- panic 0xffffff80080b7770
- dump_stack 0xffffff8008b41a64
- show_regs 0xffffff800808ec28
- _etext 0xffffff8008b70000
- __init_begin 0xffffff80091c0000
- __init_end 0xffffff80092b0000
- free_initmem 0xffffff80080ab440
- All five symbols are STRICTLY below _etext and below __init_begin: GATE PASSES. Route A fixed the T688 defect (T686 had all five inside init at 0xffffff800923xxxx; canonical printk 0xffffff8008216980 below _etext). The Route A kernel is symbol-clean and ready to fly — the flight is the next lane's job, not this lane's, because the agent never started.

## Markers (all NOT observed — no flight)
- `Freeing unused kernel memory`: NOT observed.
- `init: init first stage started!`: NOT observed.
- `simple-framebuffer ... fb0: simplefb registered!`: NOT observed.
- sys.boot_completed: NOT observed (no flight, no guest log).
- TEN-PASSES: not run. G692-1/2: not met by the agent (no gate evidence from the agent); the driver-run gate above is recorded here for the successor.

## First failing link
- No guest link failed — the lane failed in the agent harness before any work: the agent never got past the "New session" spinner, executed zero tool calls in 2h31m, and then exited on its own with no findings and no verdict. The Route A kernel was never flown by this lane, but the driver-run symbol gate PASSES, so the kernel itself is not the blocker — the box opencode launch path is.
- The Route A Image (SHA 11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367, 20,384,256 B) is now SYMBOL-GATED PASS by the driver and remains reusable by the successor. Route A vmlinux (SHA 4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5, 307,656,840 B) likewise.

WALL-TYPE-TAG: agent/tooling — opencode never-started, zero work executed, agent exited on its own; not a guest wall.
Rust par bhi lagu: nahi — no guest evidence at all; the guest kernel was never flown.

## Successor
- T693 (re-flight on the Route A kernel, symbol gate already PASSED by the driver) — launched the same turn by the driver per the locked repeat-loop rule. Launch route changed on evidence: NOT the box (three consecutive never-starts there, plus the box opencode config now defaults to the experimental qwen35-tpu endpoint) — the flight runs on the MAC via opencode (proven route: T682/T687/T688 executed there), Mac QEMU 11.1.2, with the canonical A/B control on the same Mac QEMU. The Route A Image is synced from the box vault for the flight.
