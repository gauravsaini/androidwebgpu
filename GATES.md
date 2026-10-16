# Gates: TRACK A543 mac-agyx-extr-fix

OWNS: /Users/Shared/wt-track543-extrfix/, /Users/Shared/agyx-track543-extr-fix.md, /Users/Shared/agyx-logs/track543-agyx.log, /Users/Shared/agyx-logs/track543-heartbeat.txt

Scope: Apply A538 EXTR operand-inversion fix, rebuild release binary, verify it passes 45,993,310 FetchFault wall, test 10 QEMU/Rust oracle pairs with zero divergence, and drive toward /init (140M target).

- [ ] G1: Apply prerequisite GIC MMIO and A538 EXTR fix (swap m_val and n_val in 64-bit and 32-bit paths) and add regression test gb26_extr_distinct_operands_sw_pan
  CHECK: cargo test -p u12-orchestrator --lib gb26_extr_distinct_operands_sw_pan
  EXPECT: test tests::gb26_extr_distinct_operands_sw_pan ... ok
  EVIDENCE: pending

- [ ] G2: Compile release binary of run_kernel in new worktree
  CHECK: cargo build --release -p u12-orchestrator --bin run_kernel
  EXPECT: Finished `release` profile
  EVIDENCE: pending

- [ ] G3: Verify pinned inputs match MANIFEST.json SHA-256 checksums
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
  EVIDENCE: pending

- [ ] G4: Run fixed release binary and verify it passes step 45,993,310 (bypasses 45.99M FetchFault wall)
  CHECK: /Users/Shared/wt-track543-extrfix/target/release/run_kernel --kernel /Users/Shared/guest-inputs/Image --dtb /Users/Shared/guest-inputs/minimal-virt-fixed.dtb --initrd /Users/Shared/guest-inputs/initramfs.cpio --max-steps 50000000
  EXPECT: [progress] step 46000000
  EVIDENCE: pending

- [ ] G5: Verify 10 QEMU/Rust oracle pairs at the fix point with zero divergence
  CHECK: diff -u /tmp/oracle_qemu_10hits.txt /tmp/oracle_rust_10hits.txt
  EXPECT: 
  EVIDENCE: pending

- [ ] G6: Continue toward /init (140M target) and observe new milestone / wall or userspace arrival
  CHECK: /Users/Shared/wt-track543-extrfix/target/release/run_kernel --kernel /Users/Shared/guest-inputs/Image --dtb /Users/Shared/guest-inputs/minimal-virt-fixed.dtb --initrd /Users/Shared/guest-inputs/initramfs.cpio --max-steps 150000000
  EXPECT: [progress]
  EVIDENCE: pending

- [ ] G7: Update TIMELINE.md, commit changes in wt-track543-extrfix, and produce final findings document /Users/Shared/agyx-track543-extr-fix.md with honest Verdict (PASS/FAIL)
  CHECK: grep -E "^Verdict: (PASS|FAIL)" /Users/Shared/agyx-track543-extr-fix.md
  EXPECT: Verdict:
  EVIDENCE: pending
