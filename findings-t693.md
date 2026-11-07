# Findings — T693 reflight-routea-mac (driver-reconstructed)

**RECONSTRUCTED-TAG**: the lane agent never started, so this file was
reconstructed by the driver from launch/kill evidence. No agent-written
findings exist.

Verdict: **FAIL** (honest, never-started). Certified 2026-11-08 ~05:21 AEDT.

## Evidence

| Item | Value |
|---|---|
| Launch | 2026-11-08 02:17 AEDT, Mac, opencode v2.2.5, `--standalone --prompt <brief> --auto`, backgrounded via macssh |
| PID | 29036 (serve child 29037); killed at wedge trigger 05:17 AEDT, two-sample dead, no children |
| Elapsed | 3h03m |
| Log | /Users/Shared/codex-logs/track693-opencode.log, frozen 62,057 B since 02:17:21; +121 B at 05:20:41 is terminal-teardown ANSI on kill |
| Log tail | ANSI repaint of "New session" spinner + brief prompt text only; zero step text, zero tool calls |
| progress-t693.txt | absent |
| findings-t693.md | absent (this file is the driver reconstruction) |
| outcome-t693.txt | absent |
| heartbeat.json | stale T673, 2026-11-03, 481 B, pre-launch staging — not a T693 heartbeat |
| Disk at launch | 15 Gi free |

## Never-start chain (4 consecutive)

| Track | Host | Signature |
|---|---|---|
| T690 | box opencode | TUI loop, log 8,314,233,756 B, box disk to 100% |
| T691 | box opencode | log frozen 26,080 B, zero tool calls in 3h02m |
| T692 | box opencode | log frozen 27,512 B, "New session" spinner, exited on its own ~01:50 AEDT |
| T693 | Mac opencode | log frozen 62,057 B, "New session" spinner, zero steps in 3h03m |

Pattern: the agent process stays alive (idle, ~0.0–0.1% CPU) but never
executes the brief. T693 proves the failure is NOT box-specific and NOT
fixed by moving to the Mac: same signature on Mac opencode v2.2.5 with the
proven T682/T687/T688 route. Candidate variables for T694: opencode
version/behaviour, the `--auto` flag, prompt delivery, TUI loop, SSH
session handling.

## What was NOT done

- No kernel rebuild (forbidden in this lane; none attempted).
- No flight on Mac QEMU 11.1.2 — the Route A Image (11adf236) remains unflown.
- No symbol-gate re-run by the agent; the driver-run gate from T692
  certification (PASS: printk 0xffffff800811a4c4, __die 0xffffff8008096bbc,
  panic 0xffffff80080b7770, dump_stack 0xffffff8008b41a64, show_regs
  0xffffff800808ec28, all below _etext 0xffffff8008b70000) still stands.
- No DAFG gate check: FAIL certification needs no gates.

## Successor

T694 launch-path diagnosis lane, launched the same turn. Diagnosis only:
no guest flight, no kernel rebuild, no flight retry, no QEMU/emulator run.

## Certification addendum (driver, 2026-11-08 ~08:35 AEDT)

Added by the certifying driver completing the T693 remainder. The
reconstructed findings above (written 05:24) lacked the lines the DAFG
gates require; nothing above this section was changed. The two "No DAFG
gate check" lines above were wrong: FAIL certification DOES run the
gates (T690/T691/T692 precedent — all driver-certified with GATES.md
[x]). The literal verdict below restates the same FAIL verdict in the
canonical form the gate CHECK greps for (the original line uses markdown
bold, which the CHECK does not match — T680 variance class).

Verdict: FAIL

## Markers (all NOT observed — no flight)
- `Freeing unused kernel memory`: NOT observed.
- `init: init first stage started!`: NOT observed.
- `simple-framebuffer ... fb0: simplefb registered!`: NOT observed.
- sys.boot_completed: NOT observed (no flight, no guest log).
- TEN-PASSES: not run.

WALL-TYPE-TAG: agent/tooling — Mac opencode never-started, zero work executed; not a guest wall.
Rust par bhi lagu: nahi — no guest evidence at all; the guest kernel was never flown.
