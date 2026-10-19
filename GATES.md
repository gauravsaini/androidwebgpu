# Gates: Track T584 — rust-init-gap

OWNS: GATES.md, TIMELINE.md, heartbeat.json

Scope: Run the full 150M-step boot gate from reset on combined lineage 2d3d2bc, diagnose what lies between 110M steps and /init, probe the 3,055-byte UART silence anomaly, verify whether the engine reaches /init or identify the next wall, and deliver findings to /Users/Shared/agyx-track584-rust-init-gap.md.

- [x] G1: Base commit 2d3d2bc verified (combining 39fcc56 + 11beaa16) and branch t584-rust-init-gap checked out.
  CHECK: git -C /Users/Shared/wt-track583-rust-150m-combined rev-parse --short HEAD
  EXPECT: 2d3d2bc
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=2d3d2bc

- [x] G2: Clean build of run_kernel in isolated CARGO_TARGET_DIR target-t584.
  CHECK: test -f /Users/Shared/wt-track583-rust-150m-combined/target-t584/release/run_kernel && echo "EXISTS"
  EXPECT: EXISTS
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=EXISTS

- [x] G3: Unit and regression tests pass for sysreg_p3 suite.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track583-rust-150m-combined/target-t584 /opt/homebrew/bin/cargo test -p u12-orchestrator --test sysreg_p3 -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1` | Running tests/sysreg_p3.rs (target-t584/debug/deps/sysreg_p3-3b1f8fcb84f8d576)

- [x] G4: Full 150M-step boot gate executed from reset with pinned guest inputs and quirks disabled.
  CHECK: test -f /Users/Shared/agyx-logs/track584-rust-150m.log && echo "LOG_EXISTS"
  EXPECT: LOG_EXISTS
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=LOG_EXISTS

- [x] G5: Telemetry recorded: step count, PC progression, UART byte count + SHA, halt step/PC, 0 dynamic panics in 1 GiB guest RAM scan.
  CHECK: grep -E "Scanning 1 GiB guest RAM for panic strings" /Users/Shared/agyx-logs/track584-rust-150m.log
  EXPECT: Scanning 1 GiB guest RAM for panic strings
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=[ram-search] Scanning 1 GiB guest RAM for panic strings...

- [x] G6: Probe UART silence anomaly: compare samples 20M+ steps apart, check if 3,055 bytes truncate mid-line, identify guest activity at 110M+ steps.
  CHECK: grep -E "UART_SILENCE_ANALYSIS: COMPLETE" /Users/Shared/agyx-track584-rust-init-gap.md
  EXPECT: UART_SILENCE_ANALYSIS: COMPLETE
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=UART_SILENCE_ANALYSIS: COMPLETE

- [x] G7: Certified findings file /Users/Shared/agyx-track584-rust-init-gap.md delivered with Verdict PASS or FAIL.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track584-rust-init-gap.md
  EXPECT: Verdict:
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=2095eed8e20b/12 entries; output=Verdict: PASS — The full 150M-step boot gate on the combined fixed lineage (base merge commit `2d3d2bc`, branch `t584-rust-init-gap`, combining PSCI_FEATURES fix `39fcc56` + DC ZVA fix `11beaa16`) executed cold from reset cleanly through al
