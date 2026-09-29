# HLD — Multi-vCPU (SMP) for Path N

Date: 2026-09-30. Track C (platform). Status: design only — no code.

## 1. Core Problem

Path N runs exactly one vCPU. `MachineState.cpu` is a `Vec<CpuState>` with
one entry; `step_vcpu()` always steps `cpu[0]`; a guest `WFI` parks the
whole machine until console input arrives. That is fine for `pathn-sh`,
but an Android guest assumes SMP: the kernel boots an application
processor via PSCI/CPU_ON or a spin-table, uses SGIs (Software Generated
Interrupts) as IPIs (Inter-Processor Interrupts) for TLB shootdown and
rescheduling, and arms per-CPU virtual timers. A single vCPU cannot run
that guest — and even for the shell, one vCPU means the guest cannot
compute while waiting on input.

## 2. Key Insight

The state is already explicit: `MachineState` owns `cpu: Vec<CpuState>`
and all devices. SMP is therefore not a rewrite — it is **N copies of
`CpuState` plus synchronization points on the shared devices**. The CPU
core (decode → lift → block execute) does not change at all. The real
work is all in three shared devices that are currently global singletons:
the GIC (General Interrupt Controller), the timer, and the console/
virtio queues. Get those three right and the vCPUs themselves are
embarrassingly parallel.

## 3. Mechanism / How it works

**CPU bring-up.** Secondary vCPUs start parked (the same `WfiYield`
mechanism the single vCPU uses today). The guest releases them the ARM
way: a PSCI `CPU_ON` HVC (Hypervisor Call) or an MMIO spin-table release
address. New unit surface: `psci_cpu_on(cpu_id, entry_pc)` in the
orchestrator, trapped from the guest's HVC immediate. No threads are
created at bring-up — a vCPU is just an unparked `CpuState`.

**GIC (the biggest change).** Today's `IrqState { enabled, pending,
timer_count, timer_compare }` is machine-global. SMP needs:
- Per-CPU redistributor state: each vCPU gets its own `enabled`/`pending`
  bitmasks (SGIs 0–15 are banked per CPU by architecture).
- PPI (Private Peripheral Interrupt) 27 for the per-CPU virtual timer.
- SGI send: a guest write to the distributor's `SGI1R` targets a CPU
  bitmask; the orchestrator sets the pending bit on each target vCPU and
  wakes it if parked. This is the IPI path.

**Timer.** Split `timer_count`/`timer_compare` into per-CPU
`(count_offset, compare)` pairs driven by the existing virtual `CNTVCT`.
Timer fire sets the vCPU's PPI-27 pending bit instead of a global flag.

**Memory model.** RAM stays one shared `Vec<u8>` — guests already assume
a single coherent address space. Ordering: the interpreter executes one
block at a time per vCPU; a global block-execution lock (or per-vCPU
`BlockExecutor` instances with a shared code cache) keeps MMIO
device-state races out. Finer locking is a later optimization, not a
correctness requirement.

**Scheduling.** Replace "step cpu[0] until WFI" with a quantum loop:
each runnable vCPU executes up to K blocks per turn (K ≈ 1024, tuned by
the benchmark in `docs/BENCHMARKS.md`), then yields. Parked vCPUs wake
on: SGI targeting them, their timer compare firing, or console/virtio
input. Two modes, chosen at orchestrator construction:
- *Deterministic* (tests/CI): strict round-robin, fixed K — the
  determinism test stays green.
- *Threaded* (native perf): one OS thread per vCPU behind the
  block-execution lock. wasm32/browser host stays single-vCPU forever
  (no threads in the wasm sandbox) — SMP is a native-only feature.

## 4. Why it matters / Impact

- **Android boot is blocked without it.** AOSP kernels can limp on one
  CPU, but scheduler, RCU (Read-Copy-Update), and driver probe paths
  assume SMP; single-vCPU Android is a debugging configuration, not a
  boot target.
- **Responsiveness.** Input handling no longer waits for a long guest
  compute stretch: the quantum loop bounds any vCPU's uninterrupted run.
- **Measured, not hoped.** The quantum K and the lock granularity get
  tuned against `docs/BENCHMARKS.md` steps/sec, same rig as Track C.

## 5. Tradeoff

- **GIC emulation is the cost center.** Per-CPU banking + SGI routing is
  roughly as much code as the current whole `IrqState`; it is also the
  easiest part to get subtly wrong (missed wakeups hang the guest).
  Mitigation: SGI loopback tests (cpu0 → cpu1 → ack) before any guest
  needs them.
- **Determinism vs speed.** The threaded mode is measurably faster but
  cannot be golden-tested; the deterministic mode stays the CI gate.
- **Scope discipline.** No cache-coherence modeling beyond shared RAM,
  no device-tree/ACPI topology, no browser SMP. If Android needs it
  later, it becomes its own HLD.

## 6. Phased build plan

1. **Data model** — per-CPU `IrqState`, PSCI `CPU_ON` trap stub, second
   `CpuState` constructible; single-vCPU behavior bit-identical
   (existing boot tests green).
2. **Deterministic scheduler** — round-robin quanta, wake on SGI/timer/
   input; new tests: 2-vCPU counter race, SGI loopback.
3. **GIC distributor** — `SGI1R` routing, per-CPU PPI timers.
4. **Threaded native mode** (optional, perf-gated) — only if phase 2
   numbers say the lock is the bottleneck.

## 7. Non-goals

Browser/wasm32 SMP, real hardware topology, fine-grained device locking,
any change to the block-execution contract (`BlockExecutor` untouched).
