# T674 — Composer 2.1 FMQ and framebuffer dispatch recovery

## Branch status

**Verdict: FAIL (recovery stopped before fix-image build or guest flight).**

T674 begins from the verified T670 base on the T673 display-wall lineage. Input
SHAs for the base image, kernel, embedded `system.img`, QEMU, and reference UART
logs matched. T673 raw line 43788 and the 555 `presentOrValidate` failures
reconfirm Link 4 as the first failing link. The four framebuffer snapshots are
allocation evidence, not proof of a committed frame.

## Binary audit correction

The actual composer executable inside the base image's `vendor.img` has the
same SHA as the top-level CPIO copy. Its `executeCommands` symbol starts at
`0x687ec`; an empty handle vector branches at `0x68820` to a callback at
`0x688d0` with zero output words and never reads or writes its FMQs. There is
one static `blitFromBuffer` call at `0x688a8`, conditional on nonempty handles.
It is not driven by parsed presentation commands. This corrects T673's static
“zero callers” claim without changing T673's historical record.

## Stop condition

The available artifact is a binary-only SimpleFB service; the worktree lacks
its implementation source and Android build dependencies. A correct fix needs
FMQ command parsing, result serialization, and dispatch from the presentation
path. No fix image or guest flight was produced, and no T674 regression vector
was remeasured. T674 `sys.boot_completed` is therefore **not observed**. The
T673 historical `sys.boot_completed=1` marker belongs to cycle 2 (raw line
64195), after cycle 1's Watchdog kill.

See `evidence/t674_composer_binary_audit.md` and
`/Users/Shared/agyx-track674-composer-2-1-executecommands-fmq-and-blit-fix.md`.
The requested commit was attempted but the sandbox denied writing the shared
Git worktree index lock; these branch notes remain uncommitted.
