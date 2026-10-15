# T503 ORN fast-path fix

## Question

Does the U12 logical shifted-register fast path implement ORN's N-bit operand inversion, and does correcting it remove the live Rust/QEMU state divergence observed one instruction before T499's CSINV breakpoint?

## Root cause

The live T499 traces showed different X8/NZCV at CSINV, but the matched-state control showed that CSINV itself was correct. The first divergence was the preceding word `0xaa2103e8`, `ORN X8, XZR, X1`. U12 decoded N but the ORR fast path ignored it, executing ORR. For X1=`0x40000000`, this left X8=`0x40000000`; ORN must produce `0xffffffffbfffffff`.

The U12 ANDS and BIC fast paths already honor N. No EOR/EON fast path exists there. EON falls through to U2's `EorShift { invert: n }`, which U3 implements by inverting the shifted operand.

## Change and checks

- Fix commit: `e29771b` (`fix: honor invert bit in ORN fast path`). For both `sf=1` and `sf=0`, U12 now complements the shifted operand when N is set before ORing it with Rn.
- Added `gb25_orn_honors_inverted_shifted_operand`: 64-bit ORN and ORR, 32-bit ORN and ORR, and shifted 64-bit ORN. It failed before the fix on the ORN X result and passes after it.
- Runner diagnostics compose split NZCV/DAIF state into architectural EL1h PSTATE so the live comparison reports the same state QEMU exposes. This diagnostic adjustment is commit `788357a`.

## Pinned-oracle outcome

The unchanged T499 `MANIFEST.json` validates. QEMU remains pinned to cortex-a53, 1 GiB, QEMU 11.1.2, and the manifest's Image, DTB, and initramfs hashes. Rust was rebuilt from this fixed branch; no guest-quirks flag was passed, and the log says `enabled=false`.

At the live second hit of CSINV word `0xda818058` at `0xffffff8008354018`, QEMU and Rust both have X1=X2=`0x40000000`, X8=`0xffffffffbfffffff`, X24=`0xffffff8009512000`, and architectural PSTATE=`0xa00001c5`. After one instruction, both have PC=`0xffffff800835401c`, X24=`0x40000000`, and the same PSTATE. Ten fresh QEMU/Rust paired captures passed consecutively with those same states.

The guest kernel/initramfs/userspace markers are retained from the actual pinned QEMU guest UART capture; harness text is not used as guest evidence. The ten targeted breakpoint pairs stop before those later markers and are accepted on exact pinned input verification and QEMU/Rust register-state agreement.

## Limits

The requested full workspace test run reports 349 passes and one failure in `vmemmap_l1_gap_step_vcpu_instruction_retry`, an LDRB/MMU translation-path test unrelated to logical-register execution; an isolated rerun reproduced that same translation fault. The focused ORN test, release build, manifest validation, live oracle comparison, and ten-pass bar pass. The standing-rules file was unavailable at its supplied path, so the rules quoted in the task were applied directly. No `tools/validate_okf.py` exists in this checkout.

Raw derived captures and hashes: `/Users/Shared/bundles/t503-ornfix-20261015/`. The detailed acceptance ledger is [`GATES-T503.md`](../../GATES-T503.md).
