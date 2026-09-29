# Path N Guest Boot Gap Analysis (Track B — Wave 3 Item C1)

**Date**: 2026-09-30  
**Branch**: `feat/pathn-guest-boot`  
**Status**: Measured technical audit against official prebuilt AOSP/GKI ARM64 kernel  
**Author**: Autonomous Agent (Track B Executor)

---

## 1. Executive Summary & Objective

The objective of **Track B** (Wave 3, item C1 in `SWARM.md` and `plan.md`) is to:
> "Boot a real AOSP kernel + minimal rootfs to `init` under the Path N vCPU (native first, wasm after)."

Path N currently executes a bare-metal shell guest (`pathn-sh`) under an explicit-state emulator pipeline (U1 decode → U2 lift → U3 wasm-jit → U12 orchestrator / U15 wasmtime). This document performs a granular, evidence-based gap analysis comparing the requirements of booting an official AOSP/GKI ARM64 kernel against the current capabilities of the Path N vCPU, MMU, GIC, device models, and contracts (`units/u1` through `units/u6`, `contracts/`).

---

## 2. Acquired Target Artifacts (Phase 1 Evidence)

The following target artifacts were acquired and saved strictly under `/mnt/sdb1/aosp/`:

| Artifact | Source / Build Details | Size | Characteristics |
|---|---|---|---|
| **Kernel Image** (`/mnt/sdb1/aosp/Image`) | Official Google AOSP abfarm build `6640132`<br>Git project: `kernel/prebuilts/4.19/arm64`<br>Target: `kernel_aarch64` | 24,089,088 B (~23.0 MiB) | Little-endian ARM64 boot executable Image, 4K pages.<br>Magic: `ARM\x64` (`0x644d5241`) at offset 0x38.<br>`text_offset`: `0x80000` (512 KiB). |
| **Alpine Minimal Rootfs** (`/mnt/sdb1/aosp/initramfs.cpio.gz`) | Alpine Linux v3.20 aarch64 minirootfs with `/init` symlink to `/bin/busybox` | 3,923,178 B (~3.8 MiB) | Multi-call binary (`busybox`), `/bin/sh`, core utilities. |
| **Static CPIO Ramdisk** (`/mnt/sdb1/aosp/static_initramfs.cpio.gz`) | Custom standalone aarch64 init binary (`static_init`), cross-compiled with `aarch64-linux-gnu-gcc -nostdlib -static` | 779 B | Single static ELF containing only direct `svc #0` syscalls to print confirmation to stdout. |

---

## 3. Arm64 Linux Boot Protocol Gaps

The Linux ARM64 boot protocol (per kernel documentation `Documentation/arm64/booting.rst`) specifies strict entry conditions:

### 3.1 Initial Register State

| Register | Linux Protocol Requirement | Current Path N State (`Orchestrator::load_image`) | Gap Status |
|---|---|---|---|
| `x0` | Physical address of Device Tree Blob (DTB) in system RAM (64-bit aligned). | `0` | **Missing**: Kernel immediately fails DTB validation in `setup_machine_fdt()`. |
| `x1` | Reserved, must be `0`. | `0` | Matched. |
| `x2` | Reserved, must be `0`. | `0` | Matched. |
| `x3` | Reserved, must be `0`. | `0` | Matched. |
| `pc` | Start of kernel image (`RAM_BASE + text_offset`). | `RAM_BASE` (`0x4000_0000`) | **Offset Mismatch**: Current loader enters at `0x4000_0000`, bypassing `text_offset` (`0x80000`). |
| `sp` | Unspecified (kernel sets its own initial stack). | `0x4800_0000` | Acceptable. |
| `PSTATE` | EL1 (or EL2) non-secure, all interrupts masked (`DAIF = 0b1111`), Little-Endian (`E = 0`). | `pstate = 0` | **Missing**: CurrentEL is undefined; DAIF mask is unmodeled. |

### 3.2 Device Tree (DTB) Requirement

Linux ARM64 does not support ACPI without UEFI, and has no legacy PC BIOS. A valid Flattened Device Tree (`.dtb`) must be passed in RAM at `x0`. The DTB must describe:
1. `/chosen`: `bootargs = "console=ttyAMA0 earlycon=pl011,0x09000000 init=/init"`.
2. `/memory`: base `0x40000000`, size `0x10000000+`.
3. `/cpus`: ARMv8 cores with generic timer frequency (`clock-frequency = <62500000>`).
4. `/interrupt-controller`: GICv2 (`arm,gic-400` or `arm,cortex-a15-gic`) or GICv3 node.
5. `/serial`: PL011 UART node (`arm,pl011`) at base `0x09000000` with assigned GIC SPI.

*Current Path N state*: Zero DTB generator or loader. No DTB exists in memory.

---

## 4. Instruction Decoder Gaps (`units/u1-decode`)

Disassembly of `/mnt/sdb1/aosp/Image` from the entry vector demonstrates that the kernel fails on the **5th instruction executed**:

```text
Offset 0x000000: 91005a4d  add  x13, x18, #0x16   (PE/COFF MZ header compat) -> Decodes OK
Offset 0x000004: 144effff  b    0x13c0000         (Branch to kernel stext)   -> Decodes OK
Offset 0x13c000: 94000008  bl   0x13c0020         (Call preserve_boot_args)  -> Decodes OK
Offset 0x13c020: aa0003f5  mov  x21, x0           (orr x21, xzr, x0)         -> Decodes OK
Offset 0x13c024: b0000820  adrp x0, 0x14c5000     (Compute boot args addr)   -> Decodes OK
Offset 0x13c028: 91000000  add  x0, x0, #0x0      (Align address)            -> Decodes OK
Offset 0x13c02c: a9000415  stp  x21, x1, [x0]     (Store pair x21, x1)       -> FATAL ILLEGAL INSTRUCTION
```

At `0x13c02c`, `u1_decode::decode(0xa9000415)` returns `DecodeResult::Illegal { word: 0xa9000415 }`. The vCPU immediately halts with `HaltReason::IllegalInstruction`.

### 4.1 Missing Instruction Inventory in Early Kernel Path (`head.S` / `early_param`)

| Instruction Group | Representative Mnemonics | Opcode Pattern | Kernel Usage | Status in `u1-decode` |
|---|---|---|---|---|
| **Load/Store Pair** | `STP`, `LDP` | `opc=10, 101, V=0` | Stack frame setup, register save/restore in all function prologues/epilogues. | **Missing** (`DecodeResult::Illegal`) |
| **System Registers** | `MSR`, `MRS` | `1101 0101 00x...` | Accessing `CurrentEL`, `SCTLR_EL1`, `TCR_EL1`, `TTBR0_EL1`, `CTR_EL0`, `DAIF`, `CPACR_EL1`. | **Missing** (`DecodeResult::Illegal`) |
| **Barriers & Synchronization** | `DMB`, `DSB`, `ISB` | `0xd5033xxx` | Memory barriers before/after MMU activation, page table writes, device MMIO. | **Missing** (`DecodeResult::Illegal`) |
| **Conditional Branches** | `B.cond` (`b.eq`, `b.ne`, `b.cs`, `b.cc`, etc.) | `0101 0100 ...` (`0x54...`) | All conditional branching based on NZCV condition codes. | **Missing** (Only `CBZ`/`CBNZ` supported) |
| **Flags & Comparisons** | `CMP`, `CMN`, `TST`, `ADDS`, `SUBS`, `ANDS` | `S=1` bit in DP | Setting condition codes (NZCV) in PSTATE for conditional branches. | **Missing** (Flag-setting explicitly rejected) |
| **Bitwise Operations** | `AND`, `BIC`, `EOR`, `ORN`, `MVN` | Logical immediate / shifted reg | Bitmask masking, address alignment, register clearing. | **Missing** (Only `ORR`/`EOR` shifted reg supported) |
| **Bitfield Manipulation** | `UBFX`, `SBFX`, `UBFIZ`, `EXTR` | `1001 0011 ...` | Extracting bitfields (e.g. cache line size from `CTR_EL0`, level indices from VA). | **Missing** (`DecodeResult::Illegal`) |
| **Move Wide Keep** | `MOVK` | `opc=11` in Move Wide | Assembling 64-bit constants and high-memory virtual addresses. | **Missing** (Explicitly rejected in `u1`) |
| **Cache & TLB Maintenance** | `DC CIVAC`, `IC IALLU`, `TLBI VMALLE1` | System instruction encodings | Invalidate I-cache, flush D-cache lines to PoC, invalidate TLB entries. | **Missing** (`DecodeResult::Illegal`) |
| **Exceptions & Returns** | `SVC`, `HVC`, `SMC`, `ERET` | `1101 0100 ...` / `1101 0110 100...` | System calls from user-space, hypervisor calls, return from exception level. | **Missing** (`DecodeResult::Illegal`) |

---

## 5. IR Lifter & Contracts Gaps (`units/u2-ir-lift`, `contracts/`)

### 5.1 `pathn_contracts::cpu::IrOp` Limitations

`pathn_contracts::cpu::IrOp` currently defines only 12 operations:
`Add`, `Mov`, `Load`, `Store`, `Branch`, `BranchDyn`, `LoadDyn`, `StoreDyn`, `CondBranch`, `OrrShift`, `Wfi`, `Trap`.

Gaps in the contract:
1. **No NZCV Condition Flags**: Neither `CpuState` nor `IrOp` represents the four condition flags (Negative, Zero, Carry, Overflow). A conditional branch (`B.cond`) cannot be expressed.
2. **No Subtraction, Multiply, Divide**: `IrOp::Sub` does not exist; pointer differences, decrements, and comparisons cannot be represented.
3. **No Bitwise AND/XOR/Shift Ops**: `IrOp::And`, `IrOp::Xor`, `IrOp::Shl`, `IrOp::Shr`, `IrOp::Sar` are absent.
4. **No System Register Read/Write**: No `IrOp::ReadSysReg` or `IrOp::WriteSysReg`.
5. **No Atomic / Pair Ops**: No support for 128-bit pair operations (`LDP`/`STP`) or atomic memory primitives (`LDXR`/`STXR`).

---

## 6. CPU State & Privilege Model Gaps (`contracts/src/machine.rs`)

### 6.1 `CpuState` Model

The current `CpuState` definition:
```rust
pub struct CpuState {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}
```

Missing architectural state essential for an OS kernel:
1. **Exception Levels (EL0, EL1, EL2)**: The architecture requires distinguishing user mode (EL0) from kernel mode (EL1).
2. **Banked Stack Pointers**: `SP_EL0` vs `SP_EL1` and selector `SPSel`.
3. **Saved Program Status Registers (`SPSR_EL1`)**: Stores caller PSTATE across exceptions.
4. **Exception Link Registers (`ELR_EL1`)**: Stores return address for `ERET`.
5. **Exception Syndrome & Fault Registers**: `ESR_EL1` (syndrome), `FAR_EL1` (fault address).
6. **Vector Base Address Register (`VBAR_EL1`)**: Points to the 16-entry exception vector table.
7. **Control Registers**:
   - `SCTLR_EL1` (MMU enable, cache enable, alignment check, endianness).
   - `TCR_EL1` (Translation control: TG0, TG1, T0SZ, T1SZ).
   - `TTBR0_EL1`, `TTBR1_EL1` (Translation table base registers for low and high VA).
   - `MAIR_EL1` (Memory attribute indirection).
   - `CPACR_EL1` (Architectural feature access control: FP/SIMD traps).
   - `DAIF` (Interrupt mask bits in PSTATE).

---

## 7. MMU & Memory Virtualization Gaps (`units/u4-mmu`, `units/u12-orchestrator`)

1. **U4 MMU Isolation**: While `units/u4-mmu` implements pure stage-1 page table walk algorithms (`u4_mmu::translate`), **it is completely uncalled in `u12-orchestrator`**.
2. **Identity Execution Only**: In `u12-orchestrator`:
   ```rust
   fn fetch_word(&self, pc: u64) -> Result<u32, HaltReason> {
       match self.ram_offset(pc, 4) {
           Ok(off) => Ok(u32::from_le_bytes(self.machine.ram[off..off + 4].try_into().unwrap())),
           Err(_) => Err(HaltReason::FetchFault { addr: pc }),
       }
   }
   ```
   `ram_offset` performs: `if pa >= RAM_BASE && pa + len <= RAM_BASE + RAM_SIZE { Ok((pa - RAM_BASE) as usize) }`.
3. **High-Memory Kernel Crash**: Linux executes in physical memory only until `__enable_mmu` in `head.S`. It then switches to high virtual memory (`0xFFFF_8000_0000_0000` to `0xFFFF_FFFF_FFFF_FFFF`). The moment `pc` branches to high memory, `fetch_word` triggers `HaltReason::FetchFault`.
4. **Data Access Translation**: Memory load/store operations lower to WASM host imports `env.mem_load` and `env.mem_store`, which directly access `self.ram[offset]` with the same physical bounds check. They have no concept of page translation, TLBs, or page faults.

---

## 8. Interrupt Controller (GIC) & Timer Gaps (`units/u5-gic-timer`)

1. **Missing GIC MMIO Interface**:
   Linux detects and configures the GIC via memory-mapped I/O registers:
   - GIC Distributor (`GICD_*`): `GICD_CTLR`, `GICD_TYPER`, `GICD_ISENABLER<n>`, `GICD_ICENABLER<n>`, `GICD_IPRIORITYR<n>`, `GICD_ITARGETSR<n>`, `GICD_ICFGR<n>`.
   - GIC CPU Interface (`GICC_*`): `GICC_CTLR`, `GICC_PMR`, `GICC_IAR`, `GICC_EOIR`.
   Path N's `u5-gic-timer` implements none of these MMIO registers. It models only an internal `IrqState` struct.
2. **Missing ARM Generic Arch Timer**:
   Linux uses the per-CPU ARM generic architectural timer driven by system registers:
   - `CNTV_CTL_EL0` (control), `CNTV_CVAL_EL0` (compare value), `CNTVCT_EL0` (virtual counter), `CNTFRQ_EL0` (counter frequency).
   None of these system registers exist.

---

## 9. Console & Peripheral Gaps (`guest-image/PLATFORM.md`, `u12-orchestrator`)

1. **Current Console Model**: Path N implements two byte-wide registers (`CONSOLE_TX = 0x0900_0000`, `CONSOLE_RX = 0x0900_0008`).
2. **Linux Serial Driver Expectations**:
   Linux ARM64 relies on `earlycon=pl011,0x09000000` or the full `amba-pl011` driver (`drivers/tty/serial/amba-pl011.c`). The PL011 UART register specification requires:
   - `UARTDR` (`0x00`): Data register.
   - `UARTRSR`/`UARTECR` (`0x04`): Receive status / error clear.
   - `UARTFR` (`0x18`): Flag register (bit 5 `TXFF` transmit FIFO full, bit 3 `BUSY`, bit 4 `RXFE` receive FIFO empty).
   - `UARTIBRD` (`0x24`), `UARTFBRD` (`0x28`): Baud rate divisors.
   - `UARTLCR_H` (`0x2c`): Line control register (FIFO enable, word length).
   - `UARTCR` (`0x30`): Control register (UART enable, TX enable, RX enable).
   - `UARTIMSC` (`0x38`), `UARTRIS` (`0x3c`), `UARTMIS` (`0x40`), `UARTICR` (`0x44`): Interrupt control.
   When the kernel's `earlycon` accesses `UARTFR` (`0x09000018`) to check if the transmit FIFO is full, it receives `0` from Path N's MMIO catch-all, but driver initialization fails when writing/reading control registers.

---

## 10. Memory Capacity Gap

1. Path N platform contract (`guest-image/PLATFORM.md`) sets `RAM_SIZE = 0x0800_0000` (128 MiB).
2. The acquired uncompressed kernel `Image` is 23.0 MiB.
3. The rootfs is 3.8 MiB.
4. Linux ARM64 page tables, kernel bss, init memory, buddy allocator, and slab caches require a minimum of 256 MiB to 512 MiB of RAM to avoid early out-of-memory panic during boot.

---

## 11. Work Breakdown & Feasibility Assessment

Closing these gaps to boot an AOSP/GKI kernel to `init` requires:

| Component | Required Implementation Scope | Estimated Effort |
|---|---|---|
| **Instruction Decoder (`u1`)** | Add ~80 new ARMv8 instruction formats (STP/LDP, MSR/MRS, B.cond, CMP/TST, bitfield, barriers). | 4–6 engineer weeks |
| **IR Lifter (`u2`)** | Introduce NZCV flag model, condition evaluation, 64-bit arithmetic/logic IR ops. | 3–4 engineer weeks |
| **Execution Backend (`u3`/`u15`)** | Lower all new IR ops to WASM primitives in `u3-wasm-jit` and wasmtime/wasmi runtime. | 2–3 engineer weeks |
| **CPU Exception Model** | EL0/EL1 privilege switching, exception tables (`VBAR_EL1`), `SVC`, `ERET`, syndrome registers. | 4–5 engineer weeks |
| **MMU Integration (`u4`+`u12`)** | Wire `u4-mmu` into instruction fetch and memory loads/stores; support high-VA translation. | 3–4 engineer weeks |
| **System Device Models** | PL011 UART model + ARM GICv2/v3 MMIO distributor & CPU interface + Arch Timer. | 3–4 engineer weeks |
| **Boot Loader / DTB** | DTB generator matching Path N memory map; Linux AOSP kernel entry protocol compliance. | 1–2 engineer weeks |
| **Total Cumulative Effort** | | **20–28 engineer weeks** |

---

## 12. Conclusion

Path N's current CPU/system architecture is a purpose-built bare-metal shell engine capable of executing its 11 hand-crafted AArch64 instructions in physical RAM without an MMU, exceptions, or interrupts.

Booting a real prebuilt AOSP/GKI ARM64 kernel fails deterministically at step 5 (`0x13c02c`, `stp x21, x1, [x0]`). Booting to `init` requires an entire virtual hardware architecture (instruction decoding, condition codes, exception levels, MMU runtime translation, GIC, timer, PL011 UART, and DTB passing) that does not exist in the codebase.
