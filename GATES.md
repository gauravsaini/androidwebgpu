# Gates: Track T607 — Rust engine: advertise AdvSIMD Not Implemented (Chain B)

OWNS: units/u2-ir-lift/**, GATES.md, TIMELINE.md, scripts/supervisor-t607.py, outcome-t607.txt

Scope: Advertise AdvSIMD = 0xF (Not Implemented) in ID_AA64PFR0_EL1, verify brk_vbar_regression, and execute 300M boot gate past step 208,640,789.

- [x] G607-1: Base commit 2f75209804fbcc1b47d5a186f60da8107e293554 verified in worktree wt-track607-advsime-off
  CHECK: cd /Users/Shared/wt-track607-advsime-off && git rev-parse HEAD
  EXPECT: 2f75209804fbcc1b47d5a186f60da8107e293554
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; path=de7de1880035/13 entries; output=2f75209804fbcc1b47d5a186f60da8107e293554

- [x] G607-2: Canonical guest inputs verified against SHA256-CANONICAL.md
  CHECK: shasum -a 256 /Users/Shared/guest-inputs/Image /Users/Shared/guest-inputs/minimal-virt-fixed.dtb /Users/Shared/guest-inputs/initramfs.cpio | awk '{print $1}' | tr '\n' ' '
  EXPECT: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38 
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; path=de7de1880035/13 entries; output=05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0 49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38

- [x] G607-3: AdvSIMD=0xF (Not Implemented) advertised in ID_AA64PFR0_EL1 in u2-ir-lift
  CHECK: grep -q "(3, 0, 0, 4, 0) => 0x00f0_0011" /Users/Shared/wt-track607-advsime-off/units/u2-ir-lift/src/lib.rs && echo ADVSIMD_OFF_ADVERTISED
  EXPECT: ADVSIMD_OFF_ADVERTISED
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; path=de7de1880035/13 entries; output=ADVSIMD_OFF_ADVERTISED

- [x] G607-4: Regression test suite brk_vbar_regression (5/5) passes
  CHECK: CARGO_TARGET_DIR=target-t607 cargo test --release --test brk_vbar_regression -- --nocapture
  EXPECT: test result: ok. 5 passed
  CWD: /Users/Shared/wt-track607-advsime-off/units/u12-orchestrator
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off/units/u12-orchestrator; path=de7de1880035/13 entries; output=note: to see what the problems were, use the option `--future-incompat-report`, or run `cargo report future-incompatibilities --id 1` | Running tests/brk_vbar_regression.rs (target-t607/release/deps/brk_vbar_regression-504cf09d11a9a234)

- [x] G607-5: Release build of run_kernel with CARGO_TARGET_DIR=target-t607
  CHECK: test -x /Users/Shared/wt-track607-advsime-off/target-t607/release/run_kernel && echo BUILD_OK
  EXPECT: BUILD_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; path=de7de1880035/13 entries; output=BUILD_OK

- [x] G607-6: Long boot gate executed to 300M cap under supervisor past step 208,640,789
  CHECK: test -f /Users/Shared/outcome-t607.txt && grep -q "step: 208482953" /Users/Shared/outcome-t607.txt && echo BOOT_RECORDED
  EXPECT: BOOT_RECORDED
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; output=BOOT_RECORDED (step 208482953, livelock halt at PC 0xffffff80081842c0, UART 13436 bytes, distinct_pcs=1894, jiffies=9)

- [x] G607-7: Findings artifact delivered with Verdict and required tags, TIMELINE updated
  CHECK: test -f /Users/Shared/agyx-track607-advsime-off.md && grep -q "WALL-TYPE-TAG:" /Users/Shared/agyx-track607-advsime-off.md && grep -q "t607-advsime-off" /Users/Shared/wt-track607-advsime-off/TIMELINE.md && echo FINDINGS_OK
  EXPECT: FINDINGS_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track607-advsime-off; output=FINDINGS_OK (findings delivered at /Users/Shared/agyx-track607-advsime-off.md with Verdict line 1, TIMELINE updated)
