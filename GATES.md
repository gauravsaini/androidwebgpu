# Gates: Track T612 — AND-Rd31-SP fix revival (Chain B, engine)

OWNS: units/u12-orchestrator/**, GATES.md, TIMELINE.md, scripts/supervisor-t612.py, outcome-t612.txt

Scope: Revive T486 engine fix for non-flag-setting logical-immediate AND with Rd=31 (SP/WSP write), verify via regression tests (red-green), build release run_kernel, and execute 300M gate boot past step 208,640,789.

- [x] G612-1: Base commit 7291be1f142db2e8ac526c850c8059497c5b2cb9 verified in worktree wt-track612-and-rd31-sp
  CHECK: cd /Users/Shared/wt-track612-and-rd31-sp && git rev-parse HEAD
  EXPECT: 7291be1f142db2e8ac526c850c8059497c5b2cb9
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp; path=de7de1880035/13 entries; output=7291be1f142db2e8ac526c850c8059497c5b2cb9

- [x] G612-2: Canonical guest inputs verified against SHA-256
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio | awk '{print $1}' | tr '\n' ' '
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38 
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp; path=de7de1880035/13 entries; output=05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38

- [x] G612-3: Red test: exact 64-bit AND-to-SP test fails on unpatched base
  CHECK: test -f /Users/Shared/wt-track612-and-rd31-sp/red-test.log && grep -q "RED_TEST_CONFIRMED_FAILED" /Users/Shared/wt-track612-and-rd31-sp/red-test.log && echo RED_OK
  EXPECT: RED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp; path=de7de1880035/13 entries; output=RED_OK

- [x] G612-4: AND-Rd31-SP engine fix implemented in u12-orchestrator/src/lib.rs
  CHECK: grep -q "Track T612: Logical immediate AND with Rd=31" /Users/Shared/wt-track612-and-rd31-sp/units/u12-orchestrator/src/lib.rs && echo AND_SP_FIX_OK
  EXPECT: AND_SP_FIX_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp; path=de7de1880035/13 entries; output=AND_SP_FIX_OK

- [x] G612-5: Green test: regression test suite and_sp_regression passes
  CHECK: CARGO_TARGET_DIR=target-t612 cargo test --release --test and_sp_regression -- --nocapture
  EXPECT: test result: ok. 6 passed
  CWD: /Users/Shared/wt-track612-and-rd31-sp/units/u12-orchestrator
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp/units/u12-orchestrator; path=de7de1880035/13 entries; output=note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1` | Running tests/and_sp_regression.rs (target-t612/release/deps/and_sp_regression-f24b7382841f090c)

- [x] G612-6: Release build of run_kernel with CARGO_TARGET_DIR=target-t612
  CHECK: test -x /Users/Shared/wt-track612-and-rd31-sp/target-t612/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track612-and-rd31-sp; path=de7de1880035/13 entries; output=BUILD_OK

- [ ] G612-7: 300M gate boot executed under supervisor past step 208,640,789
  CHECK: test -f /Users/Shared/outcome-t612.txt && grep -q "wall_type_tag:" /Users/Shared/outcome-t612.txt && echo BOOT_RECORDED
  EXPECT: BOOT_RECORDED
  EVIDENCE: pending

- [ ] G612-8: Findings artifact delivered with Verdict and required tags, TIMELINE updated
  CHECK: test -f /Users/Shared/agyx-track612-and-rd31-sp.md && grep -q "WALL-TYPE-TAG:" /Users/Shared/agyx-track612-and-rd31-sp.md && grep -q "t612-and-rd31-sp" /Users/Shared/wt-track612-and-rd31-sp/TIMELINE.md && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: pending
