# Instruction Family Plan: fam/mmusys (System Registers, Atomics, MMU)

## 1. Scope & Strategy
Track `fam/mmusys` owns:
1. **F5 System**: `mrs`, `msr`, barriers (`isb`, `dmb`, `dsb`), cache/TLB maintenance (`tlbi`, `dc`, `ic`, `sys`, `sysl`), and exceptions (`brk`, `hlt`, `svc`, `hvc`, `smc`, `eret`).
2. **F7 Atomics**: LSE atomics (`swp`, `cas`, `ldadd`/`stadd`, `ldclr`/`stclr`, `ldset`/`stset`, `ldeor`/`steor`, `ldsmin`/`stsmin`, `ldsmax`/`stsmax`), acquire/release (`ldar`, `stlr`, `ldapr`), exclusive monitor extensions (`ldxr`, `stxr`, `ldaxr`, `stlxr`).
3. **MMU & Exception Architecture**: Stage-1 page table walk translation paths, system register state management, and exception return (`eret`).

### Architectural Invariants & Rules
- **Interpreter-First**: All instruction semantics implemented in `units/u1-decode` and `units/u2-ir-lift`. No new `u12-orchestrator` fast-path arms without prior profiling.
- **Pure Function Contracts**: Decoders and lifters are pure functions of instruction words/addresses. No hidden state or implicit side-effects.
- **Single-vCPU Semantics**:
  - Barriers (`DMB`, `DSB`, `ISB`) guarantee ordering and completion; on a single-vCPU sequentially-consistent host, they retire as non-trapping completions (no-ops).
  - Cache/TLB maintenance operations (`DC`, `IC`, `TLBI`) ensure coherency; on a coherent unified memory model, they retire without memory corruption or traps.
  - Exclusive monitor (`LDXR`/`STXR`): Monitored address tracked locally. Ordinary stores do not clear the monitor in single-vCPU mode.
- **Exact-Word Tests**: Every instruction decoding and lifting claim is verified with exact 32-bit instruction-word witnesses.

---

## 2. Kernel Survey: Hit vs Static Frequencies

### Static Frequency in `/mnt/sdb1/aosp/Image` (Total Words: 5,768,320)
| Family | Mnemonic | Static Count | Primary Purpose |
|---|---|---|---|
| F5 | `mrs` | 22,811 | Read system register |
| F5 | `msr` | 14,910 | Write system register / PSTATE field |
| F5 | `brk` | 10,250 | Software breakpoint exception |
| F5 | `isb` | 5,738 | Instruction synchronization barrier |
| F5 | `dmb` | 1,991 | Data memory barrier |
| F5 | `dsb` | 1,740 | Data synchronization barrier |
| F5 | `hlt` | 422 | Halt instruction |
| F5 | `tlbi` | 198 | TLB invalidate operations |
| F5 | `svc` | 171 | Supervisor call |
| F5 | `hvc` | 117 | Hypervisor call |
| F5 | `smc` | 116 | Secure monitor call |
| F5 | `sys` / `sysl`| 209 | System operations / reads |
| F5 | `dc` | 30 | Data cache maintenance |
| F5 | `ic` | 13 | Instruction cache maintenance |
| F5 | `eret` | 11 | Exception return |
| F7 | LSE Atomics (`swp`, `ldadd`, etc.) | 73,439 | Atomic arithmetic and logical memory operations |
| F7 | `ldar` / `stlr` | 41,316 | Load-acquire and store-release atomic operations |
| F7 | `ldxr` / `stxr` / `ldaxr` / `stlxr` | 2,146 | Exclusive load/store pairs |
| F7 | `cas` / `casa` / `casl` / `casal` | 153 | Compare and swap |

### Dynamic Hits During Boot (Measured up to Step 1,259,220)
- **Top Dynamic MRS**:
  1. `SP_EL0` (`3, 0, 4, 1, 0`): 11 hits (thread_info / current task pointer)
  2. `TPIDR_EL1` (`3, 0, 13, 0, 4`): 13 hits (per-CPU offset)
  3. `CTR_EL0` (`3, 3, 0, 0, 1`): 4 hits (cache geometry)
  4. `ID_AA64MMFR0_EL1` (`3, 0, 0, 7, 0`): 2 hits (memory model feature probe)
  5. `MIDR_EL1` (`3, 0, 0, 0, 0`): 1 hit (CPU part/implementer ID)
  6. `CurrentEL` (`3, 0, 4, 2, 2`): 1 hit (current exception level)
  7. `SCTLR_EL1` (`3, 0, 1, 0, 0`): 1 hit (system control register)
  8. `ID_AA64MMFR1_EL1` (`3, 0, 0, 7, 1`): 1 hit
  9. `ID_AA64DFR0_EL1` (`3, 0, 0, 5, 0`): 1 hit
  10. `MPIDR_EL1` (`3, 0, 0, 0, 5`): 1 hit (core 0 affinity)
  11. `DCZID_EL0` (`3, 3, 0, 0, 7`): 1 hit (DC ZVA prohibited)
- **Top Dynamic MSR / SYS**:
  1. `DC IVAC` (`1, 0, 7, 6, 1`): 1,027 hits (cache invalidation by VA)
  2. `MSR SCTLR_EL1, Xt`: 2 hits (MMU enable/setup)
  3. `DC CIVAC` (`1, 3, 7, 14, 1`): 2 hits (cache clean & invalidate by VA)
  4. `TLBI VMALLE1` (`1, 0, 8, 7, 0`): 1 hit (TLB invalidate all)
  5. `MSR SPSel, #imm` (`0, 0, 4, 1, 5`): 1 hit
  6. `MSR SP_EL0, Xt` (`3, 0, 4, 1, 0`): 1 hit
  7. `MSR VBAR_EL1, Xt` (`3, 0, 12, 0, 0`): 1 hit (exception vector table base)
  8. `MSR TPIDR_EL1, Xt` (`3, 0, 13, 0, 4`): 1 hit
  9. `MSR TTBR0_EL1, Xt` (`3, 0, 2, 0, 0`): 1 hit (translation table base 0)
  10. `MSR DAIFClr, #8` (`0, 3, 4, 8, 7`): 1 hit
  11. `MSR TTBR1_EL1, Xt` (`3, 0, 2, 0, 1`): 1 hit (translation table base 1)
  12. `IC IALLU` (`1, 0, 7, 5, 0`): 1 hit (instruction cache invalidate)
  13. `MSR CPACR_EL1, Xt` (`3, 0, 1, 0, 2`): 1 hit (FPEN enable)
  14. `MSR TCR_EL1, Xt` (`3, 0, 2, 0, 2`): 1 hit (translation control)
  15. `MSR DAIFSet, #2` (`0, 3, 4, 2, 6`): 1 hit (local IRQ disable)
  16. `MSR MDSCR_EL1, Xt` (`2, 0, 0, 2, 2`): 1 hit (debug control)
  17. `MSR MAIR_EL1, Xt` (`3, 0, 10, 2, 0`): 1 hit (memory attributes)
- **Top Dynamic Barriers**:
  - `DSB` (ISB, ISHST, OSH, SY): 10 hits
  - `ISB`: 9 hits
  - `DMB` (ISH, ISHST): 8 hits
  - `HINT/NOP`: 26 hits
- **Top Dynamic Atomics**:
  - `SWP` / `SWPA` / `SWPL` / `SWPAL` (64-bit): 138 hits
  - `LDXR` / `STXR` (64-bit & 32-bit): 19 pairs
  - `STLR` (64-bit): 4 hits
  - `LDUMAXB` / `STUMAXB`: 3 hits
  - `LDADD` / `STADD` (64-bit): 1 hit

---

## 3. Encoding Ranges & Edge Cases

### F5 System Encoding Map
- **MSR (register)**: `1101 0101 0001 op0(2) op1(3) CRn(4) CRm(4) op2(3) Rt(5)`
- **MRS**: `1101 0101 0011 op0(2) op1(3) CRn(4) CRm(4) op2(3) Rt(5)`
- **MSR (immediate)**: `1101 0101 0000 000 op1(3) 0100 CRm(4) op2(3) 11111`
  - `DAIFSet`: op1=3, op2=6, CRm = imm4
  - `DAIFClr`: op1=3, op2=7, CRm = imm4
  - `PAN`: op1=0, op2=4, CRm = imm4
  - `UAO`: op1=0, op2=3, CRm = imm4
  - `SPSel`: op1=0, op2=5, CRm = imm4
- **Barriers & Hints**: `1101 0101 0000 0011 0010 CRm(4) op2(3) 11111` (HINT) / `... 0011 CRm(4) op2(3) 11111` (Barriers)
  - `DMB`: op2=101, CRm = domain/type (0b1011=ISH, 0b1010=ISHST, 0b1001=ISHLD, 0b1111=SY)
  - `DSB`: op2=100, CRm = domain/type (0b1011=ISH, 0b1010=ISHST, 0b1110=OSH, 0b0111=NSH, 0b1111=SY)
  - `ISB`: op2=110, CRm = 0b1111 (SY)
- **SYS / SYSL**:
  - `SYS`: `1101 0101 0000 1 op1(3) CRn(4) CRm(4) op2(3) Rt(5)` (TLBI, DC, IC)
  - `SYSL`: `1101 0101 0010 1 op1(3) CRn(4) CRm(4) op2(3) Rt(5)`
- **Exception Generation & Return**:
  - `SVC`: `1101 0100 000 imm16 00001`
  - `HVC`: `1101 0100 000 imm16 00010`
  - `SMC`: `1101 0100 000 imm16 00011`
  - `BRK`: `1101 0100 001 imm16 00000`
  - `HLT`: `1101 0100 010 imm16 00000`
  - `ERET`: `1101 0110 100 11111 0000 00 11111 00000` (`0xD69F03E0`)

### F7 Atomics Encoding Map
- **LDXR / STXR**: `size(2) 001000 0 L 0 Rs o0 11111 Rn Rt` (o0=1 for LDAXR/STLXR)
- **LDAR / STLR**: `size(2) 001000 1 L 0 11111 1 11111 Rn Rt`
- **LSE Swap (SWP)**: `size(2) 111000 A R 1 Rs 1 000 Rn Rt`
- **LSE Compare and Swap (CAS)**: `size(2) 0001000 0 A R 1 Rs 011111 Rn Rt`
- **LSE Arithmetic (LDADD / STADD)**: `size(2) 111000 A R 1 Rs 0 000 Rn Rt`
- **LSE Bitwise (LDCLR, LDSET, LDEOR)**: `opc in {001 (CLR), 010 (EOR), 011 (SET)}`
- **LSE Min/Max (LDSMAX, LDSMIN, LDUMAX, LDUMIN)**: `opc in {100..111}`

---

## 4. Execution Plan & Priorities

### Batch 1 (First Deliverable: Top-10 by Frequency + Exact-Word Unit Tests)
1. **Decode completeness for F5 exceptions**: Add `BRK`, `HLT`, `HVC`, `SMC`, `ERET` to `units/u1-decode/src/system.rs`.
2. **Lift support for high-frequency PSTATE field writes**:
   - `MSR PAN, #imm` (2,653 static hits) -> retire as no-op.
   - `MSR UAO, #imm` (102 static hits) -> retire as no-op.
3. **Lift support for high-frequency System Registers**:
   - `TPIDR_EL2` (`3, 4, 13, 0, 2`): 4,998 static hits -> return 0 / read as 0.
   - `CLIDR_EL1` (`3, 1, 0, 0, 1`): Cache Level ID Register -> return cache hierarchy geometry (0x09200023).
   - `CNTFRQ_EL0` (`3, 3, 14, 0, 0`): Counter Frequency -> return 62.5MHz (0x03B9_ACA0).
   - `CNTVCT_EL0` (`3, 3, 14, 0, 2`) & `CNTPCT_EL0` (`3, 3, 14, 0, 1`): Timer counter values -> return injected clock/timer value or 0.
   - `CSSELR_EL1` (`3, 2, 0, 0, 0`) & `CCSIDR_EL1` (`3, 1, 0, 0, 0`): Cache size selection & ID.
   - `ESR_EL1`, `FAR_EL1`, `ELR_EL1`, `SPSR_EL1`: Exception status and linkage registers.
4. **Complete barrier encodings and verification**:
   - `ISB #15` (`0xD5033FDF`)
   - `DMB ISH` (`0xD5033BBF`), `DMB ISHST` (`0xD5033ABF`), `DMB ISHLD` (`0xD50339BF`), `DMB SY` (`0xD5033FBF`)
   - `DSB ISH` (`0xD5033B9F`), `DSB ISHST` (`0xD5033A9F`), `DSB OSH` (`0xD503399F`), `DSB SY` (`0xD5033F9F`), `DSB NSH` (`0xD503379F`)
5. **Exact-Word Unit Tests**: Write unit tests pinning the exact 32-bit instruction words for every single one of the above.
6. **Tiered Gates**: Run tests across `u1-decode`, `u2-ir-lift`, `u12-orchestrator`.
7. **Local Commit**: `feat(cpu): F5 system registers, barriers, and exception instructions (Batch 1)`

### Batch 2 (F7 Atomics & Stage-1 Translation Extensions)
1. **Decode & Lift for LSE Atomics**:
   - `SWP` / `SWPA` / `SWPL` / `SWPAL` (32-bit and 64-bit)
   - `LDADD` / `STADD`, `LDCLR` / `STCLR`, `LDSET` / `STSET`, `LDEOR` / `STEOR`
   - `CAS` / `CASA` / `CASL` / `CASAL`
2. **LDAR / STLR / LDAPR Semantics**:
   - Single-copy atomic loads and stores for B, H, W, X sizes.
3. **Stage-1 MMU & Exception Model Integration**:
   - `ERET` handler: restore PC from ELR_EL1, restore PSTATE from SPSR_EL1, switch stack pointers.
4. **Tiered Gates & Differential Verification**:
   - Validate with trace_diff against QEMU oracle.
