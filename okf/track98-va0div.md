# Track 98 — VA0 first-divergence investigation

**Verdict: FAIL.** The earliest available aligned observation already has a PC mismatch at step 60,000,000, but the first causal instruction and full QEMU architectural state were not captured. H2 is resolved: T38 was not merged; the current emulator does recognize and execute this LDXR instruction through the orchestrator path.

## Scope and inputs

- Worktree: /Users/Shared/wt-track98-va0div
- Source revision: 05561be3a1de38e9eddc7842102f0f2eefc412e9
- Release build: succeeded; target/release/run_kernel SHA-256: 5ef2c32e0d2e001bc7fef0b777c0671b6485a1bdf9dda5495b61f4a4a9259697
- Guest Image SHA-256: 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
- T83 frontier SHA-256: 650a289c088a0fd29afcd4591272fbcc1e60859af0fac53930c4c0757c0c4bb4
- Frontier format: PNFS v1 / U11 v15, one CPU, saved at step 60,000,000.
- No emulator source was changed.

## H1 — first-divergence evidence

The earliest comparable checkpoint available is the 60M frontier. The emulator snapshot contains PC 0xffffff80082430c0. The archived QEMU instruction trace records PC 0xffffff8008089a14 and word 0xb80044c3 at instruction count 60,000,000; that word decodes as STUR W3, [X6, #4]. The emulator instruction word at its saved PC is 0xa9022d4c.

| Nominal step | Emulator frontier | QEMU archived trace |
|---:|---|---|
| 60,000,000 | PC 0xffffff80082430c0; next word 0xa9022d4c | PC 0xffffff8008089a14; word 0xb80044c3 |

This proves the architectural PCs differ at the earliest available checkpoint. It does not establish that step 60,000,000 is the first actual divergence: there is no emulator instruction-by-instruction trace leading into this snapshot, and the snapshot may already encode an earlier difference. QEMU’s archived row records the instruction at the named count, while the emulator snapshot records saved machine state; exact retired-instruction edge alignment is not independently established. The trace also uses a higher-overhead QEMU plugin, which the T85 findings note can perturb timer timing.

The T83 snapshot CPU state was parsed directly. ESR_EL1 and FAR_EL1 are not fields in this U11 v15 snapshot, so they cannot be compared at the frontier.

~~~text
SP(current) = 0xffffff8008003b90
SP_EL1(snapshot bank field) = 0x0000000000000000
SP_EL0 = 0xffffff800956d880
PC = 0xffffff80082430c0
PSTATE = 0x00000000800003c5
DAIF = 0x0000000000000080
SCTLR_EL1 = 0x0000000034f5d91d
VBAR_EL1 = 0xffffff8008082000
MAIR_EL1 = 0x0000bbff440c0400
TCR_EL1 = 0x00400032b5593519
TTBR0_EL1 = 0x00000000416e9000
TTBR1_EL1 = 0x00000000416ea000

x0=0x0000000000000000  x1=0x0000000000000000  x2=0xffffff80096b48c0
x3=0xffffff8008003da0  x4=0x0000000000000000  x5=0x0000000000000000
x6=0xffffff80096b4890  x7=0x0000000000000017  x8=0x000000055ae82600
x9=0x000000055ae82600  x10=0xffffff80095817c0  x11=0x00000005f5e10000
x12=0x000000083d090000  x13=0x0000000000000008  x14=0x00000005f5e10000
x15=0x0000000000000000  x16=0x0000000000000000  x17=0x0000000000000000
x18=0xffffffc03efd1060  x19=0x0000000000000000  x20=0xffffff80096b48c0
x21=0x0000000000000000  x22=0xffffff80096b48c0  x23=0xffffff80096b48e0
x24=0x0000000000000000  x25=0xffffff80096b48d0  x26=0xffffff80096b4000
x27=0x0000000000000000  x28=0x0000000000000080  x29=0xffffff8008003b90
x30=0xffffff8008243078
~~~

The archived QEMU trace is /private/tmp/track85-qemu-transition.bin.gz, with count metadata in /private/tmp/track85-qemu-transition.meta. Its rows immediately around the checkpoint are:

~~~text
59999996 PC=0xffffff8008089988 word=0x540003c3
59999997 PC=0xffffff8008089a00 word=0x36180062
59999998 PC=0xffffff8008089a0c word=0x36100062
59999999 PC=0xffffff8008089a10 word=0xb8404423
60000000 PC=0xffffff8008089a14 word=0xb80044c3
60000001 PC=0xffffff8008089a18 word=0x36080062
~~~

A fresh QEMU register-capture plugin keyed to the 60M sample did not produce a sample in either a 180-second run or a bounded 600-second run. Therefore QEMU GPRs and system registers at this checkpoint remain unavailable. The existing Track 66 QEMU UART/MMIO traces do not contain architectural register state.

The exact-revision T85 replay from the same 60M snapshot is recorded in /private/tmp/track95-pre-fix.log. It ran to global step 123,811,609 and stopped at PC 0xffffff80082591e4. The register dump has X10=0; the current instruction word is 0x885f7d4b (bytes 4b 7d 5f 88), LDXR W11, [X10]. The emulator reports TranslationFault { va: 0 }. This confirms the downstream failure, not the earlier cause of divergence.

The focused T95 QEMU probe reports an architectural Data Abort at that instruction (ESR_EL1=0x96000005, FAR_EL1=0, vector PC 0xffffff8008082200), then continues. Thus QEMU does take and handle the architectural abort; it does not halt there like the pre-fix emulator.

The low-overhead QEMU boot reaches “Run /init as init process” at step 140,855,341 and STATUS: PASS at 141,061,535, per /Users/Shared/codex-track85-diff.md.

### H1 assessment

**FAIL — exact first causal divergence not established.** The earliest measured mismatch is the PC at the nominal 60,000,000 checkpoint (emulator 0xffffff80082430c0 vs QEMU 0xffffff8008089a14). This is an observation of already-divergent execution, not proof of the instruction that first caused the divergence. No single causal instruction can be named from the available traces.

The required fresh replay to about 122.8M was not practical within the brief’s minutes-scale budget: a 1M-step release continuation from the snapshot took 236.72 seconds and reached the 61M step limit. Linear extrapolation for the remaining 62.8M steps is about 4.1 hours. An exact-revision replay to the known 123.8M fault already exists, but it did not save aligned full state at 122.8M.

## H2 — T38 and fault-instruction decode

**T38 merged: NO.** The worktree history contains no T38 commit. The available Track 38 document describes a static audit and explicitly says no repository source files were changed.

The current source does handle the faulting instruction:

| Layer | Result |
|---|---|
| Arm encoding | 0x885f7d4b is LDXR W11, [X10]: 32-bit load, base X10, destination W11. |
| U1 decoder | units/u1-decode/src/ldst.rs recognizes the exclusive-load encoding and returns LoadStore. |
| U12 orchestrator | units/u12-orchestrator/src/lib.rs has a direct LDXR path that translates the base address, loads the value, zero-extends into the destination, and records the exclusive monitor. |
| Generic U2 lifter | units/u2-ir-lift/src/lib.rs emits an explicit R_EXCLUSIVE trap for LDXR because the generic lifter lacks orchestrator monitor state. |

The direct Arm reference describes LDXR Wt, [Xn|SP] as a 32-bit exclusive load: [Arm Instruction Set Reference Guide, D3.74 LDXR](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab). The fault at this instruction is therefore not evidence of a missing decode. At the recorded emulator fault, X10 is zero and address translation fails.

## Evidence and limits

- The release build succeeded; warnings were pre-existing unused/dead-code and unreachable-pattern warnings.
- A 1M-step continuation from the T83 snapshot reached the 61M budget without a halt; it was not a full state-diff run.
- QEMU’s archived instruction trace supplies PCs and words but not registers. The focused state-capture attempt timed out without a matching callback.
- The QEMU trace instrumentation can change timer timing; checkpoint PC comparison should not be treated as a deterministic low-overhead oracle.
- No source fixes or tests were added for this investigation.
- tools/validate_okf.py is absent from the worktree, so OKF validation could not be run.

## Verdict

**FAIL.** H2 is complete. H1 identifies the earliest available checkpoint mismatch, but lacks the register-by-register QEMU state and preceding emulator trace needed to establish the exact first divergent instruction.
