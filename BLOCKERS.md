# Blockers and boot checkpoints

- Phase A | halt_step=none (2,000,000-step standard budget) | `ID_AA64MMFR0_EL1` now returns QEMU's `0x1122`; standard boot report and UART remain unchanged.
- Phase B | halt_step=none (2,000,000-step standard budget) | `MIDR_EL1=0x410fd034`, `MPIDR_EL1=0x80000000`, and `CTR_EL0=0x84448004`; feature-MRS logger observed 11 reads across 7 selectors, with standard boot/UART unchanged.
- Phase C | halt_step=none (2,000,000-step standard budget) | Remaining 21 T39 mismatched selectors now return their QEMU Cortex-A53 values; standard boot/UART remains unchanged.
