# Findings — Track T689: Kernel rebuild per the T688 recipe + re-flight (box)

RECONSTRUCTED BY REVIEWER — driver-reconstructed 2026-11-07. The T689 agent
wedged after the Route A build and filed no findings, outcome, or death
note; this file is reconstructed by the driver from driver-verified
evidence (opencode log, progress file, heartbeat, build log, built
artifacts, kill verification), per RECONSTRUCTED-TAG. Every SHA, size,
address, and timestamp cited here was re-hashed or re-read by the driver
on the box on 2026-11-07; no value is cited from the agent's word alone.

Verdict: FAIL

## Summary

T689 executed the T688 rebuild recipe's Route A (config-only LTO restore)
and completed the kernel build, then the agent wedged: its log, progress
file, and heartbeat all froze at 13:39 AEDT or earlier, and it never ran
the mandatory pre-boot symbol gate, never launched a flight, and never
filed a verdict. The parent driver killed the agent at ~16:34 AEDT under
the locked timeout rule (log frozen >2x the minutes-scale expected step
time for the symbol gate AND elapsed 03:03:40 past the 3h mark).

The Route A kernel Image is BUILT and REUSABLE: BUILD_EXIT=0, Image SHA
11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367
(20,384,256 B). Whether it fixes the T686 boot hang is UNPROVEN — the
symbol gate and the re-flight are T690's job. No flight, no guest boot,
no frame-commit claim in this lane.

First failing link: agent wedged after the Route A build — the mandatory
pre-boot symbol gate (Step 4) was never run and no flight (Step 6) was
ever launched.

## Steps

### Step 0 — Box disk gate: PASS

`df /home/muse /dev/sda3 143G 123G used 13G avail (>= 8G)` at
2026-11-07T02:34:07Z (progress-t689.txt line 0, agent-recorded,
driver re-read). Gate passed; build and staging proceeded.

### Step 1 — Verify all inputs + read the T688 record: PASS

All 8 staged guest inputs SHA-verified by the agent (progress-t689.txt
line 1); toolchain clang 21.0.0 / LLD 21.0.0 present; rebuild-recipe.md
read. Driver re-verified the staged-input SHAs at certification via the
vault and the T689 brief (t686-config 502e79c3, canonical config
d840310a, T686 Image 66d3fc1b6, canonical Image 05c43917, modified DTB
e83a6437, T683 fix initramfs 8fb3c9de, system.img 98fc0b02, T681 ELF
ddc1e4dd).

VARIANCE (openly recorded): findings-t688.md was ABSENT on the box
during T689 staging (progress-t689.txt line 1: "findings-t688.md ABSENT
(not found on box)"). The agent worked from rebuild-recipe.md (SHA
63ace6451ef89e34315a9a93bd7333e31e3ea7e0fa9a3bec86b998201ca28f2b),
which is the authoritative source for the rebuild; the missing findings
file did not block the build. T690 must stage findings-t688.md from the
box vault (evidence/findings-t688.md) before use.

### Step 2 — Source worktree at the exact commit: PASS

Source worktree at 4da740c10dc0f2de632686e8cc0274cfa52e67e4, detached
HEAD, clean, 921M (progress-t689.txt line 2, 02:34:59Z). Driver
re-confirmed the worktree exists at /home/muse/t689-rebuild/src.

### Step 3 — Route A: config-only LTO restore: BUILD DONE

- Config base: guest-inputs/t686-config as .config; Route A delta
  applied; olddefconfig run (out/olddefconfig-routeA.log, 47 B).
- LTO restore SURVIVED olddefconfig (driver-grepped
  out/routeA.config, SHA
  24ae17cc59f823a8c8702fed98d44093d5dd5447f283d22f1da649eb83e6251c,
  135,317 B):
  `CONFIG_LTO=y`, `CONFIG_THINLTO=y`,
  `# CONFIG_LTO_NONE is not set`, `CONFIG_LTO_CLANG=y`.
- Optional full hardening parity NOT applied:
  `# CONFIG_CFI_CLANG is not set`,
  `# CONFIG_SHADOW_CALL_STACK is not set` (same grep). The agent did
  not record a reason; the LTO trio alone is the Route A test.
- Build: BUILD_END=2026-11-07T02:41:07Z, BUILD_EXIT=0
  (out/build.log, 17,689 B, SHA
  2626c3a732fa0690ebae9513e6b9733d187f98b05a72f9e39cb36ad9c9009460;
  out/build-routeA.stdout, 13 B, contains `BUILD_EXIT=0`). Build log
  tail shows warnings only (drivers/usb/gadget/function/f_accessory.c,
  configfs.c, drivers/clk/clk.c, kernel/sched/core.c;
  `lib/nmi_backtrace.o: no symbols`); no errors.
- Built kernel (driver-verified 2026-11-07):
  - `src/arch/arm64/boot/Image`, 20,384,256 B, SHA
    11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367,
    `Linux kernel ARM64 boot executable Image, little-endian, 4K pages`.
  - `src/vmlinux`, 307,656,840 B, SHA
    4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5.
  - Version string (strings on the Image):
    `Linux version 4.19.130-g4da740c10 (muse@gsai) (Android
    (13989888, +pgo, +bolt, +lto, +mlgo, based on r563880c) clang
    version 21.0.0, LLD 21.0.0) #2 SMP PREEMPT Sat Nov 7 13:40:51
    AEDT 2026`.

VARIANCE (openly recorded): the brief states the built kernel goes to
`out/Image`; the actual build output is
`src/arch/arm64/boot/Image` (the Image target lands in the source
tree's arch/arm64/boot/). `out/` holds build.log, build-routeA.stdout,
olddefconfig-routeA.log, and routeA.config only. Layout variance, not
a defect: the Image is built, verified, and reusable at the src/ path.
T690 must use the src/ path (or copy it to out/Image itself and
re-hash).

### Step 4 — Pre-boot symbol gate: NEVER RUN

No symbol-gate output exists anywhere in the workdir: no gate log, no
`llvm-nm` output, no recorded addresses for printk / __die / panic /
dump_stack / show_regs vs _etext / __init_begin / __init_end. The
agent's last progress line is Step 2 (02:34:59Z); the build finished
02:41:07Z with no progress line for it and no gate after it. The gate
is T690's first step; no flight of this Image may be claimed as
symbol-gate-approved.

### Steps 5–8 — Route B / re-flight / Route C / A/B + TEN-PASSES: NOT REACHED

flights/ is EMPTY (driver-verified). No flight supervisor run, no UART,
no A/B control, no TEN-PASSES table. The T686 hang question — whether
the Route A Image removes the cold `.text.unlikely.` orphan — is
UNANSWERED in this lane.

## Wedge record (driver-verified 2026-11-07 ~16:34 AEDT)

- Agent: box opencode PID 3112135, serve child 3112145, launched
  13:30 AEDT (02:30Z), elapsed 03:03:40 at verification, state Sl,
  0.2% CPU, parked.
- Log /home/muse/t689-rebuild/opencode-t689.log: 10,458,480 B, mtime
  13:39 AEDT, frozen ~2h54m at verification (+121 B at 16:34 from the
  shutdown trace only, 10,458,601 B).
- heartbeat.json frozen at 2026-11-07T02:37:00Z ("step 2 complete").
- progress-t689.txt frozen at Step 2 (last line 02:34:59Z).
- No make/qemu processes at kill time. No findings, outcome, or death
  note filed by the agent.
- Kill: parent driver, TERM to 3112135 + 3112145 at ~16:34 AEDT; both
  gone on re-check, no orphan, no -9. No other process touched: old box
  opencode lingers T680 PID 742044, T681 PID 768083, T686 PID 2862071
  are recorded variances from their own certifications, NOT T689
  duplicates.
- Wedge condition under the locked timeout rule (AGENTS.md, 2026-11-07):
  log frozen >2x the minutes-scale expected step time for the symbol
  gate AND elapsed past the 3h mark — both prongs met. Kill +
  certify honest FAIL + launch successor the same turn, autonomously,
  no options presentation.

## Adjudication

- Winning route: NONE. Route A built; Route B and Route C not reached.
- T686 first failing link (restated from T688): cold
  `.text.unlikely.` orphaned into [__init_begin,__init_end] →
  free_initmem zeroes it → `bl printk` executes `udf #0x0` → nested
  die() self-deadlocks on die_lock. Whether the Route A Image removes
  it is UNPROVEN (symbol gate + flight = T690).
- `Freeing unused kernel memory` and `init first stage` were NEVER
  observed: no flight ran, so neither marker could appear. No
  `sys.boot_completed` observed: sys.boot_completed NOT observed (no
  flight, no UART, no guest boot).
- No frame-commit claim: this lane proves nothing about display; the
  frame commit by guest-origin evidence remains a later lane's job.
- Regressions: no DTB/initramfs/system.img/ELF modification in this
  lane; the T686 config base and canonical inputs are untouched (the
  build ran in a fresh git worktree of the pristine source tree).
- sys.boot_completed: NOT observed.

WALL-TYPE-TAG: guest type-1 — agent wedge on a guest-kernel rebuild
lane; the underlying T686 boot-hang wall is guest type-1 (shared with
Rust: the same guest kernel is required regardless of implementation
language).

Rust par bhi lagu: haan — the same guest kernel is required regardless
of implementation language; the wedge itself is a lane-execution wall,
not a guest wall, but the unproven Route A fix targets a guest type-1
wall.

## Evidence inventory (all driver-verified on the box, 2026-11-07)

- progress-t689.txt, 421 B (3 lines, Steps 0–2 PASS, last 02:34:59Z).
- heartbeat.json, 117 B (02:37:00Z, "step 2 complete").
- opencode-t689.log, 10,458,601 B (frozen 13:39, +121 B shutdown
  trace at 16:34).
- out/build.log, 17,689 B, SHA
  2626c3a732fa0690ebae9513e6b9733d187f98b05a72f9e39cb36ad9c9009460.
- out/build-routeA.stdout, 13 B (`BUILD_EXIT=0`).
- out/routeA.config, 135,317 B, SHA
  24ae17cc59f823a8c8702fed98d44093d5dd5447f283d22f1da649eb83e6251c.
- out/olddefconfig-routeA.log, 47 B.
- src/arch/arm64/boot/Image, 20,384,256 B, SHA
  11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367.
- src/vmlinux, 307,656,840 B, SHA
  4415ed77d8f31c300eb462b739a2c3b9770aa9d48d99ea767868b6cf6b2f51a5.
- brief-t689.txt, 20,265 B (SHA c2df2420… per launch record).
- rebuild-recipe.md, 10,923 B, SHA 63ace6451ef89e34315a9a93bd7333e31e3ea7e0fa9a3bec86b998201ca28f2b.
- flights/ EMPTY. No findings/outcome/death-note filed by the agent.
