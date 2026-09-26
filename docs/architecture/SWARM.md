# SWARM — Parallel Build Plan (Path N)

Goal: 14 LLD units ko ek agent swarm parallel mein banaye, bina ek-doosre ke pairon par pair rakhe.
Ek hi coupling hai — frozen contracts (`LLD.md` section 0). Isliye **contracts pehle, code baad mein.**

## Wave 0 — Contract freeze (single owner, koi parallel nahi)

- Owner: Gaurav (ya design lead agent). `LLD.md` ke schemas final honge.
- Exit criteria: har schema ke liye ek `contract-test` jo sirf types check karta hai (koi logic nahi).
- Iske bina Wave 1 shuru nahi. Yehi ek serialization point hai poore plan mein.

## Wave 1 — Pure units (full parallel, zero inter-dependency)

Har agent sirf contract + apna unit dekhta hai. Kisi ko kisi ka wait nahi.

| Agent | Unit | Notes |
|---|---|---|
| A1 | U1 `aarch64-decode` | Fuzz test mandatory |
| A2 | U2 `arm-ir-lift` | Golden IR snapshots |
| A3 | U3 `wasm-jit` | Byte-identical determinism test |
| A4 | U11 `snapshot` | Round-trip property test; `MachineState` schema Wave 0 se |
| A5 | U10-analyzer | EXISTS — sirf contract conformance check, koi rewrite nahi |
| A6 | U14 `metrics` core | EXISTS — pure aggregation isolate karo |

Definition of done (har unit): contract signatures implemented + unit tests green +
`units/<id>/CONTRACT.md` mein conformance note (kaunsa schema version, kya cover nahi hua).

## Wave 2 — Explicit-state units (full parallel)

State structs Wave 0 mein frozen hain, isliye ye bhi ek-doosre ka wait nahi karte.

| Agent | Unit | Notes |
|---|---|---|
| B1 | U4 `mmu` | Hand-built page table golden tests |
| B2 | U5 `gic-timer` | IRQ number exactness |
| B3 | U6 `virtio-transport` | Path E ke 311 tests migrate karo |
| B4 | U7 `virtio-gpu-device` | **3D accept test** — Path E wala "rejected" test flip hoga |
| B5 | U8 `gpu-host-stack` wiring | EXISTS code; naya kaam = U7 se `GpuCmd` lekar canvas tak ka path wire karna + triangle golden test |

## Wave 3 — Integration & long poles (parallel, slow wale pehle shuru karo)

| Agent | Unit | Notes |
|---|---|---|
| C1 | U9 `guest-image` | **Longest pole** — AOSP build hours leta hai. Wave 3 ke din 1 par shuru karo. Pehle prebuilt kernel + minimal rootfs se kaam chalao, full system.img baad mein. |
| C2 | U13 `browser-adapters` | Mock-first: har adapter ka mock Wave 3 mein, real-browser smoke M2 mein |
| C3 | U10-sideload | Mock-adb test; real guest M2 mein |
| C4 | Unicorn-WASM spike | HLD M0 — full-system ka shortcut; U12 ke aane tak standalone demo |

## Wave 4 — Orchestrator (LAST, single agent)

- Agent D1: U12 `orchestrator`. Sirf tab shuru jab Wave 1+2 ke contracts implemented hain.
- Kaam: vCPU threads, event routing, explicit state threading. Koi business logic nahi —
  review mein logic dikha to wapas bhej do (galat unit mein hai).
- Acceptance: boot-to-shell integration + determinism test (injected clock).

## Swarm laws (violation = build fail)

1. **No cross-imports.** Unit sirf `contracts/` (schemas) import karega.
2. **Impurity quarantine.** `now()`, threads, I/O, randomness — sirf U12/U13. Kahin aur dikha to red flag.
3. **Contract change protocol.** Schema badalna hai to: version bump + `#swarm` announcement +
   affected units ke agents ko explicit re-ack. Silent change = revert.
4. **Evidence gates.** M0–M4 gates (HLD section 6) measured evidence mangte hain —
   boot log, frame screenshot, fps number. "Mera unit complete hai" gate pass nahi karata.
5. **One-line status.** Lambe kaam mein har agent ek line ka progress deta rahega —
   silence stuck jaisa lagta hai.

## Suggested immediate next actions

1. Wave 0: `LLD.md` contracts ko `contracts/*.rs` mein freeze karo (single PR).
2. Wave 1 agents launch karo (A1–A6 parallel).
3. C1 (guest-image) day 1 par shuru — longest pole hai.
4. Unicorn-WASM spike (C4) — M0 ka fastest path to "kuch chal raha hai".
