# DEATH NOTE — T717 (0x0 buffer-allocation wall: T628 shim parser fix)

Verdict: FAIL

WALL-TYPE-TAG: guest
Rust par bhi lagu: haan

## What died

The T717 mission (nonzero allocation + queueBuffer > 0 + first present
dispatch) was not achieved. No flight-01 was flown: the Step 3a smoke
criterion (REJECT 0 AND GraphicBufferAllocator failures 0) was not met
in any of four smokes, and the brief gates Step 3b on Step 3a.

## What was proven (the parser wall is broken)

The diagnosed wall — the T628 AIDL allocator shim rejecting every
`allocate2` because its `read_size_prefixed_descriptor` could not parse
the framework's `BufferDescriptorInfo` parcelable — is BROKEN and PROVEN
broken. Smoke #4 (v4 parser, 13,600 lines, SHA 4c877ebd, 17:00:33Z–
17:05:33Z) has REJECT 0 with the shim exercised, and the shim logged:

    L10355: CALL method=allocate2 count=1 width=1280 height=720

The v4 parser parsed the descriptor correctly (flag + include-header
size + 128-byte array + width/height/layerCount/format + size-skip),
on_transact proceeded, and write_result ran without crashing. The wire
format was proven from the guest's own allocator-V2-ndk.so disassembly,
not from convention. The v4 fixed binary is bf8661d0 (380 bytes differ,
footprint-only); the v4 images are vendor 5d2d82b0, CPIO fceef5df, DTB
1a961795. All T716 holds stand (EPERM 0, installd trigger, onBootPhase
return, SYS_RAWIO fix intact).

## The new wall (first failing link)

`write_result`'s reply path in the shim's `on_transact` code-2 handler,
AFTER the CALL log and BEFORE the framework's BpAllocator reads a valid
reply. Smoke #4 L10684:

    GraphicBufferAllocator: Failed to allocate (1280 x 720) layerCount 1
    format 1 usage 1b00: -129

-129 is EX_TRANSACTION_FAILED. The shim did not crash (zero fatal-signal
/ crash_dump lines, single REGISTERED, alive for the rest of the
flight), no binder diagnostic exists anywhere in the UART, and the
parser / dispatch / lifecycle / caller are all exonerated. The failure
is inside write_result (0x11a58) or the success-path epilogue: the reply
status header, the returned AStatus, or the NativeHandle /
AllocationResult content failing driver-level validation.

## Next fix (successor lane)

1. Disassemble write_result (0x11a58–0x11f3c) + the on_transact code-2
   success epilogue in the original binary (745d19d0): (a) is
   AParcel_writeStatusHeader called on the success path, with what
   status? (b) what AStatus is returned? (c) how is the NativeHandle
   built (numFds/numInts/data vs native_handle_t, memfd fd placement)?
   (d) what AllocationResult struct goes to AllocationResult::writeToParcel?
2. Compare with the framework's reply-read expectations in
   BpAllocator::allocate2 (V2-ndk.so, 0x11998 onward).
3. Smallest fix in write_result (second footprint patch, or a
   diagnostic-logging shim build dumping the reply shape to kmsg if the
   disassembly is inconclusive). Do NOT rework the parser: v4 is proven.
4. Re-smoke (REJECT 0 AND GBA-fail 0), then flight-01 + control.

Full evidence, counts, and gate lines: findings-t717.md (sections 5d, 8,
9, 10). Four smokes: #1 v1 c0d63737 (REJECT 1), #2 v2 0e048c1c (REJECT
1), #3 v3 6af2f8b9 (REJECT 0, crash — lane-agent frame bug, fixed in
v4), #4 v4 4c877ebd (REJECT 0, -129 reply failure).

## Reflection (condensed; full text in findings-t717.md section 11)

R1. The parser diagnosis was wrong twice (field model, skip, layout)
because it reasoned from convention instead of disassembling the
framework library first. Change: framework disassembly is the FIRST step
for any wire-format fix; convention-only diagnoses are INFERENCE and
cannot authorize a build.
R2. The v3 parser crashed on its own success path (stack slots [sp,#8]
8-byte vs [sp,#12] overlapped; latent through v1/v2, fatal at the first
success). Change: asm frames carry explicit byte-range comments; reviews
gain a slot-disjointness check from assembled offsets. Applied in v4.
R3. Clock estimation, third occurrence (16:47Z label ~12 min fast).
Defect, reported plainly. Mechanical guard, applied: timestamps only
from a `date` read inside the same command that writes them.
