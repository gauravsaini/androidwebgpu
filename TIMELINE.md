# TIMELINE: Android WebGPU Stack Milestones

- **Pass 1 (Architecture & POC)**: Setup core crates, Naga GLSL->WGSL compiler, basic buffer/texture resources, and initial E2E tests.
- **Pass 2 (Pipeline & Protocol)**: Implemented OASIS Virtio 1.2 commands, compositor matrix pipeline, dynamic VAO layout, depth/stencil attachments, scissor clipping, and APK analysis.
- **Pass 3 (Hardening & Complete Gap Remediation)**:
  - Fixed vertex layout memory leak and supported multi-buffer VBO bindings per attribute slot.
  - Implemented full uniform state machine (`glUniform1f`, `glUniform4fv`, `glUniformMatrix4fv`) and shader uniform buffer bind groups.
  - Fixed Virtio-GPU `ResourceFlush` subrect stride blit to scanout buffer.
  - Implemented Virtio-GPU WASM exports (`virtio_gpu_bridge/src/wasm.rs`) and v86 device driver.
  - Implemented concrete C++ guest drivers in `guest/patches/` (`gralloc.virtio_gpu.cpp`, `hwcomposer.virtio_gpu.cpp`, `egl_webgpu.cpp`, `Android.bp`).
  - Added full uniform reflection into WGSL `layout(std140, set=0, binding=2) uniform UniformBlock` struct.
  - Differentiated `GL_SHORT` normalized (`Snorm16`) vs integer (`Sint16`) attribute vertex formats.
  - Added `VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D` protocol decoding and dispatch.
  - Added HWC rotation/reflection matrix transformations (90°, 180°, 270°, flip-h, flip-v) and damage scissor clipping in `webgpu_compositor`.
  - Added `wgpu::Surface` target mode to `webgpu_swapchain`.
  - Wired live metrics tracking assertions into full-stack E2E tests.
- **Pass 4 (S9 Real APK Flight & Polish)**:
  - Created checked-in real APK fixtures (`fixtures/unity_cube.apk`, `fixtures/godot_gles2.apk`).
  - Implemented end-to-end APK GPU analyzer & Virtio-GPU Submit3D execution flight test (`crates/apk_gpu_analyzer/tests/apk_real.rs`).
  - Implemented DRM GEM buffer creation/mmap and command stream execbuffer in `guest/patches/egl_webgpu.cpp`.
  - Implemented canvas surface VSync presentation in `crates/webgpu_swapchain/src/swapchain.rs`.
  - Replaced heap attribute allocation with stack buffer in `pipeline.rs`.
- **Pass 5 (Visual Browser Test Bench & Live Chrome Validation)**:
  - Created interactive visual HTML5 test bench (`index.html`) with WebGPU canvas, dark-mode HUD, and 60 FPS Virtio animation loop.
  - Implemented JavaScript Virtio-GPU binary packet builder (`src/virtio_packet_builder.js`) conforming to OASIS 1.2 specification.
  - Implemented automated visual test suite runner (`src/test_suite.js`) with pixel-level RGB assertions.
  - Resolved `std::time::Instant` panic on `wasm32-unknown-unknown` by implementing platform-safe `PlatformInstant` via `js-sys::Date::now()` in `metrics_overlay`.
  - Configured WebGPU adapter limits and memory hints in `gles2wgpu/src/context.rs` to match browser WebAssembly environments.
  - Integrated `console_error_panic_hook` in `virtio_gpu_bridge/src/wasm.rs`.
  - Validated all 4 End-to-End Gates live in Chrome via Chrome DevTools Protocol (100% passed: 4/4):
    - Gate 1: 2D Scanout & Flush (PASSED)
    - Gate 2: 3D Submit GLES (PASSED)
    - Gate 3: Compositor & HUD (PASSED)
    - Gate 4: Real APK Flight Stream (PASSED)
  - Captured verified visual artifact screenshot and generated `walkthrough.md`.

## UPDATED ON : 2026-10-05

### fix (2026-10-05) — Track A1 Kernel Critical Path Halt-Chasing Beyond 150M to 200M Steps

1. **Kernel Halt-Chasing & Emulator Fixes**: Fixed CCMP register decode bit (bit 11 vs 10) resolving runaway `strchr` loop; added UDIV/SDIV decoding to `u1-decode` and `u12-orchestrator`; resolved LDP/LDPSW base register clobber (`rn == rt`) in `u2-ir-lift`; supported 64-bit ADD/SUB extended UXTB, 32/64-bit CLZ, 64-bit MADD/MSUB, and LDXP/LDAXP/STXP/STLXP atomic pairs; booted AOSP kernel to 200M steps halt-free with clean 1 GiB guest RAM panic search.
2. **Tests** (before → after): u1-decode 104 → 105 passed, u2-ir-lift 126 → 127 passed, u12-orchestrator 127 → 133 passed.
3. **Files changed**: `units/u1-decode/src/dp_reg.rs`, `units/u1-decode/src/lib.rs`, `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/src/lib.rs`, `units/u12-orchestrator/src/bin/run_kernel.rs`.

## T499 matched-oracle — 2026-10-15

- Started the matched-oracle lane and verified the actual `guest-inputs` hashes; pinned cortex-a53, 1024 MiB, exact DTB bootargs, input hashes, QEMU 11.1.2, and Rust source revision in `MANIFEST.json`.
- Matched the QEMU GIC version to the provided DTB's `arm,cortex-a15-gic` controller (GICv2). The first GICv3 run faulted before init because that machine setting contradicted the DTB; it was retained as raw setup evidence, not tagged as a guest wall.
- Native QEMU's guest-only UART reached `Run /init as init process`; the kernel unpacked the initramfs and PID 1 printed its rootfs and PASS markers.
- Captured the dynamic CSINV at `0xffffff8008354018` in both engines. The live second hit differs before the instruction (QEMU X8/NZCV `0xffffffffbfffffff`/`0xA`; Rust X8/NZCV `0x40000000`/`0x6`) and produces QEMU X24 `0x40000000` versus Rust X24 `0xffffffffbfffffff`.
- Matched-state control: QEMU and Rust execute the exact Image word `0xda818058` with identical relevant registers and PSTATE and both produce X24 `0x40000000`. The live divergence is in emulator state before CSINV, not the instruction's conditional-select result.
- Gated the memblock store rewrite and vmemmap-on-fault handler behind `--enable-guest-quirks`; default is off. Raw captures are in `evidence/t499/`; curated branch is `okf/branches/t499-matchedoracle.md`.

## T503 ORN fast-path fix — 2026-10-15

- Located the live T499 state divergence one instruction before CSINV: `ORN X8, XZR, X1` was executed as ORR because the U12 fast path ignored N.
- Fixed ORR/ORN for 32-bit and 64-bit forms; the focused test covers both ORR and ORN at each width and an LSL-shifted ORN form. Audited ANDS/BIC and the EOR/EON slow path.
- Rebuilt the release runner. With guest quirks disabled, Rust now matches pinned QEMU X8 and architectural PSTATE before CSINV and matches PC, X24, and PSTATE after it.
- Completed ten consecutive accepted QEMU/Rust live-state pairs with no mismatch. Raw logs and checksums are under `/Users/Shared/bundles/t503-ornfix-20261015/`.
- Recorded gates G503-1/2/3 as MET in `GATES-T503.md`; T503 is a separate OKF branch. Full workspace testing also reports the unrelated vmemmap LDRB/MMU translation failure.

## T506 fresh Rust PID-1 boot — 2026-10-16

- Opened a separate full-boot branch from T503's `ec0a017` fixed revision; all three T499 manifest input hashes match.
- Gate protocol requires guest quirks disabled, a guest-originating exact PID-1 UART marker, and ten consecutive fresh runs for any reproducibility claim. Boot outcome is pending.
- Carried forward T503's 349/350 workspace test result and its unrelated LDRB/MMU translation failure.

## UPDATED ON : 2026-10-16

### Feature (2026-10-16) — Harness UART streaming and PID-1 detection

1. **Harness UART & Diagnostics**: Added `--console-output` support, 2M progress cadencing with live `console_bytes` monitoring, stdout flushing, full console dumping, and exact PID-1 UART marker early break in `run_kernel.rs`.
2. **Tests** (before → after): Unstreamed buffered UART → Live console monitoring and automated PID-1 witness detection.
3. **Files changed**: `units/u12-orchestrator/src/bin/run_kernel.rs`

## UPDATED ON : 2026-10-16

### Diagnostics (2026-10-16) — T506 fresh Rust boot outcome & first divergence

1. **Boot Outcome & First Divergence**: Ran fresh boot from reset with pinned inputs (Image, DTB, initramfs) and quirks disabled. Guest executed 44,738,735 steps, emitting 1,950 UART bytes, then halted at PC `0xffffff800946a438` on `BRK #0x800` (`BUG()`). Divergence traced to `kasprintf` returning NULL in `X0` at `0xffffff800946a3d8` where QEMU returns allocated buffer `0xffffffc03d803000`. PID-1 marker not reached; Verdict FAIL.
2. **Tests** (before → after): Unverified boot outcome → Completed boot run, 44.74M steps measured, first divergence localized; G506-1 MET, G506-2 NOT MET, G506-3 MET.
3. **Files changed**: `GATES-T506.md`, `TIMELINE.md`, `codex-track506-rustinit.md`

## UPDATED ON : 2026-10-16

### fix (2026-10-16) — Fix UXTW and BFM lowering defects causing kasprintf NULL BUG()

1. **Instruction Lowering Fixes**: Fixed UXTW register-offset LDR lowering in `u2-ir-lift` to use `rm` instead of zeroed `SCRATCH` as second operand of AndShift; preserved BFM destination bits outside `tmask`/`wmask` in `u3-wasm-jit`. Resolves deterministic `kasprintf` NULL return and `BUG_ON(!name)` at PC `0xffffff800946a438` (step 44,738,735).
2. **Tests** (before → after): u2-ir-lift 126 → 127 passed (`ldr_reg_offset_uxtw_sxtw_lsl_addresses`); u12-orchestrator 133 → 134 passed (`gb4_bfxil_32bit_preserves_destination_bits_outside_field`).
3. **Files changed**: `units/u2-ir-lift/src/lib.rs`, `units/u3-wasm-jit/src/lib.rs`, `units/u12-orchestrator/src/lib.rs`.

## UPDATED ON : 2026-10-16

### fix (2026-10-16) — Track A543 EXTR operand-inversion fix and GIC MMIO support

1. **EXTR Operand Inversion & GIC Support**: Fixed ARM64 EXTR lowering in `u12-orchestrator` by swapping operand concatenation order from `(Rm:Rn)` to standard `(Rn:Rm)` for both 64-bit and 32-bit operations. Integrated prerequisite GICv2 distributor and CPU-interface MMIO window support to clear the 45.64M timer barrier.
2. **Tests** (before → after): Halted at step 45,993,310 with FetchFault → Bypassed 45.99M wall (+69,923 steps); halted at step 46,063,233 on new type-3 engine wall (IllegalInstruction at PC `0xffffff80086be738`, `eor w9, w12, w8, ror #1`).
3. **Files changed**: `units/u12-orchestrator/src/lib.rs`, `units/u12-orchestrator/src/bin/run_kernel.rs`, `files/WALLS.md`, `GATES.md`.

## UPDATED ON : 2026-10-16

### fix (2026-10-16) — Track A546 EOR/EON logical shifted register ROR fix

1. **EOR/EON & ROR Logical Shift Fix**: Removed invalid `shift < 0b11` check in `u1-decode/src/dp_reg.rs` allowing ROR (0b11) for all logical shifted register instructions (EOR, EON, AND, BIC, ORR, ORN, ANDS, BICS); implemented 32-bit and 64-bit ROR in `eval_shift32`/`eval_shift64` and `execute_arm64`; corrected 32-bit ROR and ASR lowering in `u3-wasm-jit` to prevent 64-bit rotation truncation.
2. **Tests** (before → after): Halted at step 46,063,233 (IllegalInstruction eor w9,w12,w8,ror #1) → Bypassed 46.06M wall (+80M+ steps to 126M+); u1-decode 105 → 106, u2-ir-lift 127 → 128, u12-orchestrator 134 → 136 tests passed.
3. **Files changed**: `units/u1-decode/src/dp_reg.rs`, `units/u1-decode/src/lib.rs`, `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/src/arm64.rs`, `units/u12-orchestrator/src/lib.rs`, `units/u3-wasm-jit/src/lib.rs`.

## T560 MMU feature-gap — 2026-10-18

- Started T560 from the T558/A546 revision after confirming base commit `1af87e805ab3335f1e744be96fe7465d32584799`; pinned guest inputs were hash-verified and left read-only.
- Added the MMFR0 PARange=2 implementation and QEMU-matched MRS-to-TCR regression vector. The focused release suite passes 7 tests.
- Aligned traces show TCR matches the pinned QEMU value at step 1,000,000, but the first remaining state mismatch is the DFR0 MRS result at Rust step 7,464 / QEMU step 7,470. See `okf/branches/t560-mmu-feature-gap.md` and `/Users/Shared/agyx-track560-mmu-gap.md`.
- 150M Rust boot gate with per-write PL011 state capture remains in progress; final UART write, guest PID-1 marker, and gate status are pending.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T563 ID_AA64DFR0_EL1 alignment to Cortex-A53 QEMU oracle

1. **DFR0 Alignment**: Aligned `ID_AA64DFR0_EL1` MRS read to `0x10305106` matching Cortex-A53 QEMU 11.1.2; added QEMU-matched regression test `p3_dfr0_el1_matches_cortex_a53_qemu` covering MRS -> SBFX -> CMP -> B.LT fallthrough sequence; updated `u2-ir-lift` unit tests.
2. **Tests** (before → after): sysreg_p3 7/7 → 8/8 passed; u2-ir-lift 128/128 → 129/129 passed. First-million trace: DFR0 divergence at Rust step 7,464 / QEMU step 7,470 completely eliminated. Next divergence is at Rust step 7,578 / QEMU step 7,584 (`MRS X3, DCZID_EL0`: Rust returns 0x10 [DZP=1], QEMU returns 0x4 [DZP=0]), causing control flow fork at Rust step 7,580 / QEMU step 7,586 (`TBNZ W3, #4`).
3. **Files changed**: `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T570 DTB totalsize & runtime nodes alignment to Cortex-A53 QEMU oracle

1. **DTB Runtime Alignment & First Divergence Fix**: Aligned guest RAM DTB payload at `0x48200000` to Cortex-A53 QEMU 11.1.2 virt runtime DTB (22,930 bytes, SHA-256 `c94b1fd1ea4915ac0f3bd4fc69f5259bc695f8aad6ca49124760ab6512021a91`); eliminated DTB header `totalsize` mismatch (`0x5992` in QEMU vs `0x5b9` in raw DTB) accessed via `LDR W8, [X23, #4]` + `REV W8, W8` in Linux `setup_machine_fdt()` at Rust step 33,320 / QEMU step 33,326; added QEMU-matched regression test `p3_fdt_totalsize_load_rev_matches_cortex_a53_qemu`.
2. **Tests** (before → after): sysreg_p3 9/9 → 10/10 passed. First-million aligned diff: 10,015-step window (Rust steps 29,994..40,009 / QEMU steps 30,000..40,015) matches 100% bit-for-bit across all GPRs, SP, CPSR, SCTLR, TTBR0, TCR, TTBR1, MAIR. Step 40,000 divergence eliminated; next periodic divergence sampled at Rust step 49,994 / QEMU step 50,000.
3. **Files changed**: `guest-image/qemu-virt-runtime.dtb`, `units/u12-orchestrator/src/bin/run_kernel.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`, `GATES.md`, `GATES-T570.md`.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T574 MPIDR_EL1 & MIDR_EL1 alignment to Cortex-A53 QEMU oracle

1. **MPIDR_EL1 & MIDR_EL1 Alignment**: Aligned MPIDR_EL1 read to `0x8000_0000` (RES1 bit 31 = 1, U bit 30 = 0 multiprocessor-capable, Aff0 = 0) and MIDR_EL1 read to `0x410f_d034` (ARM Cortex-A53 r0p4) to match the Cortex-A53 QEMU 11.1.2 oracle. Fixed the root cause of the Rust step 41,099 / QEMU step 41,105 divergence where Linux `setup_arch()` reads MPIDR and MIDR to format the early boot CPU banner string ("Booting Linux on physical CPU 0x%010lx [0x%08x]"). Added QEMU-matched regression test `p3_mpidr_midr_el1_matches_cortex_a53_qemu`.
2. **Tests** (before → after): sysreg_p3 10/10 → 11/11 passed (TEN-PASSES OK); u2-ir-lift 129/129 passed. Aligned diff: 15,005 consecutive instructions in 40,000..55,000 window and all 60,000 dense instructions in 40,000..100,000 match 100% bit-for-bit across all GPRs, SP, CPSR, and sysregs. In addition, 100/100 periodic checkpoints across the entire 1M-step window (steps 10k..1,000,000) match QEMU 100%.
3. **Files changed**: `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`, `GATES.md`, `TIMELINE.md`.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T575 PSCI_VERSION HVC return value alignment to Cortex-A53 QEMU virt oracle (v1.1)

1. **PSCI_VERSION & MIGRATE_INFO_TYPE Alignment**: Aligned HVC PSCI dispatch in `units/u12-orchestrator/src/lib.rs` and `contracts/src/cpu.rs` so `PSCI_VERSION` (`0x84000000`) returns `0x00010001` (PSCI v1.1) matching Cortex-A53 QEMU 11.1.2 virt oracle (`target/arm/tcg/psci.c:95-96`), replacing previous `0x00010000` (v1.0). Added `PSCI_MIGRATE_INFO_TYPE` (`0x84000006`) returning 2 (`PSCI_0_2_RET_TOS_MIGRATION_NOT_REQUIRED`). Fixed root cause of divergence at Rust step 36,828,780 / QEMU step 36,828,786 where Linux `psci_probe()` reads minor version into `X2`/`X20` to format `"psci: PSCIv1.1 detected in firmware.\n"`. Added QEMU regression tests `p3_psci_version_hvc_matches_cortex_a53_qemu` and `p3_psci_migrate_info_type_matches_cortex_a53_qemu`.
2. **Tests** (before → after): sysreg_p3 11/11 → 13/13 passed (TEN-PASSES OK); u12-orchestrator 134/134 passed. Aligned diff: all 1,215 instructions across window 36,828,786..36,830,000 match 100% bit-for-bit across all 31 GPRs, SP, CPSR, SCTLR, TTBR0, TCR, TTBR1, MAIR. Divergence at step 36,828,786 completely eliminated.
3. **Files changed**: `contracts/src/cpu.rs`, `units/u12-orchestrator/src/lib.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`, `units/u15-exec-wasmi/src/lib.rs`, `GATES.md`, `TIMELINE.md`.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T577 DC ZVA implementation eliminating vmemmap translation fault

1. **DC ZVA Implementation**: Implemented `DC ZVA, Xt` (`0xd50b_7420`..`0xd50b_743f`) in `units/u2-ir-lift/src/lib.rs` to align the 64-byte cache block to 64 bytes (`Xt & !0x3F`) and store 64 zero bytes via 8x 64-bit zero stores. When `DCZID_EL0` was set to `0x04` in T564, Linux `clear_page` switched to `DC ZVA`. Because `DC ZVA` was previously lowered as a NOP, allocated pages remained unzeroed, creating corrupted pointer `X0/X22 = 0x7fd77cfdfd7cdb7f` at step 46,172,331 and halting with `WasmTrap TranslationFault` during `virt_to_page` calculation. Implementing `DC ZVA` eliminates the fault completely and advances execution past 55,000,000 steps without halt.
2. **Tests** (before → after): Halted at step 46,172,331 with TranslationFault → Bypassed fault (+8,827,669 steps to 55M budget without halt); sysreg_p3 10/10 → 11/11 passed (`p3_dc_zva_zeroes_64_byte_block_matches_cortex_a53_qemu` verified 10/10 consecutive passes); u2-ir-lift 129/129 → 130/130 passed (`dc_zva_lifts_to_zeroing_sequence`).
3. **Files changed**: `units/u2-ir-lift/src/lib.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`, `GATES.md`, `TIMELINE.md`.

## UPDATED ON : 2026-10-18

### fix (2026-10-18) — Track T579 PSCI_FEATURES HVC query alignment to Cortex-A53 QEMU virt oracle

1. **PSCI_FEATURES Alignment**: Aligned HVC dispatch across `contracts/src/execution.rs`, `units/u3-wasm-jit`, `units/u15-exec-wasmtime`, `units/u15-exec-wasmi`, and `units/u12-orchestrator` so `PSCI_FEATURES` (`0x8400000a`) queries the feature in X1 (`param[1]`) matching Cortex-A53 QEMU 11.1.2 virt oracle (`target/arm/tcg/psci.c:178-204`). Returns `0` (`PSCI_RET_SUCCESS`) for `CPU_SUSPEND` (`0xc4000001` / `0x84000001`), `CPU_ON`, `CPU_OFF`, `AFFINITY_INFO`, `MIGRATE_INFO_TYPE`, `SYSTEM_OFF`, `SYSTEM_RESET`, `PSCI_VERSION`, and `PSCI_FEATURES`; returns `-1` (`PSCI_RET_NOT_SUPPORTED`) for unsupported features. Fixed root cause of divergence at Rust step 36,852,806 / QEMU step 36,852,812 where Linux queries `PSCI_FEATURES` for `CPU_SUSPEND` (`X1 = 0xc4000001`); QEMU returned 0 while Rust previously returned -1. Added QEMU regression tests `p3_psci_features_cpu_suspend_hvc_matches_cortex_a53_qemu`, `p3_psci_features_unsupported_hvc_matches_cortex_a53_qemu`, and `p3_psci_features_unknown_64_hvc_matches_cortex_a53_qemu`.
2. **Tests** (before → after): sysreg_p3 13/13 → 16/16 passed (TEN-PASSES OK); u3-wasm-jit 39/39 passed; u12-orchestrator passed. Aligned diff: all 10,021 instructions across window 36,850,000..36,860,000 match 100% bit-for-bit across all 31 GPRs, SP, CPSR, SCTLR, TTBR0, TCR, TTBR1, MAIR. Divergence at step 36,852,812 completely eliminated; periodic checkpoints match through step 36,930,000 (80k+ steps past divergence).
3. **Files changed**: `contracts/src/execution.rs`, `contracts/src/cpu.rs`, `units/u3-wasm-jit/src/lib.rs`, `units/u15-exec-wasmtime/src/lib.rs`, `units/u15-exec-wasmi/src/lib.rs`, `units/u12-orchestrator/src/lib.rs`, `units/u12-orchestrator/tests/sysreg_p3.rs`, `GATES.md`, `TIMELINE.md`.


## UPDATED ON : 2026-10-19

### diagnosis (2026-10-19) — Track T584 150M boot gate & UART silence root-cause diagnosis

1. **150M Boot Gate & UART Silence Diagnosis**: Executed full 150M-step boot gate cold from reset on combined lineage 2d3d2bc without trap or crash (150,000,000 steps reached at PC 0xffffff800822e650, step budget reached, 0 dynamic panics). Diagnosed 3,055-byte UART silence anomaly: guest is live and cycling in Linux kernel idle/scheduler loop (`cpu_startup_entry` -> `do_idle` -> `cpu_do_idle` [PC 0xffffff8008084eb4, WFI at 0xffffff8008084eb8] and `__schedule` -> `pick_next_task` -> `switch_mm` -> `finish_task_switch`). Wall identified: engine lacks architectural IRQ exception injection to `VBAR_EL1 + 0x280` (`el1_irq`), leaving the CPU without timer tick interrupts; PID 1 (`kernel_init`) is never scheduled to userspace /init.
2. **Tests** (before → after): sysreg_p3 17/17 passed (TEN-PASSES 10/10 verified); 150M boot gate executed to completion (150,000,000 steps, 0 panics).
3. **Files changed**: `GATES.md`, `TIMELINE.md`, `/Users/Shared/agyx-track584-rust-init-gap.md`.
