# Track T730 — timeline (labeled-payload re-fly, 700 s guest window)
- 20:08:52 AEDT 19 Nov 2026: T730 launched by driver (brief a2de6960, wedge 23:08:52). Worktree /Users/Shared/wt-track730-700s-window, branch t730, base a006993.
- 20:12 AEDT 19 Nov 2026: Steps 0–3 PASS (see progress-t730.txt). Gate wide open (74,408,144 KiB free).
- 20:12 AEDT 19 Nov 2026: flight service-flight launched (supervisor 16308, QEMU 16312); guest target 700 s, host cap 800 s.
- 20:24:28 AEDT 19 Nov 2026: flight service-flight ended — guest target reached (705.1 s >= 700 s), host 706.04 s, QEMU exit 0, UART 33,588 lines SHA f400ac5e.
- 20:31 AEDT 19 Nov 2026: Step 5 adjudication — Verdict FAIL (T728_SERVICE_OK 0; trigger fired at 495.717 s, both registration legs proven, service never started silently; T726 wall recurring; label still UNTESTED). Findings + outcome + death note written; heartbeat CLOSED.
