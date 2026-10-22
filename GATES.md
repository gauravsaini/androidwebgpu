# Gates: Track T611 — Rust engine: sha256_finup CFI indirect-call failure (Chain B)

OWNS: units/u12-orchestrator/**, GATES.md, TIMELINE.md, scripts/supervisor-t611.py, outcome-t611.txt

Scope: Differential-trace sha256_finup CFI indirect call fault against QEMU, implement engine-side fix in u12-orchestrator, verify brk_vbar_regression, and execute 300M boot gate past step 208,640,789.

- [ ] G611-1: Base commit 66906aa2fa481e8b2ca33230c1e138a417637731 verified in worktree wt-track611-sha256-cfi-engine
  CHECK: cd /Users/Shared/wt-track611-sha256-cfi-engine && git rev-parse HEAD
  EXPECT: 66906aa2fa481e8b2ca33230c1e138a417637731
  EVIDENCE: pending

- [ ] G611-2: Canonical guest inputs verified against SHA256-CANONICAL.md
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio | awk '{print $1}' | tr '\n' ' '
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38 
  EVIDENCE: pending

- [ ] G611-3: Engine-side Clang CFI indirect-call handling implemented in u12-orchestrator
  CHECK: grep -q "Track T611: Handle Clang CFI assembly indirect-call check" /Users/Shared/wt-track611-sha256-cfi-engine/units/u12-orchestrator/src/lib.rs && echo CFI_ENGINE_FIX_OK
  EXPECT: CFI_ENGINE_FIX_OK
  EVIDENCE: pending

- [ ] G611-4: Regression test suite brk_vbar_regression (5/5) passes
  CHECK: CARGO_TARGET_DIR=target-t611 cargo test --release --test brk_vbar_regression -- --nocapture
  EXPECT: test result: ok. 5 passed
  CWD: /Users/Shared/wt-track611-sha256-cfi-engine/units/u12-orchestrator
  EVIDENCE: pending

- [ ] G611-5: Release build of run_kernel with CARGO_TARGET_DIR=target-t611
  CHECK: test -x /Users/Shared/wt-track611-sha256-cfi-engine/target-t611/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: pending

- [ ] G611-6: Long boot gate executed to 300M cap under supervisor past step 208,640,789
  CHECK: test -f /Users/Shared/outcome-t611.txt && grep -q "wall_type_tag:" /Users/Shared/outcome-t611.txt && echo BOOT_RECORDED
  EXPECT: BOOT_RECORDED
  EVIDENCE: pending

- [ ] G611-7: Findings artifact delivered with Verdict and required tags, TIMELINE updated
  CHECK: test -f /Users/Shared/agyx-track611-sha256-cfi-engine.md && grep -q "WALL-TYPE-TAG:" /Users/Shared/agyx-track611-sha256-cfi-engine.md && grep -q "t611-sha256-cfi-engine" /Users/Shared/wt-track611-sha256-cfi-engine/TIMELINE.md && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: pending
