# OKF log

## 2026-10-08 — Track 98: VA0 first-divergence investigation

- Added the Track 98 investigation as its own branch record in [track98-va0div.md](track98-va0div.md), preserving it as a parallel experiment family.
- Earliest available aligned observation is the nominal 60M checkpoint: emulator PC 0xffffff80082430c0, QEMU trace PC 0xffffff8008089a14.
- The exact first causal instruction remains unproven because there is no preceding emulator instruction trace or full QEMU register capture.
- H2 found no merged T38 commit; the current U1 decoder and U12 orchestrator do handle LDXR W11, [X10].
- Overall verdict: FAIL for the exact H1 first-divergence requirement.
