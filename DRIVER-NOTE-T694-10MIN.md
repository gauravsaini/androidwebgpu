# DRIVER NOTE — T694 10-minute start check (driver-recorded, agent never started)

Checked 2026-11-08 05:37:45 AEDT, 10m36s after launch (05:27:09 AEDT).

- PID 34237 ALIVE (0.2% CPU), full T694 prompt cmdline, serve child unknown.
- Log /Users/Shared/codex-logs/track694-opencode.log FROZEN at 20,875 B
  since 05:27:15. Tail is TUI menu chrome ("/connect", "Switch model",
  "/models") — the opencode TUI painted its welcome screen and stopped.
- NO progress-t694.txt, NO heartbeat.json. Brief Step 0 (prove alive within
  5 minutes) missed; brief 10-minute self-exit guard not executed.
- Verdict at 10 min: NEVER STARTED — same signature as T693 (log frozen
  62,057 B, spinner + prompt repaint, zero tool calls).

This is the 5th consecutive never-start: T690 (box, TUI loop), T691 (box),
T692 (box), T693 (Mac), T694 (Mac, diagnosis lane itself). The failure is
in the launch path, not the mission: even a diagnosis brief with an explicit
"prove you are alive first" step cannot get the agent to execute one tool
call. Strongest remaining hypotheses: opencode v2.2.5 TUI never leaves its
menu/spinner state under nohup + stdin /dev/null + --auto, or the
"experiential" provider/model never answers the first request.

Actions taken by driver: NO duplicate launched, NO kill. Wedge trigger
~08:27 AEDT (launch + 3h) per the locked timeout rule; driver owns the
kill/certify call. This lane's own Step 4 probes never ran — the diagnosis
itself is blocked by the same defect it was launched to diagnose.
