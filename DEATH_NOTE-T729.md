# Death note — Track T729 (RECOVERY-11)

- Flight `service-flight`: full 600 s guest / 601.43 s host, QEMU exit 0,
  UART 18,375 lines / 2,034,071 B, SHA
  `2679eb703de1abaed2c12c956b9a4e61b1e34ef764d2ed3a18eb6290517f4b7b`.
- Verdict: FAIL. `T728_SERVICE_OK`: 0. The labeled payload never ran.
- Cause of death: the trigger never fired — `sys.boot_completed` never
  became 1 in 600 s, so the `on property:sys.boot_completed=1` action
  (`start t728probe`) never ran. No SELinux refusal (0 lines): init was
  never asked to start the binary, so the label fix is UNTESTED, not refuted.
- Boot reached `ams_ready` at guest 555.040 s (T728: 484.500 s), then
  system_server entered a watchdog dump storm (pre-watchdog crash file
  written); at T728's pace boot_completed would land at ~634 s, past the
  600 s window.
- Wall: BOOT-COMPLETED-NEVER-FIRED (guest). Rust par bhi lagu: haan.
- Successor: re-fly the same labeled image (`568af6ac`) with a longer guest
  window (recommend 700 s guest target) or a faster boot. No rebuild needed.
- Findings: `findings-t729.md`. Outcome: `outcome-t729.txt` (`FAIL`).
  Analyzer output: `flights/service-flight/analysis-t729.txt`.
