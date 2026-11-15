# DEATH NOTE — T718 (write_result reply path: the missing AIDL null flag)

Verdict: FAIL

WALL-TYPE-TAG: guest
Rust par bhi lagu: haan

## What died

The T718 mission (nonzero allocation + queueBuffer > 0 + first present
dispatch) was half achieved. Nonzero allocation was achieved and proven
at scale in the guest. queueBuffer > 0 and first present dispatch were
NOT achieved: flight-01 shows queueBuffer 0, presentDisplay 0, and no
present dispatch in 323,607 lines. The lane therefore ends FAIL per the
brief's Step 3b criteria, with the exact first failing link named below.

## What was proven (the -129 wall is broken)

The diagnosed wall — the T628 AIDL allocator shim's `write_result`
failing every `allocate2` with -129 (EX_TRANSACTION_FAILED) — is
BROKEN and PROVEN broken. Framework-first disassembly proved the
cause: `write_result` called `AllocationResult::writeToParcel`
without writing the AIDL non-null int32 flag the framework's Bp
reply reader requires first. The fix (17-instruction epilogue
rewrite + 4-instruction flag trampoline in the dead
`AParcel_readInt64` PLT stub) was spliced into the v4 binary
(3daa5a49, 73 bytes differ, parser footprint byte-identical) and
shipped (vendor bac3cdc1, CPIO 1150c841, DTB d298f5ad). The smoke
PASSED (REJECT 0, CALL 2, GBA-fail 0, importBuffer x2), and flight-01
shows 604 allocate2 calls, 695 imports, 0 REJECT, 0
GraphicBufferAllocator failures, 0 allocateBuffers failures. All
T716 holds stand; Dexopt PERFORMED for Launcher3; sys.boot_completed
reported 11 times; the canonical-kernel control matches the T716
pattern exactly.

## First failing link

The present path between SurfaceFlinger's Scheduler and the
composer/display. Every frame the Scheduler tried to track carried
an invalid present fence: 2241 identical
`E/Scheduler: trackPendingFrame: Invalid present fence` errors,
first L23250, last L323495, spanning the whole flight. No buffer
was ever queued or presented in the observable UART. The display
formed (1280x720, state ON) and the composer received 704 command
batches, but no guest-origin frame commit is evidenced.

## Open items

1. Successor lane: determine why every present fence is invalid
   (Scheduler fence-validity check + producer fence origin,
   framework-first disassembly), discriminate whether an invalid
   fence drops the frame or only skips tracking, and apply the
   smallest fix at the proven link. Do not rework the v4 parser or
   the T718 flag fix.
2. GateKeeper NPE parallel wall (separate named lane): stub/guard
   the GateKeeper service so LockSettingsService acquires it. One
   system_server death in T718; all 6 FATALs DeadSystemException
   collateral.
3. Futex regression vector unobserved (0 matches in both flights);
   the control exits at 20.19 s, before the probe point. Not a lane
   defect; recorded for the driver.
