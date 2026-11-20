# DEATH NOTE — T732 (exec-probe discrimination)

Verdict: FAIL. Wall: BINARY-EXEC-SILENTLY-FAILED (guest, NEW in T732).
This supersedes SERVICE-START-SILENTLY-FAILED (T726 recurring) as the
fault location: the start path was a symptom, not the fault.

## What died

The on-boot `exec /system/bin/t728probe` returned — the write on the next
line of the same action fired `T728_PROBE_OK` at guest 122.69 s (L10593),
and `exec` is synchronous, so the exec had returned by then — but the
binary produced no `T728_SERVICE_OK`, no `T728_ARGV`, and init logged no
exec line for it at all (no "exec N" start / SVC_EXEC / exit; execs 11 ->
16 -> 18 bracket the window). The property leg's `start t728probe` failed
silently too, this time with the trigger fired: `sys.boot_completed=1` at
guest 646.03 s (L22683), the property action's write fired at guest
653.26 s (L23016), but 0 `init starting service 't728probe'` lines and 0
service EXIT lines. Both mechanisms fail identically. The fault is in the
binary's execution itself, not the start path and not the binary bytes:
the binary is a verified-good static aarch64 executable (mode 0755,
label u:object_r:system_file:s0, SHA 19db5d31, no INTERP/DYNAMIC) that
init cannot observably execute under either `start` or `exec`.

The trigger leg is FIXED in T732: boot_completed fired at guest 646.03 s
(first time since T730), the guest ran at ~1.00 pace and reached the full
850 s target (851.001 s). The widened 850/1100 window + first-poll pace
check closed the T729/T731 secondary wall BOOT-COMPLETED-NEVER-FIRED.

Secondary observation (not the cause): system_server (pid 2920) was
killed by signal 9 at ~guest 725 s, after both probe legs; all apps died
with DeadSystemException (FATAL EXCEPTION x6). The guest's system_server
died before the flight ended — a separate boot-instability signal.

## What the successor must do

Discriminate whether init even attempts the execve of this binary. The
candidates: a minimal test binary that writes a marker from _start with
no libc (rules out a libc/startup failure in the payload), or init-side
tracing of the exec dispatch (rules out init dropping the exec before
fork). Do NOT re-test the start path or the trigger leg — both are now
settled (start fails silently with the trigger fired; exec returns
without a marker). The binary's bytes, mode, and label are all verified
good, so the fault is in the exec dispatch or the runtime environment,
not the artifact. Keep the on-boot write as the execution proof and the
first-poll pace check; the 850/1100 window is now proven sufficient at
1.00 pace.

## Evidence

- Flight: service-flight-2 19:50:03–20:04:15 AEDT 20 Nov 2026, host
  851.56 s, guest 851.001 s, QEMU exit 0, UART 31,938 lines / 3,748,681 B,
  SHA256 395bedf378a84283501866713724ab9966e40ec8e363e0d976e00fc14589e4b1.
- Image f7980cfb, rc f9baa479 (parser ACCEPT, 0 errors), binary 19db5d31,
  label u:object_r:system_file:s0 and mode 0755 read back from the flown
  image.
- T728_PROBE_OK x2 (122.69 s, L10593; 653.26 s, L23016).
  T728_SERVICE_OK 0. T728_ARGV 0. SELinux refusals 0. Panic/Oops/BUG 0.
  FATAL EXCEPTION 6 (all DeadSystemException, post-leg system_server death).
