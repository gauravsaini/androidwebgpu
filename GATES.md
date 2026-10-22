# Gates: Track T611 — Rust engine: sha256_finup CFI indirect-call failure (Chain B)

OWNS: units/u12-orchestrator/**, GATES.md, TIMELINE.md, scripts/supervisor-t611.py, outcome-t611.txt

Scope: Differential-trace sha256_finup CFI indirect call fault against QEMU, implement engine-side fix in u12-orchestrator, verify brk_vbar_regression, and execute 300M boot gate past step 208,640,789.

- [x] G611-1: Base commit 66906aab12b858971827086951903ba103e024d5 verified in worktree wt-track611-sha256-cfi-engine
  CHECK: cd /Users/Shared/wt-track611-sha256-cfi-engine && git merge-base HEAD t607-advsime-off
  EXPECT: 66906aab12b858971827086951903ba103e024d5
  EVIDENCE: automatic-evidence=v1; definition-sha256=e645e8d05c407ad542925c336a84bef80d90cea3f6cca38cae88c25b8766f96c; exit=0; EXPECT=matched; output-sha256=8af97b26fd8bb209fda39cdda86b4221cc4d27fa7b9c99e98710367d5155cd2a; output-bytes=41; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine; path=de7de1880035/13 entries

- [x] G611-2: Canonical guest inputs verified against SHA256-CANONICAL.md
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio | awk '{print $1}' | tr '\n' ' '
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38 
  EVIDENCE: automatic-evidence=v1; definition-sha256=43a4e585e2b98ac1e610d3782c833cf603579db61834883fe2003d7ee8a7b3f8; exit=0; EXPECT=matched; output-sha256=3eaf634b877170815c78778bff138ad71001f1ed73a0edf536f427c934cd7d56; output-bytes=195; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine; path=de7de1880035/13 entries

- [x] G611-3: Engine-side Clang CFI indirect-call handling implemented in u12-orchestrator
  CHECK: grep -q "Track T611: Handle Clang CFI assembly indirect-call check" /Users/Shared/wt-track611-sha256-cfi-engine/units/u12-orchestrator/src/lib.rs && echo CFI_ENGINE_FIX_OK
  EXPECT: CFI_ENGINE_FIX_OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=57e8c1ef9b4494aef9881a27ddde7e274243538a4612beb2f02464c5da4f6d50; exit=0; EXPECT=matched; output-sha256=2dee253c8337cec360123ce8f59cdf2ea757c80f4c03ad6af266748af19c0bea; output-bytes=18; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine; path=de7de1880035/13 entries

- [x] G611-4: Regression test suite brk_vbar_regression (5/5) passes
  CHECK: CARGO_TARGET_DIR=target-t611 cargo test --release --test brk_vbar_regression -- --nocapture
  EXPECT: test result: ok. 5 passed
  CWD: /Users/Shared/wt-track611-sha256-cfi-engine/units/u12-orchestrator
  EVIDENCE: automatic-evidence=v1; definition-sha256=a2accdfc6f846df538fdef5bc4fe304289bd230af06689d2b4406c664a5846d5; exit=0; EXPECT=matched; output-sha256=ce2302dcd3daf69287714aae371691de60f97b5e7632aa69febbaa1dd50682ce; output-bytes=11506; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine/units/u12-orchestrator; path=de7de1880035/13 entries

- [x] G611-5: Release build of run_kernel with CARGO_TARGET_DIR=target-t611
  CHECK: test -x /Users/Shared/wt-track611-sha256-cfi-engine/target-t611/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: automatic-evidence=v1; definition-sha256=cbba5ed27a52603ba21769e88acef4810fea8bae6038bd2110fc6d05deaca245; exit=0; EXPECT=matched; output-sha256=f1442cb880a74e4b1de1d178c5fd94b9f63149191c757d5cd429d653fead95f7; output-bytes=9; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine; path=de7de1880035/13 entries

- [x] G611-6: Long boot gate executed to 300M cap under supervisor past step 208,640,789
  CHECK: test -f /Users/Shared/outcome-t611.txt && grep -q "wall_type_tag:" /Users/Shared/outcome-t611.txt && echo BOOT_RECORDED
  EXPECT: BOOT_RECORDED
  EVIDENCE: automatic-evidence=v1; definition-sha256=6d85bf54882a956a69f0ab3e3ac0c505284c0f24051681edd594a63519585c4b; exit=0; EXPECT=matched; output-sha256=c75d951de10e5c11958f29ace7139a8c780d24f4ef1d6397fc7601975c7d67d9; output-bytes=14; shell=/bin/sh; cwd=/Users/Shared/wt-track611-sha256-cfi-engine; path=de7de1880035/13 entries

- [ ] G611-7: Findings artifact delivered with Verdict and required tags, TIMELINE updated
  CHECK: test -f /Users/Shared/agyx-track611-sha256-cfi-engine.md && grep -q "WALL-TYPE-TAG:" /Users/Shared/agyx-track611-sha256-cfi-engine.md && grep -q "t611-sha256-cfi-engine" /Users/Shared/wt-track611-sha256-cfi-engine/TIMELINE.md && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: pending
