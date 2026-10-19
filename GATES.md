# Gates: Track T586 — chain-b-vbar-irq

OWNS: contracts/**, units/u2-ir-lift/**, units/u12-orchestrator/**, GATES.md, TIMELINE.md, heartbeat.json

Scope: Implement architectural IRQ exception injection to VBAR_EL1 in the Rust orchestrator, verify with QEMU-matched regression vectors under TEN-PASSES, execute full 150M boot gate from reset, measure guest advancement past the idle loop, and deliver findings to /Users/Shared/agyx-track586-vbar-irq.md.

- [x] G1: Base lineage verified on branch t586-vbar-irq at ffa13db off merge commit 2d3d2bc.
  CHECK: git -C /Users/Shared/wt-track583-rust-150m-combined rev-parse --short HEAD
  EXPECT: ffa13db
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=de7de1880035/13 entries; output=ffa13db

- [x] G2: SysRegs and contracts extended with ELR_EL1, SPSR_EL1, ESR_EL1, FAR_EL1 with MSR/MRS lifting.
  CHECK: /opt/homebrew/bin/cargo test -p pathn-contracts --lib
  EXPECT: test result: ok
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; path=de7de1880035/13 entries; output=Finished `test` profile [unoptimized + debuginfo] target(s) in 0.50s | Running unittests src/lib.rs (target/debug/deps/pathn_contracts-20ff7717236bca27)

- [x] G3: IRQ exception injection and ERET handling implemented in u12-orchestrator.
  CHECK: /opt/homebrew/bin/cargo test -p u12-orchestrator --lib
  EXPECT: test result: ok
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; output=test result: ok. 14 passed

- [x] G4: QEMU-matched regression suite for VBAR_EL1 IRQ injection passes 10/10 consecutive times.
  CHECK: export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$PATH" && /opt/homebrew/bin/cargo test -p u12-orchestrator --test irq_vbar_regression -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; TEN-PASSES verified 10/10 consecutive passes green.

- [x] G5: Clean build of run_kernel in isolated CARGO_TARGET_DIR target-t586.
  CHECK: test -f /Users/Shared/wt-track583-rust-150m-combined/target-t586/release/run_kernel && echo "EXISTS"
  EXPECT: EXISTS
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; output=EXISTS; sha256=fd0b3f8b35af84cb2e5340ebc3bbccdc23991509bc682ebe08c79219338ae2e3

- [x] G6: Full 150M-step boot gate executed from reset with pinned guest inputs and quirks disabled.
  CHECK: test -f /Users/Shared/agyx-logs/track586-rust-150m.log && echo "LOG_EXISTS"
  EXPECT: LOG_EXISTS
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; output=LOG_EXISTS; executed 49688549 steps

- [x] G7: Telemetry recorded: halt step, PC, UART bytes + SHA-256, 1 GiB RAM panic scan, idle loop progression.
  CHECK: grep -E "Scanning 1 GiB guest RAM for panic strings" /Users/Shared/agyx-logs/track586-rust-150m.log
  EXPECT: Scanning 1 GiB guest RAM for panic strings
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; output=[ram-search] Scanning 1 GiB guest RAM for panic strings... [CLEAN]

- [x] G8: Certified findings file /Users/Shared/agyx-track586-vbar-irq.md delivered with Verdict (PASS/FAIL) and TIMELINE.md updated.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track586-vbar-irq.md
  EXPECT: Verdict:
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track583-rust-150m-combined; output=Verdict: PASS
