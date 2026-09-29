# ABANDON: Track B (AOSP Arm64 Guest Boot to Init) Gate Failure Report

**Date**: 2026-09-30  
**Branch**: `feat/pathn-guest-boot`  
**Decision**: ABANDON per Swarm Law 5 & Track B Honesty Rule  
**Author**: Autonomous Agent (Track B Executor)

---

## 1. Measured Reason for Abandonment

The Track B objective ("boot a real AOSP kernel + minimal rootfs to `init` under the Path N vCPU") is **genuinely impossible** under the current Path N architecture without completing 20–28 engineer weeks of full-system emulator construction across 6 crates (`contracts/`, `u1-decode`, `u2-ir-lift`, `u3-wasm-jit`, `u4-mmu`, `u12-orchestrator`).

### Measured Gate Evidence

Using the official Google AOSP ARM64 kernel (`/mnt/sdb1/aosp/Image`, build 6640132, 23 MiB uncompressed) loaded into `u12-orchestrator`:

```text
=== Kernel Execution Trace ===
step  0: pc=0x40000000 word=0x91005a4d (add  x13, x18, #0x16 - MZ header)  -> OK
step  1: pc=0x40000004 word=0x144effff (b    0x413c0000 - branch to stext) -> OK
step  2: pc=0x413c0000 word=0x94000008 (bl   0x413c0020 - preserve_boot_args) -> OK
step  3: pc=0x413c0020 word=0xaa0003f5 (mov  x21, x0 - copy DTB pointer)  -> OK
step  4: pc=0x413c0024 word=0xb0000820 (adrp x0, 0x42885000)               -> OK
step  5: pc=0x413c0028 word=0x91000000 (add  x0, x0, #0x0)                 -> OK
step  6: pc=0x413c002c word=0xa9000415 (stp  x21, x1, [x0])                -> FATAL HALT
```

**Final Outcome**:
`HaltReason::IllegalInstruction { addr: 0x413c002c, word: 0xa9000415 }`

The vCPU halts on instruction 6 (`stp x21, x1, [x0]`).

---

## 2. Impossible Architectural Gates (Summary of Gaps)

1. **Instruction Set Deficiency**:
   - `u1-decode` supports only ~15 discrete AArch64 mnemonics.
   - Missing: `STP`/`LDP`, `MSR`/`MRS`, `B.cond` (all condition codes), `CMP`/`CMN`/`TST`, bitwise `BIC`/`AND`, bitfield `UBFX`/`SBFX`, barriers `DMB`/`DSB`/`ISB`, system operations `DC`/`IC`/`TLBI`, and exceptions `SVC`/`ERET`.
2. **Missing Architectural Register & Privilege Model**:
   - `CpuState` does not model NZCV flags, exception levels (`EL0`/`EL1`), `CurrentEL`, `SP_EL0`/`SP_EL1`, `SPSR_EL1`, `ELR_EL1`, `ESR_EL1`, or `VBAR_EL1`.
3. **MMU Translation Disconnected in Execution Path**:
   - `u12-orchestrator` operates strictly on physical RAM offsets (`0x4000_0000` to `0x4800_0000`).
   - High-memory kernel mapping (`0xFFFF_8000_0000_0000+`) immediately triggers `HaltReason::FetchFault`.
   - `u4-mmu` is not integrated into `u12`'s memory pipeline.
4. **No System Interrupt Controller (GIC) or Arch Timer MMIO**:
   - Linux requires GICv2/v3 MMIO registers (`GICD_*`, `GICC_*`) and ARM Generic Arch Timer system registers (`CNTV_*`). Neither exists.
5. **No Serial Console Model for Linux**:
   - Linux `earlycon` and `amba-pl011` require the ARM PrimeCell PL011 UART register model (`UARTDR`, `UARTFR`, `UARTCR`, `UARTIBRD`, `UARTLCR_H`). Path N only implements two custom byte registers (`0x09000000` and `0x09000008`).
6. **No Device Tree (DTB) Passing**:
   - Linux requires `x0` to point to a valid DTB describing the machine architecture.

---

## 3. Delivered Artifacts & Handoff

Despite the gate block, Track B has delivered the necessary foundations:
1. **Acquired Prebuilts** (saved in `/mnt/sdb1/aosp/`):
   - `/mnt/sdb1/aosp/Image` (Linux 4.19.130 AOSP build 6640132).
   - `/mnt/sdb1/aosp/initramfs.cpio.gz` (Alpine 3.20 aarch64 minimal rootfs).
   - `/mnt/sdb1/aosp/static_initramfs.cpio.gz` (Static no-libc initramfs).
2. **Granular Gap Analysis**:
   - Detailed specification in `docs/architecture/guest-boot-gaps.md`.
3. **Guest-Image Crate Extension**:
   - Linux ARM64 image header parsing (`Arm64Header`, `parse_kernel_header`).
   - Pure, deterministic kernel + ramdisk + DTB PNIM packager (`build_kernel_image`).
   - Kernel boot CPU state generator (`initial_kernel_cpu_state`).
   - Unit and integration tests green (`cargo test -p guest-image`).
4. **Measured Execution Test**:
   - `units/u12-orchestrator/tests/kernel_boot_attempt.rs` validating the exact step trace and halt reason against the real AOSP kernel.
