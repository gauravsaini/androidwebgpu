# Track T746 findings — the 731 killer hunt, round 2: the zygote-signal-9 sender

Verdict: FAIL

## Question

Name the killer of system_server pid 731 (SIGKILL, guest ~1145.27 s,
flight-22 L16111–16112). After a fourth flight, the killer is STILL
UNIDENTIFIED. Wall SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED stays
OPEN — and flight-24 adds a SECOND unidentified SIGKILL instance
(system_server pid 670, flight-24 L17414).

## Flight-24 summary

- service-flight-24: launched 2026-11-21T08:47:55Z (19:47:55 AEDT),
  ended 09:12:58Z (20:12:58 AEDT). Guest 1501.937 s of 1500 s; host
  1503.0 s; QEMU exit 0. UART 94,908 lines / 12,990,410 B, SHA
  8a554c38 (flight-end.json verified).
- Stack identical: kernel 70d3859d, QEMU 8e5c46dc, DTB 4ecab141, CPIO
  c1203336, system.img af40403a, vendor.img fe0bc359.
- Boot: first sys.boot_completed firing guest 502.087 s; 11
  sys.boot_completed lines. T723_PROBE 27 lines. Dup-fail 0 in 999.8 s
  presenting, rate 0.000/1000 s — below both T722 baselines (4.058 /
  6.660). All holds met (T720 PASS 1 / FAIL 0; T721 fence 1; Invalid
  present fence 0; REJECT 0; EPERM 0; simplefb: mapped 1; Kernel panic /
  Oops / BUG 0; [T737] hooks 9; [T739] hooks 7; [T740] hooks 3;
  post-handoff spam 0/0; Watchdog KILLING 0).

## The zygote-signal-9 event RECURRED — 4th instance, sender STILL UNNAMED

- Event: zygote pid 2009 @ guest 1090.410 s (L61549: `[ 1090.409872]
  init: Service 'zygote' (pid 2009) received signal 9`). Group-kill
  L61550, cgroup-kill L61562. The live zygote-sender tap (new in T746)
  captured it in real time: 89 lines, including the event, the
  group-kill, the cgroup-kill, and every `Sending signal` line in the
  flight.
- The sender is named NOWHERE: the only `Sending signal` line naming
  2009 is the post-reap group kill (L61550) — cleanup, not cause. No
  Android-API `Sending signal. PID: 2009 SIG: 9` line exists. No
  libprocessgroup line names a sender. No line in the UART, the kmsg
  tap, or the sender tap shows who sent signal 9 to 2009.
- The ~1090 s neighbourhood recurs: flight-22's event was at guest
  1090.914 s; flight-24's at 1090.410 s. Two events in the same
  guest-time neighbourhood, 0.5 s apart across flights. Recorded as a
  pattern, not a mechanism — nothing in either flight explains WHY
  ~1090 s.
- The paradox RECURRED (4th instance): 2009 forked 17 children after
  its event (4374 @ L61618, 4385, 4399, 4541, 4561, 4613, 4632, 4643,
  4693, 4732, 4803, 4824, 4877, 4889, 4913, 5004, 5082 @ L64994) and
  logged the Exit-zygote line L65764. Init ran the onrestart block
  (L62071–L62090, guest 1093.35–1093.57 s) and a new zygote 5170
  started (ZygoteInit START L65883; created 5312 at L67517). Init
  logged NO `started service 'zygote'` line in this flight (count 0)
  — the ZygoteInit START line (3 in f24: 506 L2072, 2009 L17606, 5170
  L65883) is the reliable restart marker, not the `started service`
  line.
- Self-kill hypothesis: REFUTED a 4th time on ordering. The event
  (1090.410 s) precedes 2143's death (1124.395 s) by 33.985 s. The
  only earlier system_server death is 670 at 554.501 s — 536 s before
  the event — and 2009 never served 670 (2009 started after 670 died;
  506 served 670). A zygote does not wait 536 s to self-kill for
  another zygote's system_server.
- Group-kill hypothesis: REFUTED a 4th time. 0 lines place
  system_server in zygote's group; 2143's death does not coincide
  with the event (34 s gap); 2009 survived its own group kill.

## The two deaths in flight-24

### Death 1 — 670, SIGKILL, killer UNIDENTIFIED (NEW 731-class instance)

- Death L17414 (guest 554.501 s): `Exit zygote because system server
  (pid 670) has terminated`, logged by 506. Last 670 line L17260
  (wall 23:25:43.862, routine Compat-change report); silence gap 154
  lines; 131 Zygote/init/lmkd/watchdog/composer lines within ±200 —
  a kill line naming 670 would be visible. None exists.
- Exclusion table for 670 (all counts from the flight-24 UART):
  GateKeeper-NPE self-kill RULED OUT (no `Sending signal. PID: 670
  SIG: 9` line; GateKeeper grep finds only boot-time LockSettings
  lines, L6813–L8857, incl. `Unable to acquire GateKeeperService`
  L7323 — a boot warning, not the NPE crash; no
  `system_server_crash` DropBox, no KillApplicationHandler line).
  Watchdog RULED OUT (WATCHDOG KILLING 0). OOM/LMK RULED OUT
  (analyzer: `Killed process` 0, OOM-killer 0; meminfo-pressure tap
  shows no pressure event at the death). init RULED OUT (`Sending
  signal 9 to service system_server` 0; 30 init signal-9 lines, none
  names system_server). ActivityManager RULED OUT (kill-verb grep:
  10 lines, victims bluetooth/systemui phone media, killed by
  670/2143/5312 — none names 670). Android-API sendSignal RULED OUT
  (the API self-kill lines in f24 name 1274, 1606, 1904, 1934 —
  children self-killing after 670 died — and 2525, just before 2143's
  death — none names 670). Flood loss
  RULED OUT (131 lines within ±200 of the death).
- 670 is the SECOND unidentified system_server SIGKILL in the chain
  (731 in flight-22, 670 in flight-24). The wall now covers two
  instances. The 731 question is unchanged: no line in flight-24
  names pid 731 at all (count 0 for `( 731)` / `pid 731`).

### Death 2 — 2143, SIGSEGV, killer NAMED (T744 trigger, 3rd recurrence)

- Fault L64161 (wall 23:34:15.711): `Fatal signal 11 (SIGSEGV),
  fault addr 0xd1008403 in tid 2178 (binder:2143_3), pid 2143
  (system_server)` — the IDENTICAL fault address (flight-21 3235,
  flight-23 2118, flight-24 2143). Death L65764 (guest 1124.395 s);
  dying window 13.0 s (fault g=1111.406 → death g=1124.395, 1603
  lines); 26 tombstoned lines, 3 intercepts for pid 2143 in the
  window. Last 2143 line L65118 (silence gap 646 lines).
- 5312 survived to flight end.

## Trigger-file parser verdict (the T744 trigger, revised chain)

The T744 trigger RECURRED in flight-24 — the third recurrence, and
the connectionToken NPE link is BACK:

- RenderThread 0x8893 deaths: 12 lines (settings + launcher3,
  e.g. L27310, L30121, L33044, L38583).
- connectionToken NPE: 2 lines — L64005 (`W/BinderNative( 2143):
  java.lang.NullPointerException: connectionToken must not be null`,
  wall 23:34:09.207) ~6.5 s before 2143's fault (23:34:15.711), in the
  SAME pid; and L89068 (pid 5312, wall 23:39:50 — a separate instance
  in the surviving system_server, after 2143's death).
- Binder SIGSEGV at 0xd1008403: 1 line (L64161, 2143). Disposed-channel
  lines: 13 in flight-24.
- REVISED CHAIN (stands, with the NPE as frequent-but-not-necessary):
  RenderThread 0x8893 death → binder death notification →
  input-channel teardown race (connectionToken NPE present in 2 of 3
  recurrences — flight-21, flight-24 — absent in 1 — flight-23) →
  binder-thread UAF at 0xd1008403. The fault address is the stable
  signature; the NPE is a frequent companion, not a necessary link.
  T745's revision (NPE revised OUT as necessary) stands; flight-24
  adds that the NPE DOES recur with the trigger in the majority of
  instances (2/3).

## Why this is FAIL, not PASS

PASS required the killer of 731 NAMED with a line naming it, or the
zygote-signal-9 sender named WITH evidence the sender IS 731's
killer. Neither happened:

- The sender (2009's) is unnamed — the live sender tap captured the
  event with full context and no sender line exists in any stream.
- No line in flight-24 names pid 731 (count 0). A mechanism proven in
  flight-24 cannot be linked to 731 without a line naming 731.
- Naming 2143's killer (the trigger, 3rd recurrence) and 670's death
  as a new unidentified instance extends the table but does not
  answer the question — the T745 lesson applies: naming a death is
  not the same as answering the question.
- The exclusion table is extended for 731 by flight-24's evidence in
  one respect only: the zygote-signal-9 sender is now unnamed in FOUR
  instances (537, 505, 2001, 2009), and the self-kill and group-kill
  hypotheses are refuted in all four. The sender hunt has exhausted
  the log streams — no `Sending signal` line, no kmsg line, no tap
  line in four flights names it.

## WALL-TYPE-TAG

Wall: SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED — type:
UNIDENTIFIED-KILLER (SIGKILL with no naming line in any stream after
four full flights). Sub-type: RECURRING-PATTERN (the zygote-signal-9
event recurred 4/4 flights with an unnamed sender; a second
unidentified SIGKILL instance, 670, appeared in flight-24). The wall
is NOT a crash, NOT a boot failure, NOT a regression — the flight ran
full 1500 s, QEMU exit 0, all holds met, dup-fail 0. The wall is an
EVIDENCE GAP: the killer leaves no line in any captured stream
(UART logcat, UART kernel lines, kmsg tap, live sender tap).

## Rust par bhi lagu

Rust par bhi lagu — the finding applies to the Rust emulator work in
the same way it applies here: a SIGKILL with no naming line is an
evidence gap in the GUEST's logs, not in the host harness. The Rust
emulator's UART capture must preserve the same streams (logcat,
kernel [t] lines, kmsg) with the same fidelity, because the killer —
if it ever logs — will log in one of them. No Rust-side change is
implied by this verdict: the wall is a guest-logging gap, and the
next evidence must come from the guest (a new flight with a
sender-capturing tap, or a guest-side change), not from the harness.

## Reflection

- The live zygote-sender tap worked as designed: it captured the 4th
  event in real time with full context, and its 89 lines prove the
  absence (no sender line) rather than merely failing to find one.
  Absence-of-evidence is now evidence-of-absence for the log streams.
- The analyzer generalization worked: the dying-window section
  reconstructed 2143's window (13.0 s, 26 tombstoned, 3 intercepts)
  where the T745 analyzer would have skipped it (3235-hardcoded).
  Two analyzer defects were found and fixed during regression (fault
  -line guest time, ±300-line bracket window too narrow for sparse
  kernel regions — now 2000). The regression reproduces the T744 and
  T745 counts exactly on both prior flights.
- The ~1090 s recurrence of the zygote event (1090.914 s in f22,
  1090.410 s in f24) is the strongest new lead: two events in the
  same guest-time neighbourhood across flights. It is a pattern
  without a mechanism — the next track should hunt WHAT happens at
  guest ~1090 s (boot-phase timing? a scheduled job? a service
  restart timer?) rather than re-reading the sender lines.
- The 670 death is the second 731-class instance. If a third appears,
  the question widens from "who killed 731" to "what kills
  system_server by SIGKILL with no line" — a class, not an instance.

## Deliverables

- findings-t746.md (this file)
- outcome-t746.txt (`FAIL`)
- TIMELINE-t746.md
- DEATH_NOTE-T746.md (required — FAIL)
- analysis-t746.txt (analyzer output on flight-24, 1135 lines)
- analysis-t746-regression-flight23.txt (1136 lines, reproduces T745)
- analysis-t746-regression-flight22.txt (903 lines, reproduces T744)
- t746-mining-record.md (Step 2 sender timeline, 253 lines)
- scripts/supervisor_t746.py (21,255 B) + scripts/analyze_t746.py
  (63,581 B)
- flights/service-flight-24/ (16 files: uart.log SHA 8a554c38,
  uart.log.gz, uart.log.numbered.txt, kmsg, meminfo-pressure,
  death-window, binder-death, kill-verb, watchdog, zygote,
  zygote-sender (NEW), flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log, supervisor-stdout.log)
- progress-t746.txt, heartbeat.json (CLOSED, FAIL)
