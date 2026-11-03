# T674 death note

Stopped after input verification, T673 flight re-verification, and a
read-only audit of the actual composer ELF embedded in `vendor.img`.

The FMQ failure is confirmed, but T673's second-half claim needs correction:
the ELF contains one conditional static call to `blitFromBuffer` at `0x688a8`.
It is reachable only for nonempty `inHandles`, outside an FMQ-parsed
`PRESENT_DISPLAY` path. The empty-handle branch at `0x68820` skips both FMQ
queues and returns zero output words at `0x688d0`.

The service source and Android command-engine build dependencies are absent
from this worktree. A correct fix requires command parsing, FMQ result
serialization, and client-target dispatch. I could not produce a defensible
single fix initramfs from the available binary-only artifact, so no image was
built and no QEMU boot was started. T674 flight checks and regression
re-measurements remain unobserved; see the findings file for the T673
historical baseline and exact citations.

The requested lane commit is also blocked by the workspace sandbox: `git add`
could not create the shared worktree `index.lock` under the read-only common
Git directory (`Operation not permitted`). The findings and audit files remain
in the worktree, unstaged and uncommitted.
