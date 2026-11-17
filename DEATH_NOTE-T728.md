# Death note — T728 observable-payload service

T728 died on a new wall: SERVICE-SELINUX-LABEL-REFUSED.

The service-flight ran the full 600 s (guest 600.263 s, host 601.02 s, QEMU
exit 0). The trigger fired (`sys.boot_completed=1`, first guest 563.938 s),
the registration marker fired twice (`T728_PROBE_OK` guest 109.049 s and
569.563 s), and init processed the action from `t728probe.rc:12` — but the
`start t728probe` command failed at guest 569.484 s: the binary
`/system/bin/t728probe` is labeled `u:object_r:unlabeled:s0` with no domain
transition from `u:r:init:s0`, so init refuses it even in permissive mode.
`T728_SERVICE_OK` 0, `T728_ARGV` 0 — the payload never ran.

The T727 exit-127 question is unanswered: the quoting hypothesis was never
tested because the direct exec never ran. The fix is to label the binary and
re-fly. EGL dispatch R1/R3/R4 stays open.

Verdict: FAIL (see findings-t728.md).
