# DEATH NOTE — T694 launch-path-diagnosis

Verdict: FAIL (honest, never-started). Certified by driver 2026-11-08 ~08:40 AEDT.

## What happened

- Launched 2026-11-08 05:27:09 AEDT on the Mac via opencode v2.2.5
  (`/opt/homebrew/bin/opencode --standalone --prompt <brief> --auto`,
  backgrounded, PID 34237, serve child 34238).
- Wedge trigger ~08:27 AEDT (launch + 3h, locked timeout rule 2026-11-07):
  log frozen, zero steps. Fresh-verified wedged at 08:36:41 AEDT (PID alive
  0.1% CPU, elapsed 03:09:32, log 20,875 B frozen since 05:27:15, two
  samples 10 s apart identical), killed (TERM) 08:37:05 AEDT, two-sample
  dead (08:37:10 and 08:37:22), no children, no orphan, no `-9`.
- Elapsed at kill ~3h10m. Log /Users/Shared/codex-logs/track694-opencode.log
  final 20,996 B (+121 B at 08:37 — terminal teardown ANSI on kill, not
  agent work). Tail is TUI menu chrome ("/connect", "Switch model",
  "/models"); zero step text, zero tool calls.
- No progress-t694.txt, no findings-t694.md, no outcome-t694.txt, no
  heartbeat.json. The 10-minute start check (05:37:45 AEDT) had already
  FAILED: never started, Step 0 missed, brief self-exit guard not executed.

## What this means

- 5th consecutive never-start: T690 (box, TUI loop, 8.3 GB log, disk 100%),
  T691 (box, log frozen 26,080 B), T692 (box, log frozen 27,512 B, exited
  on its own), T693 (Mac, log frozen 62,057 B), T694 (Mac, log frozen
  20,875 B). The launch path itself is broken, not the flight — the Route A
  Image (11adf236) was never flown, and the driver-run symbol gate (PASS at
  T692 certification: printk 0xffffff800811a4c4 below _etext) still stands
  unproven in flight.
- The diagnosis lane itself never started, so its Step 4 (launcher
  comparison + controlled probes) never ran. The driver ran the launcher
  comparison at certification instead: launch-t694-op.sh vs
  launch-t688-op.sh vs launch-t693-op.sh are the IDENTICAL invocation shape
  (only prompt wording / worktree / log path differ) — the launcher file is
  excluded as the variable. The serve child DID spawn, so the failure is
  downstream of process spawn.
- No DAFG gate check applies to the agent (it filed nothing); the driver
  ran G694-1/G694-2 CHECK bytes verbatim at certification.

## Successor

T695 re-flight on the Route A kernel via a DIFFERENT launch route, launched
the same turn by the driver — only after the route proves it starts an agent
on a trivial probe. NOT another opencode --standalone --prompt launch.
