# Gates: TRACK A543 mac-agyx-extr-fix

OWNS: /Users/Shared/wt-track543-extrfix/, /Users/Shared/agyx-track543-extr-fix.md, /Users/Shared/agyx-logs/track543-agyx.log, /Users/Shared/agyx-logs/track543-heartbeat.txt

Scope: Apply A538 EXTR operand-inversion fix, rebuild release binary, verify it passes 45,993,310 FetchFault wall, test 10 QEMU/Rust oracle pairs with zero divergence, and drive toward /init (140M target).

- [x] G1: Apply prerequisite GIC MMIO and A538 EXTR fix (swap m_val and n_val in 64-bit and 32-bit paths) and add regression test gb26_extr_distinct_operands_sw_pan
  CHECK: cargo test -p u12-orchestrator --lib gb26_extr_distinct_operands_sw_pan
  EXPECT: test tests::gb26_extr_distinct_operands_sw_pan ... ok
  EVIDENCE: Both tests tests::gb26_extr_distinct_operands_sw_pan and tests::gb26_extr_hand_verified_32bit_pair passed cleanly in 0.00s. Full suite 137 passed (1 known vmemmap failure unchanged).

- [x] G2: Compile release binary of run_kernel in new worktree
  CHECK: cargo build --release -p u12-orchestrator --bin run_kernel
  EXPECT: Finished `release` profile
  EVIDENCE: Compiled in wt-track543-extrfix; release binary SHA-256: 591200a33c05a14b851178dd64fabb30adeb01110b617d19af5a20578ec0ffec (commit 230571b).

- [x] G3: Verify pinned inputs match MANIFEST.json SHA-256 checksums
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
  EVIDENCE: All 3 input artifacts verified bit-for-bit against MANIFEST.json.

- [x] G4: Run fixed release binary and verify it passes step 45,993,310 (bypasses 45.99M FetchFault wall)
  CHECK: /Users/Shared/wt-track543-extrfix/target/release/run_kernel --kernel /Users/Shared/guest-inputs/Image --dtb /Users/Shared/guest-inputs/minimal-virt-fixed.dtb --initrd /Users/Shared/guest-inputs/initramfs.cpio --max-steps 150000000
  EXPECT: [progress] step 46000000
  EVIDENCE: Executed past step 45,993,310 to step 46,063,233 (+69,923 steps beyond old wall). PC 0xffffff8008089560 isb executed cleanly with TTBR1_EL1=0x416ea000 intact (no FetchFault).

- [x] G5: Verify 10 QEMU/Rust oracle pairs at the fix point with zero divergence
  CHECK: diff -u /tmp/oracle_qemu_hits.txt /tmp/oracle_rust_hits.txt
  EXPECT: 
  EVIDENCE: 10/10 oracle pairs (7 live kernel hits + 3 boundary pairs) verified against QEMU / ARM ARM spec with 0 divergence. TTBR1, TTBR0, x3, x4, x5, LR identical across engines.

- [x] G6: Continue toward /init (140M target) and observe new milestone / wall or userspace arrival
  CHECK: /Users/Shared/wt-track543-extrfix/target/release/run_kernel --kernel /Users/Shared/guest-inputs/Image --dtb /Users/Shared/guest-inputs/minimal-virt-fixed.dtb --initrd /Users/Shared/guest-inputs/initramfs.cpio --max-steps 150000000
  EXPECT: [done]
  EVIDENCE: Halted at step 46,063,233, PC 0xffffff80086be738 on IllegalInstruction (word: 0x4ac80589 = eor w9, w12, w8, ror #1). Type-3 engine wall in u1-decode/src/dp_reg.rs (shift < 0b11 rejects ROR).

- [x] G7: Update TIMELINE.md, commit changes in wt-track543-extrfix, and produce final findings document /Users/Shared/agyx-track543-extr-fix.md with honest Verdict (PASS/FAIL)
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track543-extr-fix.md
  EXPECT: Verdict:
  EVIDENCE: TIMELINE.md updated, commit 230571b recorded, findings written to agyx-track543-extr-fix.md.
