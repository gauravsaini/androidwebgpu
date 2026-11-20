# DEATH NOTE — T731 (start-leg discrimination)

Verdict: FAIL. Wall: SERVICE-START-SILENTLY-FAILED (guest, T726 recurring),
now proven for BOTH start legs.

## What died

The on-boot direct `start t728probe` failed silently — 0 `t728probe` lines,
0 `init starting service` lines, 0 exit lines, 0 refusals — in an action
init demonstrably executed: its write command fired `T728_PROBE_OK` at
guest 168.989 s (L10703, leg 1). T730 proved the property-action dispatch
fails silently; T731 proves the on-boot start fails identically silently.
The fault is in the service definition / start path itself, not the
dispatch leg.

Secondary wall: BOOT-COMPLETED-NEVER-FIRED (guest, T729 recurring). The
trigger never fired (`sys.boot_completed` 0; L2418 `[T723] boot_completed
never detected in 600s`). The guest ran ~20% slower than T730 (guest/host
0.80 vs 1.00), so the 800 s host cap fired at guest 641.31 s and the 700 s
guest target was missed; at T730's pacing boot_completed would have landed
near guest ~800 s. Leg 2 (property action's write) is therefore UNTESTED
in T731, and the T721/simplefb holds are unmet because the boot never
reached the display stack.

## What the successor must do (when the Mac pause ends)

Discriminate the start path itself, not another dispatch leg: the next
probe must make the service start observable from inside the guest — a
manual start from a shell, or a service definition that differs in
exactly one respect (e.g. a different binary path, a class assignment, or
an `exec` instead of a service), with the on-boot write kept as the
execution proof. The flight window must also account for guest pace: add
a first-poll pace check, because a 0.80 guest/host ratio already loses
the 700 s target inside the 800 s host cap.

## Evidence

- Flight: service-flight 20:58:00–21:11:22 AEDT 19 Nov 2026, host 801.72 s,
  guest 641.31 s, QEMU exit 0, UART 15,258 lines / 1,611,633 B, SHA256
  8b2d922ca9ac081c0d67332e71554b9db6c38dfa9086c800d52bdf4d2091d463.
- Image 15c79aad, rc 053189d0 (parser ACCEPT, 0 errors), binary 19db5d31,
  label u:object_r:system_file:s0 read back from the flown image.
- T728_PROBE_OK x1 (168.989 s, L10703). T728_SERVICE_OK 0. T728_ARGV 0.
  SELinux refusals 0. Panic/Oops/BUG 0.
