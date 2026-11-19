# DEATH NOTE — Track T730

- Track: T730 (labeled-payload re-fly, 700 s guest window). Base a006993,
  branch t730, brief a2de6960.
- Flight: service-flight 20:12:42–20:24:28 AEDT 19 Nov 2026. Stop: "Guest
  target reached (705.1s >= 700s)". Host 706.04 s, guest 705.059 s, QEMU
  exit 0. UART 33,588 lines, SHA
  `f400ac5e1ea791bd8a9e6d45693dfc231607a67509a39e0966c58fe0765f752e`.
- Verdict: FAIL (`outcome-t730.txt`, 5 bytes).
- Cause of death: the trigger fired (sys.boot_completed=1 at guest
  495.717 s) and the property action's write executed (T728_PROBE_OK #2 at
  guest 500.955 s), but the `start t728probe` in the same action produced
  zero observable effect — no start line, no exit line, no refusal, no rc
  citation. T728_SERVICE_OK 0, argv 0. Wall: SERVICE-START-SILENTLY-FAILED
  (guest) — the T726 wall, recurring. The label is still UNTESTED (never
  refused, never started); the label is ruled out as the explanation for
  the silence.
- Secondary: first system_server (pid 2885) died mid-flight (watchdog dump
  storm from guest ~457 s, DeadSystemException cascade at guest ~548 s);
  restart as pid 4358 reached ams_ready at guest 652.932 s. The death does
  not explain the silent start (start failed at ~501 s, before the death).
- Next: the successor must discriminate the start leg itself (on-boot
  direct start, or a manual `start` from the shell) and chase init's
  missing t728probe.rc:12 processing-line gap.
- Evidence: `findings-t730.md`, `flights/service-flight/` (flight-end.json,
  preflight.json, qemu-command.txt, qemu-stderr.log, uart.log + .gz +
  .numbered.txt, analysis-t730.txt).
