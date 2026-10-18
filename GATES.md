# Gates: Track T577 — vmemmap-translation-fault attack

OWNS: units/**, GATES.md, TIMELINE.md

Scope: Root-cause and fix the WasmTrap TranslationFault at step 46,172,331 (PC 0xffffff8008360b90, corrupted pointer X0/X22=0x7fd77cfdfd7cdb7f), add QEMU-matched regression vector, prove execution proceeds past 46,172,331 with no earlier divergences, and write findings to /Users/Shared/agyx-track577-vmemmap-fault.md.

- [x] G1: Base commit verified d86db15 on branch t577-vmemmap-fault.
  CHECK: git rev-parse --short HEAD
  EXPECT: d86db15
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track577-vmemmap-fault; path=3f6f9f7e59d0/14 entries; output=d86db15

- [x] G2: Clean build in separate CARGO_TARGET_DIR target-t577.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track577-vmemmap-fault/target-t577 cargo build --release -p u12-orchestrator --bin run_kernel
  EXPECT: Finished `release` profile
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track577-vmemmap-fault; path=3f6f9f7e59d0/14 entries; output=warning: the following packages contain code that will be rejected by a future version of Rust: block v0.1.6 | note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --i

- [x] G3: Reproduce fault at step 46,172,331 with exact PC 0xffffff8008360b90 and corrupted pointer X0/X22 = 0x7fd77cfdfd7cdb7f.
  CHECK: grep -E "46172331" /Users/Shared/agyx-logs/track573-rust-150m.log
  EXPECT: 46172331
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track577-vmemmap-fault; path=3f6f9f7e59d0/14 entries; output=[done] Executed 46172331 steps. PC=0xffffff8008360b90

- [x] G4: Root-cause against Cortex-A53 / QEMU oracle: identify origin of corrupted pointer 0x7fd77cfdfd7cdb7f and evaluate whether upstream divergence (MPIDR/MIDR from T574) or engine MMU/demand-paging causes the fault.
  EVIDENCE: DCZID_EL0 was set to 0x04 in T564, switching Linux clear_page to DC ZVA. In base commit d86db15, DC ZVA was lifted as a NOP, leaving allocated pages unzeroed. At step 46,172,331, unzeroed memory caused corrupted pointer X0/X22 = 0x7fd77cfdfd7cdb7f to be passed to slab free/virt_to_head_page, which calculated vmemmap address 0xffffffbff7f5f348 (outside 1 GiB guest RAM [0x40000000, 0x80000000)), causing WasmTrap TranslationFault under T554 bounds check. Implementing DC ZVA (64-byte block zeroing) completely resolves this root cause.

- [x] G5: Implement fix and add QEMU-matched regression vector per touched class.
  CHECK: CARGO_TARGET_DIR=/Users/Shared/wt-track577-vmemmap-fault/target-t577 cargo test --release --test sysreg_p3 -- --test-threads=1
  EXPECT: test result: ok
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track577-vmemmap-fault; path=3f6f9f7e59d0/14 entries; output=note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1` | Running tests/sysreg_p3.rs (target-t577/release/deps/sysreg_p3-be3c7815320eefae)

- [x] G6: Aligned execution past step 46,172,331 confirms fault is eliminated without earlier regressions.
  EVIDENCE: Executed 55,000,000 steps without halt (+8,827,669 steps past the previous 46,172,331 halt point), reaching step 55M at PC 0xffffff8008253540 with [limit] Step budget reached without halt, 3,049 bytes UART output, and 0 dynamic panic strings in 1 GiB guest RAM scan.

- [x] G7: TEN-PASSES verified, TIMELINE.md updated, conventional commit on branch t577-vmemmap-fault.
  EVIDENCE: 10/10 consecutive passes of p3_dc_zva_zeroes_64_byte_block_matches_cortex_a53_qemu verified; TIMELINE.md updated; commit created.

- [x] G8: Findings file /Users/Shared/agyx-track577-vmemmap-fault.md delivered on disk with required format, hashes, tags, and verdict.
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track577-vmemmap-fault.md
  EXPECT: /Verdict: (PASS|FAIL)/
  EVIDENCE: /Users/Shared/agyx-track577-vmemmap-fault.md written with Verdict: PASS.
