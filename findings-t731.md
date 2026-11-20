# T731 findings — start-leg discrimination (on-boot direct start)

Verdict: FAIL

## Flight

- service-flight, 2026-11-19 20:58:00–21:11:22 AEDT (09:58:00–10:11:22Z).
- Host 801.72 s, guest 641.31 s, QEMU exit 0 (SIGTERM from supervisor on the
  host cap). Stop reason: host timeout (801.5 s >= 800 s).
- Guest target 700 s NOT reached: guest/host ratio 0.80 (T730: 705.059/706.04,
  ratio 1.00). The guest ran ~20% slower than T730's flight.
- UART: 15,258 lines, 1,611,633 B, SHA256
  8b2d922ca9ac081c0d67332e71554b9db6c38dfa9086c800d52bdf4d2091d463.
- Flown image: t731-system.img SHA256
  15c79aad1348c72f76f4ad831bc51d637c8bbbc453c09a574d924c22dbe49f22
  (T729 image + new rc only; binary untouched, 19db5d31).
- New rc: t728probe.rc SHA256
  053189d04b24a12834584eece9bf64cbda41d6b9b39af1d70be19723c2a9029c
  (711 B; old rc + `start t728probe` as line 18 in the `on boot` action;
  property action unchanged at line 12).
- Parser citation: parse_init_rc.py on the exact new rc bytes — VERDICT
  ACCEPT, 0 errors, 1 service, 2 actions, 0 imports. (Positive control: the
  old rc bytes f61b9c0f reproduce T730's citation exactly.)

## Adjudication (brief's 7-item order)

1. On-boot start: FAILED SILENTLY. 0 `t728probe` lines in the UART, 0
   `init starting service 't728probe'` lines, 0 service EXIT lines, 0
   service exec/start failures, 0 init service-refusal lines. The `start
   t728probe` command in the `on boot` action left no trace at all.
2. Trigger: NEVER FIRED. `sys.boot_completed` 0 lines. The only
   `boot_completed` line is L2418: `[T723] boot_completed never detected in
   600s; probe not run` — the T723 hook's 600 s guest timer expired. Wall
   BOOT-COMPLETED-NEVER-FIRED (guest, T729 recurring) is back: the boot ran
   ~61% slower than T730's pace (first PROBE_OK guest 168.989 s vs
   104.700 s in T730), so at T730's pacing boot_completed would have landed
   near guest ~800 s — past the 600 s hook window and the 700 s target.
3. Registration legs: leg 1 PROVEN, leg 2 UNTESTED. `T728_PROBE_OK` x1:
   L10703, guest 168.989 s — the on-boot action's write command (same leg
   as T730's 104.700 s). Leg 2 (property action's write, T730: 500.955 s)
   never had a chance: the trigger never fired. Crucially, leg 1 proves
   init DID parse the new rc and DID execute the on-boot action — the write
   command in the same action ran. The `start` command in that same action
   did not.
4. argv: NONE. `T728_ARGV` 0 lines, `T728_ARGC` 0 lines. The binary never
   ran, so no argv handoff exists to quote. There is no argv evidence.
5. `T728_SERVICE_OK`: 0 lines. The payload never ran. This is the verdict
   condition: PASS iff `T728_SERVICE_OK` appears. It does not.
6. SELinux refusal: 0 lines. 0 init service-refusal lines, 0
   label/domain-transition lines, 0 `avc: denied for /dev/mem`. The binary's
   label was read back from the flown image via box debugfs before the
   flight: `security.selinux (25) = "u:object_r:system_file:s0"`. Label
   UNTESTED as a cause in the strict sense, but ruled out as the silence
   explanation — same as T730: a label refusal logs a refusal line, and
   there is none.
7. Processing lines: 0 on-boot action processing lines citing t728probe.rc,
   0 boot_completed action processing lines, 0 t728probe.rc citations.
   (T730's open anomaly stands: init logs no processing line for either
   t728probe action. But leg 1 firing proves the on-boot action DID
   execute, so the missing processing lines are a logging gap, not proof
   the action was skipped.)

## Discrimination answer

The fault is in the service definition / start path itself — NOT in the
dispatch leg. T730 proved the property-action dispatch fails silently;
T731 proves the on-boot direct `start` fails identically silently, in an
action init demonstrably executed (its write command fired at guest
168.989 s). Both legs, same silence. The trigger failure (item 2) is a
separate, secondary wall: it left leg 2 untested and cut the flight short
of the display stack, but it does not explain the on-boot silence — the
on-boot action runs at boot regardless of boot_completed.

## Holds

- T720_PROBE DONE status=PASS: 1 (L223). Met.
- T721 fence-log lines: 0. NOT MET (requires >=1).
- simplefb: mapped lines: 0. NOT MET (requires >=1).
- Kernel panic/Oops/BUG: 0. Met. EPERM: 0. Met. T628 REJECT allocate2: 0.
  Met. Watchdog KILLING: 0. Met. FATAL EXCEPTION: 0. Met.
- CALL allocate2: 2 (T730: 15). avc: denied total: 1232 (T730: 2570).
  avc denied (sensorservice): 9. The two unmet holds are consequences of
  the boot never completing inside the window — the guest never reached
  the display stack — not independent failures.

## Wall

WALL-TYPE-TAG: guest wall. SERVICE-START-SILENTLY-FAILED (guest, T726
recurring) — now proven for BOTH start legs (property action T730,
on-boot direct start T731). Secondary: BOOT-COMPLETED-NEVER-FIRED
(guest, T729 recurring) — the flight ended on the host cap at guest
641.31 s with the boot ~61% slower than T730's pace.

Rust par bhi lagu: haan — the Rust emulator must reproduce this exact
guest-side silence (init executes the action's write but the `start`
command produces no service, no refusal, no exit line) before it can
claim the start path is understood; a Rust init that merely logs the
action would pass a check the real guest fails.

## Reflection

- What happened: the discrimination worked — the on-boot start failed
  silently in an action init demonstrably ran. The verdict is FAIL on the
  start path, with a secondary trigger failure caused by a slow boot.
- Root cause of the two surprises: (a) the guest ran 20% slower than
  T730 (guest/host 0.80 vs 1.00), so the 800 s host cap fired at guest
  641 s and the 700 s target was missed; (b) init logs no processing line
  for t728probe actions, so action execution can only be proven by the
  action's own write side-effect (leg 1), never by init's logs.
- Missed signal: the 21:03 poll already showed guest 195.6 s at host
  301 s (ratio 0.65) — the slowdown was visible 5 minutes in, but no
  rule exists for a guest/host pace alarm; the flight was left to run to
  the host cap.
- What changes: a future flight brief should add a pace check (guest
  seconds vs host seconds at the first poll; if the projected guest total
  at the host cap is below the target, the window is already lost) and
  should treat the action's own write as the execution proof, not init's
  processing log. Both live in this file and in the death note, not as a
  new lane — no lane launches during the user-ordered Mac pause.
