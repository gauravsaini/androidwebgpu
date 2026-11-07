# Findings — T694 launch-path-diagnosis (driver-reconstructed)

Verdict: FAIL

RECONSTRUCTED-TAG: this file was reconstructed by the driver at
certification (2026-11-08 ~08:40 AEDT). The agent wrote no findings
of its own; every claim below is from driver-verified Mac evidence.

## Lane
- Track T694 launch-path-diagnosis, successor to driver-certified T693 honest FAIL.
- Route: Mac opencode v2.2.5. Agent PID 34237, serve child PID 34238
  (`opencode serve --stdio --port 0`).
- Launched 2026-11-08 05:27:09 AEDT, `--standalone --prompt <brief> --auto`,
  backgrounded via macssh (launcher `/Users/Shared/launch-t694-op.sh`, 682 B).
- Killed TERM at the wedge trigger: trigger ~08:27 AEDT (launch + 3h, locked
  timeout rule); kill executed 08:37:05 AEDT by the driver cycle that started
  08:30 AEDT. Elapsed at kill ~3h10m. Both PIDs gone on two-sample check
  (08:37:10 and 08:37:22); no orphan, no `-9`.
- This is the fifth consecutive never-start: T690 (box, TUI loop, 8.3 GB log),
  T691 (box, log frozen 26,080 B), T692 (box, log frozen 27,512 B, exited on
  its own), T693 (Mac, log frozen 62,057 B), T694 (Mac, log frozen 20,875 B).
  Two hosts, same launch shape, same result.

## Progress (brief steps)
- NO brief step executed at all. Step 0 (prove alive within 5 minutes) missed;
  the 10-minute start check (05:37:45 AEDT, driver-recorded) FAILED: PID alive
  0.2% CPU, log frozen, tail TUI menu chrome, NO progress-t694.txt, NO
  heartbeat.json. Step 4 (launcher comparison + controlled probes) never ran —
  the comparison was done by the driver at certification (see Evidence).
- Zero tool calls in 3h10m. No findings, outcome, death note, progress, or
  heartbeat written by the agent. The diagnosis itself is blocked by the same
  defect it was launched to diagnose.

## Evidence
- Agent log `/Users/Shared/codex-logs/track694-opencode.log`: frozen 20,875 B
  since 05:27:15; final 20,996 B after the kill (+121 B TUI teardown
  sequences, mtime 08:37). Tail is TUI menu chrome ("/connect",
  "Switch model", "/models") — the opencode TUI painted its welcome screen
  and stopped. The single "tool" string in the log is the brief's own
  wedge-guard text rendered in the TUI, not an executed call.
- Serve child 34238 DID spawn (`opencode serve --stdio --port 0`) — so the
  serve process started; the failure is downstream of process spawn: the agent
  never executed one tool call.
- Launcher comparison (driver-run at certification — T694 Step 4 never ran):
  `launch-t694-op.sh` (682 B, SHA 38a79b64…) vs `launch-t688-op.sh`
  (510 B, SHA 5f81e2f6…) vs `launch-t693-op.sh` (664 B, SHA 6ca22811…).
  IDENTICAL invocation shape in all three: cd worktree; nohup opencode
  --standalone --prompt "..." --auto > log 2>&1 < /dev/null &.
  Differences are only prompt wording, worktree, and log path. T688's agent
  executed under this shape on 7 Nov; T693/T694 never started under it on
  8 Nov. The launcher file is EXCLUDED as the variable.
- Remaining hypotheses (UNPROVEN, for the successor): opencode version drift
  (2.2.5 at T693/T694; the T688-era version is unrecorded — gap in the
  record); the provider/model never answering the first request; the TUI
  never leaving its menu/spinner state under nohup + stdin /dev/null +
  --auto; the serve-child handshake stalling after spawn.
- A re-probe `pgrep -af "track694"` hit (PID 39123) was the probe shell's
  own cmdline — gone on re-check, not a lane process; no kill.
- Known teardown lingers untouched: Mac PIDs 94999 (T687-recovery) and
  99833 (T688), both idle-alive with logs frozen since 7 Nov — not orphans,
  do NOT kill.

## Markers (all NOT observed — no flight; diagnosis only, flight out of scope)
- `Freeing unused kernel memory`: NOT observed.
- `init: init first stage started!`: NOT observed.
- `simple-framebuffer ... fb0: simplefb registered!`: NOT observed.
- sys.boot_completed: NOT observed (no flight, no guest log).
- TEN-PASSES: not run.

WALL-TYPE-TAG: agent/tooling — opencode never-started, zero work executed; not a guest wall.
Rust par bhi lagu: nahi — no guest evidence at all; the guest kernel was never flown.

## First failing link
- No guest link failed — the lane failed in the agent harness before any work:
  the agent never got past the TUI welcome screen, executed zero tool calls in
  3h10m. The driver-run symbol gate from T692 certification still PASSES
  (printk 0xffffff800811a4c4 below _etext 0xffffff8008b70000); the Route A
  Image (11adf236) remains unflown. The wall is the opencode launch path.

## Successor
- T695: re-flight on the Route A kernel via a DIFFERENT launch route — the
  route must first prove it starts an agent (progress file + tool calls
  within minutes) on a trivial probe before any flight lane is entrusted to
  it. Candidates: Mac agyx (ban lifted 29 Oct, Mac only) or an interactive
  opencode session. NOT another opencode --standalone --prompt launch.
