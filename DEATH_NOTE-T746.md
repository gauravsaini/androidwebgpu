# DEATH_NOTE-T746

Track T746 is FAIL. The killer of system_server pid 731 (SIGKILL,
flight-22 L16111–16112) is STILL UNIDENTIFIED after a fourth full
flight.

## What flight-24 did

- Flew the full 1500 s (guest 1501.937 s, host 1503.0 s, QEMU exit 0)
  on the identical stack. All holds met; dup-fail 0.
- The live zygote-sender tap (new in T746) captured the 4th
  zygote-signal-9 event (zygote 2009 @ guest 1090.410 s, L61549) in
  real time with full context — and proved the absence: no sender
  line exists in any stream (UART logcat, UART kernel lines, kmsg
  tap, sender tap, `Sending signal` inventory).
- Named 2143's killer (T744 trigger, 3rd recurrence: RenderThread
  0x8893 → connectionToken NPE → binder SIGSEGV at 0xd1008403) and
  found a SECOND unidentified SIGKILL instance (670, L17414) with
  the full exclusion table re-verified for it.
- Extended the exclusion table: self-kill and group-kill hypotheses
  refuted in all FOUR zygote-signal-9 instances; the sender is
  unnamed in 4/4.

## What flight-24 did NOT do

- It did not name 731's killer. No line in flight-24 names pid 731
  (count 0). The sender (2009's) is unnamed, so no link from the
  sender to 731 exists.
- It did not explain WHY the zygote event recurs at guest ~1090 s
  (1090.914 s in f22, 1090.410 s in f24) or what sends the signal.
  The log streams are exhausted as a source for the sender.

## The wall

SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED stays OPEN. It now
covers TWO unidentified instances (731 in flight-22, 670 in
flight-24). Type: UNIDENTIFIED-KILLER / EVIDENCE GAP — the killer
leaves no line in any captured stream.

## Reflection

- The sender hunt has exhausted the log streams. Four flights, four
  events, zero sender lines. The next track should NOT re-read the
  same streams for a sender — it should hunt WHAT happens at guest
  ~1090 s (a boot-phase timer? a scheduled job? a service restart
  cycle?) and WHAT kills system_server by SIGKILL with no line (a
  third instance would confirm a class, not an instance).
- The 670 death is the more important new fact than the 2009 event:
  a second 731-class SIGKILL with a fully verified exclusion table
  means the killer is a recurring mechanism, not a one-off. The
  question widens from "who killed 731" to "what kills system_server
  by SIGKILL with no line".
- The GateKeeper HAL stub stays UNBUILT (brief forbids it): the
  GateKeeper mode is a known, named, recurring death (667 in f23)
  and is NOT 731's or 670's killer — 670 has no self-kill line and
  no NPE crash. Building the stub would conflate a known death with
  the unknown one.
- The T744 trigger is now a 3-time recurrence with the NPE link
  present in 2/3 instances. It is a settled, named death mode — not
  the 731 answer, but no longer an open question either.

## Successor direction (for the driver, not this lane)

Do NOT launch a successor that re-mines flights 22–24 for the sender.
The next evidence must come from a NEW source: a guest-side change
that logs the sender (e.g. a kmsg-level kill audit, or init logging
the sender of the signal it reaps), or a flight instrumented to catch
WHAT happens at guest ~1090 s. Until such a source exists, further
flights will re-confirm the same absence.
