# Gates: Track T599 — Rust engine: 300M boot gate toward /init (Chain B successor to T594)

OWNS: units/u12-orchestrator/src/bin/run_kernel.rs, scripts/supervisor-t599.py, GATES.md, TIMELINE.md, files/WALLS.md, outcome-t599.txt, PANIC_INVESTIGATION_T594.md

Scope: Execute 300,000,000 step cold reset boot gate under owned supervisor, enforce cap rules (distinct PCs last 10k, jiffies, /init grep), investigate T594 dynamic_panics, deliver findings.

- [x] G599-1: Lineage verification: worktree at /Users/Shared/wt-track599-rust-300m, branch t599-rust-300m, commit equals 2f75209804fbcc1b47d5a186f60da8107e293554
  CHECK: cd /Users/Shared/wt-track599-rust-300m && git rev-parse HEAD
  EXPECT: 2f75209804fbcc1b47d5a186f60da8107e293554
  EVIDENCE: verified `2f75209804fbcc1b47d5a186f60da8107e293554` on branch `t599-rust-300m`

- [x] G599-2: QEMU-REGRESSION-VECTORS: BRK vectoring regression suite (5/5 tests) passes under release build
  CHECK: cd /Users/Shared/wt-track599-rust-300m && CARGO_TARGET_DIR=target-t599 cargo test --release --test brk_vbar_regression -- --nocapture
  EXPECT: test result: ok. 5 passed
  EVIDENCE: verified 5/5 regression tests pass (EL1 imm=0x800 T592 wall, EL0 imm=0x1, imm=0x0, DAIF masking, ERET round-trip)

- [x] G599-3: Clean release build of run_kernel with CARGO_TARGET_DIR=target-t599
  CHECK: test -x /Users/Shared/wt-track599-rust-300m/target-t599/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: verified executable at `target-t599/release/run_kernel`

- [x] G599-4: Investigation and characterization of T594 dynamic_panics
  CHECK: test -f /Users/Shared/wt-track599-rust-300m/PANIC_INVESTIGATION_T594.md && grep -q "DYNAMIC_PANICS_ANALYSIS_OK" /Users/Shared/wt-track599-rust-300m/PANIC_INVESTIGATION_T594.md && echo INVESTIGATION_OK
  EXPECT: INVESTIGATION_OK
  EVIDENCE: verified `PANIC_INVESTIGATION_T594.md` characterizes count artifact (1 console panic + 4 RAM search matches) and confirms guest wall `clang-cfi-sha256-check-fail`

- [x] G599-5: Cold reset boot gate executed to 300M steps under owned supervisor with heartbeat and log capture
  CHECK: test -f /Users/Shared/wt-track599-rust-300m/outcome-t599.txt && grep -q "outcome:" /Users/Shared/wt-track599-rust-300m/outcome-t599.txt && echo BOOT_GATE_RECORDED
  EXPECT: BOOT_GATE_RECORDED
  EVIDENCE: verified `outcome-t599.txt` recorded 208,640,789 steps execution (halted at PC `0xffffff80081842c0` on `b .`, UART 13,503 bytes, 1,984 IRQs, 1 DA, 1 BRK)

- [x] G599-6: Mandatory cap rule telemetry: distinct PCs in last 10,000 steps, jiffies, and /init grep with byte offset recorded
  CHECK: grep -q "distinct_pcs_last_10k" /Users/Shared/wt-track599-rust-300m/outcome-t599.txt && grep -q "jiffies" /Users/Shared/wt-track599-rust-300m/outcome-t599.txt && echo CAP_RULE_OK
  EXPECT: CAP_RULE_OK
  EVIDENCE: verified distinct_pcs_last_10k=1894, jiffies=9, init_grep=not found

- [x] G599-7: Findings artifact delivered at /Users/Shared/agyx-track599-rust-300m.md with Verdict line FIRST, WALL-TYPE-TAG, and Rust par bhi lagu
  CHECK: test -f /Users/Shared/agyx-track599-rust-300m.md && head -n 1 /Users/Shared/agyx-track599-rust-300m.md | grep -q "Verdict:" && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: verified `/Users/Shared/agyx-track599-rust-300m.md` delivered with Verdict line FIRST, WALL-TYPE-TAG: guest, Rust par bhi lagu: haan
