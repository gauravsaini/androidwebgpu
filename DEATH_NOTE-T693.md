# DEATH NOTE — T693 reflight-routea-mac

Verdict: FAIL (honest, never-started). Certified by driver 2026-11-08 ~05:21 AEDT.

## What happened

- Launched 2026-11-08 02:17 AEDT on the Mac via opencode v2.2.5
  (`/opt/homebrew/bin/opencode --standalone --prompt <brief> --auto`,
  backgrounded, PID 29036, serve child 29037).
- Wedge trigger 05:17 AEDT (launch + 3h, locked timeout rule 2026-11-07):
  log frozen, zero steps. Fresh-verified wedged, killed (TERM), two-sample
  dead, no children, no orphan. No other opencode PID touched.
- Elapsed at kill: 3h03m. Log /Users/Shared/codex-logs/track693-opencode.log
  frozen at 62,057 B since 02:17:21 (only +121 B at 05:20:41 — terminal
  teardown ANSI on kill, not agent work). Tail is a pure ANSI repaint of the
  "New session" spinner + brief prompt text; zero step text, zero tool calls.
- No progress-t693.txt, no findings-t693.md, no outcome-t693.txt.
  heartbeat.json in the worktree is stale T673 (2026-11-03, hardlinked staging),
  not a T693 heartbeat.

## What this means

- 4th consecutive never-start: T690 (box, TUI loop, 8.3 GB log, disk 100%),
  T691 (box, log frozen 26,080 B), T692 (box, log frozen 27,512 B),
  T693 (Mac, log frozen 62,057 B). The launch path itself is broken, not the
  flight — the Route A Image (11adf236) was never flown, and the driver-run
  symbol gate (PASS at T692 certification: printk 0xffffff800811a4c4 below
  _etext) still stands unproven in flight.
- No DAFG gate check applies: FAIL certification needs no gates (gates apply
  to done/PASS only). No flight evidence exists to gate.

## Successor

T694 launch-path DIAGNOSIS lane (not a flight retry), launched the same turn
by the driver. Scope: why lanes never start — opencode version/behaviour,
--auto flag, prompt delivery, TUI loop, SSH session handling.
