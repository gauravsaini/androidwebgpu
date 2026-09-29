# Guest Boot Roadmap: the iterative loop

**Status**: active plan (2026-09-30). Track B's ABANDON was the honest call for
one phase; this document turns the remaining gap into a measurable,
iteratively closable loop. No fake boot, ever — the metric below cannot be
faked.

## 1. The loop

**Metric**: `halt_step` — the number of AArch64 instructions the real AOSP
kernel (`/mnt/sdb1/aosp/Image`, Linux 4.19.130, build 6640132) executes under
the Path N vCPU before halting. Current value: **6** (halts on `STP`).

**Why this metric is unfakeable**: it is produced by executing the real,
unmodified Google kernel binary step-by-step. Every gap closed moves the halt
further. There is no way to inflate it without genuinely implementing more of
the architecture.

**Loop driver** (mechanical, no judgment calls):
1. Run `cargo test -p u12-orchestrator --test kernel_boot_attempt` on the box
   (needs `/mnt/sdb1/aosp/Image`).
2. Read the halt cause (illegal word, fault address, missing device).
3. That halt cause IS the next track's mission.
4. Track implements the missing piece on its own branch.
5. Gate: `halt_step` strictly increases AND `cargo test --workspace` green AND
   new unit tests for the added semantics.
6. Merge, push, repeat from 1.
7. Termination: kernel prints to PL011 UART (earlycon) and reaches `init`.

## 2. Phases

### Phase 1 — ISA expansion (4 parallel tracks, independent)
| Track | Slice | Why |
|---|---|---|
| GB-1 | Load/store: `STP`/`LDP` (all variants), LDR/STR pre/post-index + literal, `LDRSW` | Unblocks instruction 6 directly; pair ops are everywhere in kernel entry |
| GB-2 | Branches + flags: `B.cond` (all 14 codes), `TBZ`/`TBNZ`, `CMP`/`CMN`/`TST` + NZCV in `CpuState` | Kernel entry is branch-and-compare heavy; NZCV is prerequisite for everything after |
| GB-3 | System + barriers: `MSR`/`MRS` (basic sysregs: `DAIF`, `NZCV`, `CurrentEL`, `TPIDR_EL1`), `DMB`/`DSB`/`ISB` as honest NOPs on single-vCPU | Kernel touches sysregs within the first hundred instructions |
| GB-4 | Bitwise/bitfield: `AND`/`ANDS`, `BIC`, `UBFX`/`SBFX`, `LSL`/`LSR`/`ASR` (imm+reg), `MOVK` | Bitfield ops dominate device setup code |

Each GB track: extend `u1-decode` classification + `u12` execute semantics +
golden-word unit tests (hand-computed encodings, like the existing style).
Out of scope for Phase 1: anything needing EL switching, MMU, or devices.

### Phase 2 — Exception model (1–2 tracks, after Phase 1)
`EL0`/`EL1`, `SP_EL0`/`SP_EL1`, `SPSR_EL1`/`ELR_EL1`/`ESR_EL1`, `VBAR_EL1`,
`SVC`/`ERET`, synchronous exception entry/return. Gate: kernel survives its
first `SVC`.

### Phase 3 — MMU (1 track, after Phase 2)
Integrate `u4-mmu` into `u12`'s memory pipeline; identity-map the kernel's
high-half (`0xFFFF_8000_0000_0000+`) region. Gate: no more `FetchFault` on
high addresses; `halt_step` jumps by orders of magnitude.

### Phase 4 — Interrupt controller + timer (1 track)
GICv2/v3 distributor + CPU interface MMIO, ARM generic timer (`CNTV_*`).
Gate: kernel passes timer calibration.

### Phase 5 — Console + boot (1 track)
PL011 UART register model (`UARTDR`/`UARTFR`/`UARTCR`/…), DTB passed in `x0`,
minimal rootfs from Track D attached. Gate: kernel log appears on UART and
`init` runs. **This is the finish line of the whole Path N guest-boot goal.**

## 3. Swarm laws for this loop (binding)
- One `git worktree` per track, branches `feat/gb-<n>-<slice>` off the current
  tip. No shared working trees.
- Workers commit locally (conventional commits); they cannot push. Delivery is
  `git bundle` → reviewer gate → push from sandbox.
- The reviewer gate re-runs the real gate on every track: `halt_step`
  increase is verified by ME, not claimed by the worker.
- A track that cannot move `halt_step` is not "almost done" — it is not done.
- Honest NOPs are allowed only where architecturally sound (barriers on
  single-vCPU); anything else must be real semantics.
- NTFS mode noise suppressed per-command (`GIT_CONFIG_COUNT=1
  GIT_CONFIG_KEY_0=core.fileMode GIT_CONFIG_VALUE_0=false`); never `git config`
  inside the box repo.

## 4. Current state
- `halt_step` = 6, `HaltReason::IllegalInstruction { word: 0xa9000415 }` (STP).
- Kernel packaging (`guest-image/src/kernel.rs`), gap analysis
  (`docs/architecture/guest-boot-gaps.md`), and the measured halt test
  (`units/u12-orchestrator/tests/kernel_boot_attempt.rs`) are merged.
- Phase 1 launches 2026-09-30: GB-1..GB-4 in parallel on the box swarm.
