# Walls

## T499 live CSINV state divergence — engine

- Tag: **engine** (Rust emulator execution state).
- Pinned witness: `MANIFEST.json` input hashes and QEMU/Rust revision; [`qemu-csinv-live-hit2.log`](../evidence/t499/qemu-csinv-live-hit2.log) versus [`rust-csinv-live-hit2.log`](../evidence/t499/rust-csinv-live-hit2.log).
- Observation: both live paths reach Image word `0xda818058` at guest VA `0xffffff8008354018`; QEMU enters with PSTATE `0xa00001c5`, Rust with `0x60000000`, and X24 differs after the instruction.
- Scope: one manifest-bound live capture. This tag records the observed engine wall; TEN-PASSES was not performed, so no reproducibility claim is made.
- Control: [`qemu-csinv-micro.log`](../evidence/t499/qemu-csinv-micro.log) and [`rust-csinv-micro.log`](../evidence/t499/rust-csinv-micro.log) show equal result for matched input state.

## Setup observation not classified as a wall

`evidence/t499/qemu-gicv3-mismatch.log` records an initial QEMU GICv3 run that conflicted with the supplied DTB's GICv2 controller and stopped before `/init`. That configuration was outside the pinned manifest and is not tagged as a guest, WASM, or engine wall.

## T535 GICv2 MMIO unhandled fault (45.64M arch_timer wall) — engine

- Tag: **engine** (type-3 wall: missing device model / MMIO handler).
- Step: `45,636,759` (within the `45,576,164` – `45,844,254` post-`NR_IRQS` window).
- PC: `0xffffff8008621324` (`gic_init_bases` + 0x34).
- Instruction: `ldr w8, [x8]` (opcode: `0xb9400508`), where `x8` points to VA `0xffffff8008010004` (PA `0x08000004`, `GICD_TYPER`).
- Fault: `HaltReason::WasmTrap { addr: 0xffffff8008621324, message: "mem_load fault: TranslationFault { va: 134217732 }" }`.
- Root Cause: Missing GICv2 distributor (`0x08000000`) and CPU interface (`0x08010000`) MMIO handling in `WasmHost::mem_load`/`mem_store`. Address `0x08000004` falls outside RAM (`0x40000000..0x80000000`), triggering an unhandled `TranslationFault`.
- Oracle: QEMU returns `0x00000008` for `GICD_TYPER` and cleanly configures GIC distributor and CPU interface, enabling virtual timer PPI 27 (`0x08000000` into `GICD_ISENABLER0`) at step `45,834,542` and reaching `arch_timer` probe at step `45,844,254`.

## T585/T588/T589 netd-updatable-crashing — guest [RESOLVED in T589]

- Tag: **guest** (Type-1 guest image wall).
- Rust par bhi lagu: haan (applies to both QEMU virt and Rust emulator Android guest userspace).
- Status: **RESOLVED in T589**. In-stream patching of `/system/etc/init/netd.rc` at offset `0x17c9c000` in `ramdisk-images/system.img` substituted dummy sleep daemon and commented out `updatable` stanza. `netd` status-1 crash loop and `init: process with updatable components 'netd' exited 4 times before boot completed` completely eliminated (0 occurrences across 656.25s continuous uptime vs 5+ in T585). Camera provider ranchu (`0x13ab000`) and sensors multihal (`0x13b7000`) stubbed in `vendor.img`. T585 fabricate-display lineage reconstructed and preserved in `/Users/Shared/guest-inputs/t585-fabricate-display/` with SHA-256 manifest.
- Findings: `/Users/Shared/agyx-track589-netd-retry.md`.

## T589 surfaceflinger-display-abort-loop / zygote-clean-exit — guest

- Tag: **guest** (Type-1 guest image wall).
- Rust par bhi lagu: haan (SurfaceFlinger display negotiation and Zygote lifecycle on headless Android).
- Pinned witness: `/Users/Shared/agyx-track589-netd-retry.md` line 15607 (`init: Service 'zygote' (pid 2343) exited with status 0`), lines 15689, 15714, 15742, 15772 (`init: Service 'surfaceflinger' (pid ...) received signal 6`), line 15718 (`init: process with updatable components 'surfaceflinger' exited 4 times before boot completed`).
- Observation: In headless virt guest, SurfaceFlinger aborts with signal 6 (`LOG_ALWAYS_FATAL("!display")`) when `vendor.hwc-2-1` is not started before SurfaceFlinger, triggering `init: process with updatable components 'surfaceflinger' exited 4 times before boot completed` and setting `sys.init.updatable_crashing=1`. Zygote cleanly exits with status 0 at 134.95s, triggering `onrestart` stanza.
- Recommended fix: Explicitly start `vendor.hwc-2-1` and `vendor.allocator-2-0` in `surfaceflinger.rc` or `on early-boot` prior to SurfaceFlinger startup, and investigate Zygote clean exit-0 restart.



## T586 sbcs-carry-flag-unimplemented (49.69M wall) — engine [RESOLVED in T587]

- Tag: **engine** (DataProc ADC/SBC/ADCS/SBCS carry flag arithmetic in `u2-ir-lift`/`u12-orchestrator`).
- Rust par bhi lagu: haan (applies directly to the Rust engine fast-path interpreter and lifter).
- Step: `49,688,549`.
- PC: `0xffffff8008259154` (`lib/find_bit.c:__find_next_bit`).
- Instruction: `sbcs xzr, x10, x8` (`0xfa08015f`).
- Fault: `HaltReason::Unsupported { addr: 18446743524090417492, reason: "DataProc: ADC/SBC/ADCS/SBCS need NZCV carry flag, not expressible in IrOp" }`.
- Status: **RESOLVED in T587**. Full AArch64 architectural `AddWithCarry` flag propagation implemented for 32-bit and 64-bit ADC, SBC, ADCS, and SBCS in `units/u12-orchestrator/src/lib.rs` and `arm64.rs`. Verified against 16 QEMU-matched regression test vectors (`adc_sbc_regression.rs`). Boot cleanly advanced past step 49,688,549 to step 49,688,578.

## T587 TranslationFault va:0 / NULL dereference (49.69M wall) — engine [RESOLVED in T590]

- Tag: **engine** (unhandled MMU synchronous exception / TranslationFault on zero address in `u12-orchestrator`).
- Rust par bhi lagu: haan (applies to Rust emulator MMU / exception vectoring).
- Step: `49,688,578` (29 steps past bypassed T586 wall at step 49,688,549).
- PC: `0xffffff80082591e4` (`kernel/futex.c:futex_init` / `futex_detect_cmpxchg`).
- Instruction: `ldxr w11, [x10]` (`0x885f7d4b`), where `x10 = 0x0` (NULL pointer probe) at halt.
- Fault: `HaltReason::WasmTrap { addr: 0, message: "mmu_fault: TranslationFault { va: 0 }" }`.
- Root Cause: In Linux ARM64, `futex_init` intentionally executes atomic cmpxchg probe with NULL pointer, expecting an architectural synchronous Data Abort exception to vector to `VBAR_EL1 + 0x200` where `do_page_fault` -> `fixup_exception` searches `__ex_table` and fixes up the return value to `-EFAULT`. The Rust emulator previously lacked synchronous Data Abort vectoring, treating non-vmemmap `TranslationFault` as an unhandled fatal `WasmTrap`.
- Status: **RESOLVED in T590**. Synchronous Data Abort exception vectoring to `VBAR_EL1 + 0x200` implemented in `u12-orchestrator` (`DetailedFault`, `translate_detailed`, `take_data_abort_exception`). Sets ESR_EL1 with exact EC (0x25 for EL1), IL=1, WnR, and DFSC (0x05 for Level 1 Translation Fault), FAR_EL1 = faulting VA, saves ELR_EL1/SPSR_EL1, masks DAIF, and vectors to `VBAR_EL1 + 0x200`. Verified against 6 QEMU-matched regression test vectors (`data_abort_vbar_regression.rs`). Live cold boot gate run cleanly advanced past step 49,688,578 to 68M+ steps with UART console advancing from 3,936 bytes to 5,542 bytes (+1,606 bytes).
- Console bytes: 3,936 → 5,542 bytes (clean, 0 dynamic panics).


## T593 zygote-exit0 / missing-hwc-dependency — guest [FAIL]

- Tag: **guest** (DisplayManagerService display timeout, SurfaceFlinger unstarted HWC dependency, and Zygote clean exit-0 loop).
- Rust par bhi lagu: haan (applies to guest image used by both Rust and QEMU engines).
- Guest uptime: 650.9s continuous QEMU virt ARM64 boot.
- Fault: `DisplayManagerService` throws `RuntimeException("Timeout waiting for default display to be initialized")` when SurfaceFlinger is unstarted, causing `system_server` crash and Zygote clean exit 0. Restoring SurfaceFlinger (`class core animation`) and bypassing DMS timeout eliminated the Zygote exit-0 loop (45 -> 1), but uncovered the underlying root cause: `vendor.hwc-2-1` (`android.hardware.graphics.composer@2.1-service`) never starts because `/vendor/etc/init` is not parsed in headless virt boot without `mount_all`. SurfaceFlinger in `SurfaceFlinger::init()` fails with `LOG_ALWAYS_FATAL_IF(!display, "Failed to configure the primary display")` (`SIGABRT` / signal 6, 43 aborts across 650.9s).
- Status: **RESOLVED (Diagnosis complete; Zygote exit-0 loop eliminated [45 -> 1])**. Fix versioned in `guest-inputs/t585-fabricate-display/t593-initramfs.cpio.gz`. Next wall: `surfaceflinger-signal6-abort-loop / missing-hwc-dependency`.

## T592/T594 brk-0x800-unvectorized (145.41M wall) — engine

- Tag: **engine** (AArch64 architectural BRK debug exception vectoring in `u12-orchestrator` / `u2-ir-lift`).
- Rust par bhi lagu: haan (applies to Rust emulator exception dispatch and execution pipeline).
- Step: `145,410,118`.
- PC: `0xffffff80082e0f4c` (`sha256_block_data_order` / Clang CFI check failure during `cfg80211` X.509 regulatory certificate loading).
- Instruction: `brk #0x800` (`0xd4210000`).
- Fault: `HaltReason::Unsupported { addr: 18446743524090974028, reason: "System: BRK exception" }`.
- Status: **RESOLVED in T594**. Architectural BRK exception vectoring to `VBAR_EL1 + 0x200` (`el1_sync`) for EL1 and `VBAR_EL1 + 0x400` (`el0_sync`) for EL0 implemented. ESR_EL1 populated with EC=0x3C, IL=1, ISS=imm16, ELR_EL1 = faulting PC, SPSR_EL1 = saved PSTATE, DAIF masked (0x3C0), FAR_EL1 untouched. Verified against 5 QEMU-matched regression test vectors (`brk_vbar_regression.rs`).

## T594 clang-cfi-sha256-check-fail / init-kill-panic (145.41M wall) — guest

- Tag: **guest** (Type-1 guest kernel Clang Forward-Edge CFI jump table mismatch on assembly symbol `sha256_block_data_order`).
- Rust par bhi lagu: haan (applies to guest kernel execution under both Rust emulator and QEMU virt during X.509 regulatory cert load).
- Step: `145,410,118`.
- PC: `0xffffff80082e0f4c` (`__cfi_check_fail+0x18/0x1c`).
- Fault: `CFI failure (target: sha256_block_data_order.cfi_jt+0x4/0x8)`, `kernel BUG at kernel/cfi.c:32!`, `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b`.
- Status: **DISCOVERED in T594**. Following architectural BRK vectoring bypass, the kernel's trap handler executed the CFI check failure crash handler. Next action: boot with `cfi=permissive` or patch `sha256_block_data_order` CFI type hash in `Image`.

## T607 clang-cfi-sha256-generic-assembly-fail / init-kill-panic (145.27M wall) — engine

- Tag: **engine** (Rust emulator lacks ARMv8 Crypto Extensions, forcing guest to fallback to assembly-backed sha256 drivers that fail Clang CFI).
- Rust par bhi lagu: haan (applies to Rust emulator CPU feature advertising and execution).
- Step: `145,268,307`.
- PC: `0xffffff80082e0f4c` (`__cfi_check_fail+0x18/0x1c`).
- Fault: `CFI failure (target: sha256_block_data_order.cfi_jt+0x0/0x8)`, `kernel BUG at kernel/cfi.c:32!`, `Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b`.
- Status: **DISCOVERED in T607**. Advertising `AdvSIMD = 0xF` (Not Implemented) in `ID_AA64PFR0_EL1` cleanly skipped `sha256-neon` (+0x4 slot). However, the scalar fallback `sha256-generic` in `arch/arm64/crypto/sha256_glue.c` still calls assembly routine `sha256_block_data_order` (`0xffffff8008c88508`, +0x0 slot) via indirect call. Because assembly functions lack Clang CFI type annotations in this kernel build, Clang CFI unconditionally traps on `brk #0x800` at step 145,268,307, with terminal `machine_halt` livelock at step 208,482,953 PC `0xffffff80081842c0`.
- Oracle: QEMU advertises `ID_AA64ISAR0_EL1.SHA2=1` (`0x0001_1120`), which causes Linux to bind `sha256-ce` via direct `bl #0xffffff8008084f38`, completely bypassing Clang CFI indirect call checks.

## T636 zygote64-preload-failure-mediaplayer — guest [RESOLVED in T636]

- Tag: **guest** (Type-1 guest image wall).
- Rust par bhi lagu: haan (applies to guest image used by both Rust emulator and QEMU virt during Android Zygote preloading).
- Pinned witness: `/Users/Shared/agyx-track636-bootclasspath-fix.md`, `flights/flight-01/verify-boot.log.gz`.
- Observation: During `ZygoteInit.preloadClasses()`, `MediaPlayer.<clinit>` -> `native_init()` called JNI `FindClass("android/net/ProxyInfo")` at `/system/lib64/libmedia_jni.so` offset `0x79334`, failing with `NoClassDefFoundError` because `framework-connectivity.jar` was absent from `BOOTCLASSPATH` (the `com.android.tethering` Mainline APEX loop mount had failed). This caused Zygote to abort with `RuntimeException: Error preloading android.media.MediaPlayer` and exit with signal 9 / status 0, cycling repeatedly without starting `system_server`.
- Status: **RESOLVED in T636**. In-stream patching appended flattened `/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar` to both `init.rc`'s `export BOOTCLASSPATH` and `/data/system/environ/classpath`. In QEMU 11.1.2 flight-01, Zygote pid 4870 inherited the full 20-jar BOOTCLASSPATH, `MediaPlayer` preloaded with 0 occurrences of `Error preloading android.media.MediaPlayer` or `NoClassDefFoundError`, and boot proceeded past class preloading.
- Next wall: `surfaceflinger-signal6-abort-loop` (SurfaceFlinger aborts with SIGABRT signal 6 during Gralloc 4 IMapper library suffix lookup, triggering `onrestart restart --only-if-running zygote`, owned by Track T637).


