# T732 findings — exec-probe discrimination (on-boot exec of the labeled probe)

Verdict: FAIL

## Flight

- service-flight-2, 2026-11-20 19:50:03–20:04:15 AEDT (08:50:03–09:04:15Z).
- Host 851.56 s, guest 851.001 s, QEMU exit 0. Stop reason: guest target
  reached (851.0 s >= 850 s). The guest target WAS reached — the widened
  850/1100 window worked, and the guest ran at ~1.00 pace (851.001/851.56),
  unlike T731's 0.80.
- Flight name is service-flight-2, not service-flight: flights/service-flight/
  in this worktree holds the COMPLETED T731 flight (committed in 18d3a6c),
  and reusing the name would have overwritten it — forbidden by the standing
  no-overwrite rule and the supervisor's own docstring. The T731 folder was
  left untouched.
- UART: 31,938 lines, 3,748,681 B, SHA256
  395bedf378a84283501866713724ab9966e40ec8e363e0d976e00fc14589e4b1.
- Flown image: t732-system.img SHA256
  f7980cfba319fdb23764508664fa5dd1ea7e290ae9be72c3863cdc70c94ed5d7
  (T731 image + new rc only; binary untouched, 19db5d31).
- New rc: t728probe.rc SHA256
  f9baa4799e55882448d83b569828e7596a809264d24f52c2dcb0a5f45e72a123
  (722 B; T731 rc + one line: on-boot `start t728probe` -> `exec
  /system/bin/t728probe` at line 18; property action unchanged at line 12).
  The exec is BEFORE the write in the on-boot action (line 18 exec, line 19
  write) — so the write firing proves the exec returned.
- Parser citation: parse_init_rc.py on the exact new rc bytes — VERDICT
  ACCEPT, 0 errors, 1 service, 2 actions, 0 imports. (Positive control: the
  T731 rc bytes 053189d0 also ACCEPT, exit 0.)

## Adjudication (brief's 7-item order)

1. On-boot exec: RETURNED, binary produced nothing. The exec (line 18) is
   synchronous and the write (line 19) fired at guest 122.69 s (L10593),
   proving the exec RETURNED by then. But `T728_SERVICE_OK` 0,
   `T728_ARGV` 0, 0 `t728probe` lines in the UART, 0 init exec lines for
   t728probe (no "exec N" start / SVC_EXEC / exit; execs 11 -> 16 -> 18
   bracket the window, no 12/13/14/15 lines), 0 service exec/start
   failures, 0 init service-refusal lines. Init was NOT blocked by the
   exec: it ran exec 16 (bootstat, guest 126 s), exec 18 (guest 254.9 s,
   exit 0), and started artd/drm/bootanim after the probe. The exec
   dispatch returned; the binary did not observably run.
2. Trigger: FIRED. `sys.boot_completed` 8 lines. First at guest 646.03 s
   (L22683, hw/init.rc:1224). Stock boot_completed actions processed +
   succeeded (rm -rf /data/per_boot, bootstat record, dmesgd,
   flags_health_check, wifi) — the dispatch control works. This is the
   first time the trigger has fired since T730 (T729 and T731 both had
   BOOT-COMPLETED-NEVER-FIRED). The widened window + 1.00 pace fixed the
   trigger leg.
3. Registration legs: BOTH PROVEN. `T728_PROBE_OK` x2: L10593, guest
   122.69 s (on-boot action) and L23016, guest 653.26 s (property action).
   Both actions' writes fired, proving both actions executed. But the
   property leg's `start t728probe` produced no service start (0 `init
   starting service 't728probe'` lines, 0 service EXIT lines) — the same
   silent failure as T731, now with the trigger fired. And the on-boot
   leg's `exec` returned without a marker. Both legs' payload invocations
   failed silently.
4. argv: NONE. `T728_ARGV` 0 lines, `T728_ARGC` 0 lines. The binary never
   ran, so no argv handoff exists to quote. There is no argv evidence.
5. `T728_SERVICE_OK`: 0 lines. The payload never ran. This is the verdict
   condition: PASS iff `T728_SERVICE_OK` appears. It does not.
6. SELinux refusal: 0 lines. 0 init service-refusal lines, 0
   label/domain-transition lines. The binary's label was read back from
   the flown image via box debugfs before the flight:
   `security.selinux (25) = "u:object_r:system_file:s0"`. The binary's
   mode was also verified in the flown image: 0755 (executable), same in
   the T731 base image. `avc: denied for /dev/mem`: 6 — all from
   comm="HwBinder:2721_3" (graphics composer HAL), not the probe;
   permissive=1 (allowed). Label and mode are ruled out as the silence
   explanation — a refusal would log a refusal line, and there is none.
7. Processing lines: 0 on-boot action processing lines citing t728probe.rc,
   0 t728probe.rc citations. Boot_completed action processing lines: 3
   (L22683, 22819, 22820) — all stock (hw/init.rc:1224, dmesgd.rc:4,
   flags_health_check.rc:7). The t728probe.rc property action (line 12)
   has no processing line, but its write fired at guest 653.26 s, proving
   it executed. (T730's open anomaly stands: init logs no processing line
   for either t728probe action. Execution is proven only by the action's
   own write side-effect, never by init's logs.)

## Discrimination answer

The fault is NOT in the service definition / start path. T732's exec
bypasses the service mechanism entirely — init's exec command ran and
returned (proved by the write after it firing) — yet the binary produced
no marker under exec either. And the property leg's `start` failed
silently again, this time with the trigger fired, so leg 2 was fully
tested. Both mechanisms fail identically. The fault is in the binary's
execution itself: a verified-good static binary (0755, system_file, SHA
19db5d31, ELF64 aarch64, statically linked, no INTERP/DYNAMIC, entry
0x400310) that init cannot observably execute under either `start` or
`exec`. This reframes T730/T731's SERVICE-START-SILENTLY-FAILED: the
start path was never the fault location — it was a symptom. The next
discrimination must determine whether init even attempts the execve (a
minimal test binary that writes a marker from _start with no libc, or
init-side tracing of the exec dispatch).

## Holds

- T720_PROBE DONE status=PASS: 1 (L224). Met.
- T721 fence-log lines: 1 (L24914). Met.
- simplefb: mapped lines: 1 (L24913). Met. (T731 had 0 — now met.)
- Kernel panic/Oops/BUG: 0. Met. EPERM: 0. Met. T628 REJECT allocate2: 0.
  Met. Watchdog KILLING: 0. Met.
- FATAL EXCEPTION: 6 (L26629, 26635, 26841, 26854, 26958, 26964). NOT MET
  (T731 had 0). All 6 are DeadSystemException ("The system died") in
  launcher3, documentsui, permissioncontroller, settings, systemui —
  consequences of system_server (pid 2920) being killed by signal 9 at
  ~guest 725 s (Zygote: "failed to write to system_server FD: Connection
  refused"; lowmemorykiller closing its lmkd connection just before).
  This is a LATE-flight event, after both probe legs (exec at 122 s,
  property start at 653 s) — it does not explain the marker absence, but
  it is a real boot-instability signal: the guest's system_server died
  before the flight ended.
- CALL allocate2: 5 (T731: 2). avc: denied total: 2281 (T731: 1232).
  avc denied (sensorservice): 6 (T731: 9). avc denied for /dev/mem: 6
  (graphics composer HAL, permissive). The guest ran at full pace for
  the full 850 s, so the higher counts reflect a longer, faster boot,
  not a new failure mode.

## Wall

WALL-TYPE-TAG: guest wall. BINARY-EXEC-SILENTLY-FAILED (guest, NEW in
T732) — the verified-good payload binary produces no observable execution
under either init mechanism (`exec` returns without a marker or an init
log line; `start` produces no service start). This supersedes T730/T731's
SERVICE-START-SILENTLY-FAILED as the fault location: the start path was
a symptom, not the fault. The trigger leg is FIXED in T732
(boot_completed fired at guest 646.03 s; the widened 850/1100 window +
1.00 pace closed the T729/T731 secondary wall
BOOT-COMPLETED-NEVER-FIRED).

Rust par bhi lagu: haan — the Rust emulator must reproduce this exact
guest-side silence (init executes the action's write but the `exec` of a
verified-good static binary produces no marker, no argv dump, no init
exec line, and the `start` of the same binary produces no service)
before it can claim the exec/start path is understood; a Rust init that
merely runs the binary would pass a check the real guest fails.

## Reflection

- What happened: the discrimination worked — the exec returned (write
  fired after it) but the binary produced no marker, and the property
  leg's start failed silently again with the trigger now fired. Both
  mechanisms fail identically. Verdict FAIL on the binary-execution path.
- The trigger leg is fixed: boot_completed fired at guest 646.03 s
  (first time since T730), the guest ran at ~1.00 pace and reached the
  full 850 s target (851.001 s). The widened window + first-poll pace
  check worked.
- Root cause of the two surprises: (a) init logs no processing/exec
  line for t728probe actions — the exec's return is proven only by the
  write after it, and the start's failure is proven only by the absence
  of a service line + marker; (b) system_server died late in the flight
  (signal 9, ~guest 725 s), killing the apps with DeadSystemException —
  a separate boot-instability signal, not the cause of the marker
  absence.
- Missed signal: none on the pace (the first poll showed ratio 0.91 and
  the guest caught up to 1.01). The kernel-silence window (guest
  258–646 s) looked like a stall at poll 1 but was a quiet period — the
  kernel resumed logging and the trigger fired. A quiet kernel is not a
  stalled guest; only the guest clock + UART growth together tell the
  story.
- What changes: the next track must discriminate whether init even
  attempts the execve of this binary — the candidates are a minimal test
  binary (writes a marker from _start with no libc) or init-side
  tracing. The binary's bytes, mode, and label are all verified good, so
  the fault is in the exec dispatch or the runtime environment, not the
  artifact. No lane launches during any user-ordered pause.
