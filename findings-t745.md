# T745 findings — the 731 SIGKILL killer (service-flight-23)

Verdict: FAIL

Track T745. Successor to T744 (driver-certified PASS ~16:30 AEDT 21 Nov 2026).
Question: name the killer of system_server pid 731 (SIGKILL, guest ~1145.27 s,
flight-22 L16111–16112) — the T742 wall class in a new instance.

## First failing link

The killer of pid 731 is still UNIDENTIFIED after a full flight with the
exclusion table extended. Flight-23 produced TWO named system_server deaths —
but neither is 731's death, and neither names 731's killer:

- 667 died by SIGKILL; its killer is NAMED (GateKeeper-NPE self-kill, the
  T743 mechanism, 4th recurrence). The brief scopes this OUT: "the T743
  closure covered the 1960/1970 GateKeeper mode only, not this one" — and
  the GateKeeper mechanism was already RULED OUT for 731 (NPE 0, crash file
  0 in flight-22).
- 2118 died by SIGSEGV (signal 11); its killer is NAMED (binder-thread
  fault at 0xd1008403, the T744 trigger, CONFIRMED recurring at the
  identical fault address). 731 had no binder SIGSEGV (0 in flight-22).

Flight-23 produced NO new 731-class instance (SIGKILL with killer
unidentified): its only SIGKILL (667) has a named killer. The 731 killer —
a SIGKILL with no crash file, no NPE, no SIGSEGV, no watchdog line, no OOM,
no init line, no AM line — remains unidentified. Wall
SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED stays OPEN.

## Flight-23 summary (service-flight-23, identical stack)

- Launched 17:13:23 AEDT 21 Nov 2026 (supervisor pid 32625, QEMU pid 32628).
  Disk gate OPEN at launch (40,461,180 KiB free, 81%).
- Ended 17:38:26 AEDT. Guest 1500.936 s of the 1500 s target; host 1503.21 s;
  QEMU exit 0. Stop reason: "Guest target reached (1500.9s >= 1500s)".
- UART 113,348 lines / 15,519,744 B, SHA 868ff855.
- Boot completed: first sys.boot_completed firing guest 464.019 s (fastest
  in the chain: flight-18 521.327 s, flight-21 882.325 s, flight-22
  1044.345 s). sys.boot_completed 14 lines, in three groups — ALL THREE
  system_server instances booted (667, 2118, 4778).
- Dup regression HOLDS: RenderEngine dup-fail 0 in 1036.9 s presenting,
  rate 0.000/1000 s, below both T722 baselines (4.058 / 6.660).
- All holds met (counts in the Holds section below). T723_PROBE: 27 lines.
- System_server died twice (667 SIGKILL, 2118 SIGSEGV); a third instance
  (4778) survived to flight end.

## Death 1 — 667, SIGKILL, killer NAMED (GateKeeper self-kill, 4th recurrence)

Timeline (wall times from logcat lines; guest times from kernel-line
bracketing where stated):

- Crash file L16212: /data/system/dropbox/system_server_crash@1699658714930.txt,
  DropBox ts 1699658714930 (wall 23:25:14.930), guest ~502.3 s (bracketed).
  Contents: `Process: system_server`; `Crash-Handler:
  com.android.internal.os.RuntimeInit$KillApplicationHandler`.
- NPE L16219–16220: `java.lang.NullPointerException: Attempt to invoke
  interface method 'void android.service.gatekeeper.IGateKeeperService.
  reportDeviceSetupComplete()' on a null object reference`, at
  `LockSettingsService$DeviceProvisionedObserver.reportDeviceSetupComplete
  (LockSettingsService.java:3394)` — byte-identical call chain to T743
  (flight-20 pid 1960, flight-21 pid 1970, flight-18 pid 678). Main thread
  (`SystemServer.run` -> `Looper.loop`).
- Death reported L17437 (wall 23:25:17.766, guest 518.219): `Process 667
  exited due to signal 9 (Killed)`. Exit-zygote L17440 (same wall second):
  `Exit zygote because system server (pid 667) has terminated`.
- File-to-death-report interval ~2.8 s wall — the handler writes the report,
  then kills. This is the T743-proven evidence pattern, 4th recurrence.
- SELF-KILL LINE (the strongest proof, caught on the re-check): L17305
  (wall 23:25:15.117): `I/Process (  667): Sending signal. PID: 667 SIG: 9`
  — 667 ITSELF logged sending SIGKILL to itself, ~0.2 s after the crash
  file's wall ts and ~2.6 s before the death report. The logger is 667, so
  the sender is 667: the KillApplicationHandler's `Process.killProcess(
  myPid())`, logged by `Process.sendSignal`. The GateKeeper self-kill is
  doubly proven — crash file (handler named) + self-kill line (sender
  named). No other process sent 667 a signal.
- The 6 FATAL EXCEPTION lines (L17324, L17475, L17482, L17486 around 667's
  death) are DeadSystemException consequences in OTHER processes
  (RoleControllerService, main) — not causes.
- Last main-thread 667 line before the death: L17147 (wall 23:25:11.561,
  `Slow dispatch took 504ms main`, SettingsProvider) — the main thread was
  still dispatching ~3.4 s before the crash file. 667 kept logging on other
  threads (media.c2 retries, WifiService, ActivityManager, DropBoxManager)
  until L17305, the self-kill line. Last line naming 667 before the death
  report: L17305 — 135 lines before the report, not 884 (see Reflection:
  the analyzer's first last-line pattern missed the padded logcat pid
  format "(  667)" and reported L16556, a kernel binder line).
- The lmkd data-connection drop (L17320–17321, wall 23:25:17.573) is a
  death SYMPTOM (T742 lesson), 0.2 s before the death report — not LMK
  involvement. "Zygote failed to write to system_server FD: Connection
  refused" (L17436, wall 23:25:17.765) — Zygote discovering 667 is gone.

## Death 2 — 2118, SIGSEGV, killer NAMED (T744 trigger CONFIRMED recurring)

Timeline:

- RenderThread 0x8893 death L56133 (wall 23:31:45.489): launcher3 pid 4190,
  tid 4272 (RenderThread), fault addr 0x8893 — the libhwui
  `GrResourceProvider::createPatternedIndexBuffer` cluster (13 fault-addr
  lines in flight-23, 13 pids: settings + launcher3).
- Binder-thread fault L57125 (wall 23:32:08.955, guest ~953.852): `Fatal
  signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0xd1008403 in tid
  3593 (binder:2118_14), pid 2118 (system_server)` — the IDENTICAL fault
  address as flight-21's 3235. ~23 s after the launcher death.
- Death L58607–58608 (wall 23:32:44.998, guest ~971.463): `Process 2118
  exited due to signal 11 (Segmentation fault)`; `Exit zygote because
  system server (pid 2118) has terminated`. Fault-to-death 36 s (flight-21:
  53 s) — the dying window, spent on failed dumps.
- Dying window (reconstructed manually — the analyzer's dying-window
  section is 3235-hardcoded and skipped 2118): tombstoned Java intercept
  registered 23:32:15.023, terminated "due to timeout" 23:32:25.026;
  native intercept registered 23:32:26.695, terminated "due to input"
  23:32:28.715 (L58034 — NOT the last 2118 line; the true last 2118 line
  is L58582, `Done dumping`, wall 23:32:41.845, 26 lines before the death
  report: 2118 was alive and dumping stacks until ~3 s before its death);
  crash_dump64 started a dump of process 2118 (target tid 3593) at
  23:32:16.306 (L57501) and produced nothing; F/DEBUG lines naming 2118:
  0. The capture gap from T744 (debuggerd produces nothing for a
  system_server native crash) recurs.
- NO "Sending signal. PID: 2118" line — nobody sent 2118 a SIGKILL via
  the Android Process API; it died by SIGSEGV, not by a sent signal.
- The 2 FATAL EXCEPTION lines around 2118's death (L58630, L58653, wall
  23:32:46) are DeadSystemException consequences — not causes.
- NO system_server_crash file for 2118 (only 2 crash-file lines in the
  flight, both the 667 file). NO Watchdog KILLING. NO lmkd kill-decision.

## Trigger-file parser verdict

The T744 trigger (binder-thread SIGSEGV at fault addr 0xd1008403,
following a RenderThread 0x8893 death) is CONFIRMED recurring in flight-23
(2118, same fault address, same 23 s launcher-death-to-fault spacing, same
36 s fault-to-death window, same failed-dump capture gap). BUT the
connectionToken NPE link is REVISED OUT: "connectionToken must not be
null" count 0 in flight-23, "Uncaught exception from death notification"
0, "removeSpyWindowGestureMonitor" 0 — yet the binder SIGSEGV still
followed the launcher death. The NPE is NOT a necessary link in the T744
chain. The revised chain: RenderThread 0x8893 death (foreground app) ->
binder death notification -> input-channel teardown race (disposed-without-
removal 22 lines — the fragility recurred) -> binder-thread UAF at
0xd1008403, with or without the NPE firing. The fault address is the
stable signature, not the NPE.

## Death 3 (non-death) — 4778 survived

- Created L60888 (wall 23:33:52.455) by zygote 4634. Booted
  (sys.boot_completed lines 69983–70427). Alive at flight end (guest
  1500.936). Its zygote (4634) did NOT receive a signal 9 — the third
  instance ran without the zygote event.

## The 731 killer: still unidentified (why FAIL despite two named deaths)

Neither named death is 731's death, and neither mechanism can be 731's:

- 667's GateKeeper mode was already named in T743 and explicitly excluded
  for 731 (NPE 0, crash file 0, handler 0 in flight-22). Naming it a 4th
  time does not answer 731.
- 2118's binder SIGSEGV was already named in T744, and 731 had no SIGSEGV
  (Fatal signal 11 for a binder thread: 0 in flight-22; 731 died by
  signal 9).
- Flight-23 produced no new 731-class instance: its only SIGKILL (667)
  has a named killer, and its SIGSEGV (2118) is a different death mode.

## Extended exclusion table (flight-22 + flight-23)

| Candidate | Evidence | Verdict |
|---|---|---|
| GateKeeper-NPE self-kill (T743 mechanism) | Flight-22: NPE 0, crash 0, handler 0 — RULED OUT for 731. Flight-23: RECURRED as 667's killer (4th instance) — the mode is common, not the 731 answer | RULED OUT for 731 |
| Watchdog | Flight-23: WATCHDOG KILLING 0; system_server_watchdog DropBox file 0 (no watchdog file at all in flight-23); pre_watchdog 12 lines (2 distinct files: ...@1699658672814, ...@1699659373915 — noticing, not killing); Blocked in handler 6 lines, half_watchdog 6 (distress noticed, never acted on) | RULED OUT |
| OOM / LMK | Flight-23: Killed process 0; Out of memory 0; OomAdjuster Not killing 48; meminfo lines 0; lmkd drops at 23:25:17.573 and 23:32:44.943 are death symptoms | RULED OUT |
| init | Flight-23: Sending signal 9 to service system_server 0; stop system_server / stop zygote 0 | RULED OUT |
| ActivityManager | Flight-23: Killing ... system_server 0; Killing 667 0; Killing 2118 0; Force finishing 0; am_kill victims 19 app/bluetooth pids only; Killing lines 20 (18 clean AM lines, all BY system_server of apps; 2 inside binary dump blobs, victims still apps) | RULED OUT |
| Process.sendSignal kill (Android API) | NEW CHECK (T745): `Sending signal. PID: N SIG: 9` is the log line `Process.sendSignal` writes whenever ANY process kills via the Android API — the logger is the sender. Flight-22: NO line names 731 as the target from ANY logger (3 lines total, all self-kills: 2099 at 23:30:37.005, 1.2 s before 731's death; 2281 and 2223 at 23:30:39.407/437, 1.2–1.4 s after — 2223/2281 are zygote 537's forked children, killing themselves after 731 died: consequences, not the killer). Flight-23: 667 HAS the line (L17305, self-kill proof); 2118 has none. So nobody used the Android API to SIGKILL 731 — the killer, if it used a kill call, used a raw syscall, init, or another path no log names | RULED OUT (API path) |
| Flood loss hiding the kill line | Flight-23 death windows carry 99–107 Zygote/init/lmkd/watchdog/composer lines within ±200 lines of each death; a kill line naming the pid would be visible | RULED OUT |
| Zygote-signal-9 event (the prime suspect) | DEMOTED, not ruled out — see the section below | DEMOTED |
| T744 NPE link | connectionToken NPE 0, death-notification exception 0, removeSpyWindowGestureMonitor 0 in flight-23, yet the binder SIGSEGV recurred — the NPE is not a necessary link | REVISED (see Trigger-file parser verdict) |

## The zygote-signal-9 event: DEMOTED from prime suspect

The event recurred twice in flight-23, sender unnamed in both:

- L16605 (guest 504.272): `init: Service 'zygote' (pid 505) received
  signal 9`; L16606 (504.272): `Sending signal 9 to service 'zygote'
  (pid 505) process group...`; L16607 (504.401): libprocessgroup
  `Successfully killed process cgroup uid 0 pid 505 in 128ms`.
- L57062 (guest 952.368): `init: Service 'zygote' (pid 2001) received
  signal 9`; L57063 (952.380): group kill; L57065 (952.759): cgroup
  killed in 378ms.
- Flight-22: L15094 (guest 1090.914): the same event for 537, sender
  unnamed. Three instances total (537, 505, 2001).

Why demoted: in BOTH flight-23 cases the system_server death that
followed had a known, different, PROVEN killer (GateKeeper handler for
667; binder SIGSEGV for 2118). The event therefore does not kill
system_server in the observed cases — it is a parallel event that
accompanies system_server deaths, not their cause. Its precedence of
731's death (54 s in flight-22) is not evidence of causation.

Two further observations, stated as observations, not conclusions:

- The zygote SURVIVES the event and keeps working: 505 forked child
  1949 afterwards (L17244, wall 23:25:13.573, bracketed guest 515.342 —
  AFTER the signal-9 at guest 504.272); 537 forked 2223 and 2281
  afterwards in flight-22. A SIGKILL delivered to the zygote process
  itself would end it — so the signal was not delivered to the zygote
  process as such, or the "received signal 9" line does not mean what
  its wording suggests. The exact mechanics are unexplained.
- Timing across the interleaved streams cannot order the event against
  the death precisely (kernel guest-time lines vs logcat wall-time lines
  interleave with flush delay; wall↔guest clocks diverge locally under
  QEMU TCG — the T745 mining record). For 667 the crash file (guest
  ~502.3, bracketed) precedes the event (guest 504.272), and the
  self-kill line (L17305, wall 23:25:15.117, ~0.2 s after the crash
  file) also falls between the file and the death report — but the
  event's position relative to the self-kill cannot be pinned across
  the two clock streams. For 2118 the event (guest 952.368) precedes
  the SIGSEGV (stream guest ~953.852, actual time uncertain by
  seconds). The event is neither consistently a cause nor
  consistently a consequence. Its SENDER is the remaining
  unknown: what sends SIGKILL to the zygote?

Leading hypothesis (NOT proven — no line states it): when system_server
dies, Zygote logs "Exit zygote because system server (pid N) has
terminated" and kills ITSELF with SIGKILL; init reaps it ("received
signal 9") and group-kills the cgroup. This fits the 667 case (file,
then event, then init starting the new zygote 2001 at guest 507.097,
L16826 `init: ... started service 'zygote' has pid 2001`). It is an
inference from timing, not a logged fact — the T743 precedent (NPE +
handler lines) is not met for it, so it is recorded as a hypothesis,
not a naming. It also would not name 731's killer even if true: it
explains the zygote's death, not system_server's.

## Holds (all met)

- T720_PROBE DONE status=PASS: 1 (hold >=1).
- T721 fence log: 1 (hold >=1). Invalid present fence 0. REJECT 0. EPERM 0.
- simplefb: mapped: 1 (hold >=1).
- sys.boot_completed: 14 lines; first firing guest 464.019 s (hold >=1).
- Kernel panic / Oops / BUG: 0.
- T734_KERNEL_OK: 1 (early, guest <10 s). T733_PROBE_OK: 4; T728_PROBE_OK:
  3 clean kernel lines (+1 binary false positive inside a gzipped DropBox
  blob at L13863 — the analyzer counts 4; the true count is 3). One pair
  per system_server instance (667, 2118, 4778) plus the initial leg-2 T733
  repeat — the T741 pattern extended to two restarts. Control holds.
- T723_PROBE: 27 lines (start 2, DONE 1, SP1 DONE 1, SP2 DONE 1, SP3 DONE 1).
- [T737] hooks: mount /sysimg 1, mount /vendor 1, mount debugfs 1,
  mount proc 1, unmount proc 1 — all rc=0, all FAILED 0.
- [T739] hooks: unmount /sysimg 1, unmount /vendor 1, unmount debugfs 1,
  unmount proc (verify) 1 — all rc=0; handoff state verified (L584–L585);
  HANDOFF ABORTED 0; handoff line L589.
- [T740] hooks: t493-probe.sh killed 1, t501-keystore2-dump.sh killed 1.
- Post-handoff spam: can't create /dev/null 0, inaccessible or not found 0
  (totals in UART: 0 and 0).
- Watchdog KILLING: 0 (recorded, not a hold — a watchdog kill would be a
  named mechanism).

## Other failure modes in flight-23 (recorded, not the question)

- RenderThread 0x8893 cluster: 13 fault-addr lines, 13 pids (settings
  2575, 2773, 5272, 5509; launcher3 2609, 3286, 4190, 5309, 5933, 6116,
  6310, 6471, 7163) — the libhwui defect fired 13× (flight-21: 5×,
  flight-22: 0×). Non-deterministic across flights.
- Bluetooth SIGSEGV cluster: 6 (pids 1464, 1840, 2532, 2884, 5220, 5578),
  distinct fault addrs — independent defect, unchanged.
- Tombstone lines (case-sensitive): 1661. F/DEBUG lines: 748 (app crashes
  have blocks; 2118 has none — the capture gap).
- system_server_pre_watchdog: 12 lines (2 distinct files:
  ...@1699658672814, ...@1699659373915 — the Watchdog noticing slowdowns,
  never killing).

## WALL-TYPE-TAG

- WALL: SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED — OPEN (first
  failing link; the 731 killer is still unidentified after a full flight
  with the exclusion table extended).
- WALL (T744): SYSTEM_SERVER-SIGSEGV-BINDER-THREAD — trigger CONFIRMED
  recurring (2118, fault 0xd1008403, same address as 3235); the trigger
  question is answered; the NPE link is revised out (see Trigger-file
  parser verdict).
- WALL (T743): GateKeeper-NPE self-kill — 4th recurrence observed (667);
  the mode is common, not the 731 answer.
- NEW OPEN QUESTION (not a wall — no death is attributed to it): the
  zygote-signal-9 sender — what sends SIGKILL to the zygote (3 instances:
  537, 505, 2001), and why does the zygote survive it and keep forking?

## Rust par bhi lagu

Rust par bhi lagu: the method that produced this verdict — a live
kill-verb grep running during the flight, a live death-window tap
(Zygote / init / lmkd lines), a live Watchdog tap, a live zygote tap,
and a byte-read analyzer that reproduces the previous flight's counts
exactly before adjudicating the new one — applies unchanged to the Rust
emulator work. When the Rust guest runs, its UART gets the same
treatment: the kill-verb grep first, the death window second, the
exclusion table extended with counts, and no killer named without a
line that names it. The two capture gaps found here (debuggerd produces
nothing for a system_server native crash; the zygote-signal-9 sender is
never logged) are guest-side facts the Rust emulator must also handle:
a Rust guest that cannot capture a system_server native crash stack, or
cannot log who sent a signal, will hit the same unidentified-killer
wall.

## Reflection

Two first-draft errors were caught by the analyzer regression before the
flight, not after: (1) the mining record's "last 731 line" was L15564 —
the analyzer's last-line search caught L15668 (crash_dump64 dump,
23:30:18.987), correcting the silence from ~34–46 s to ~19 s and
surfacing the five Watchdog crash-dump requests; (2) the "ZygoteInit
START" count was 34 (bare pattern) — the true form is the ">>>>>> START
... ZygoteInit" line, exactly 2. The lesson, again: counts come from the
regression, never from memory, and a mining record is not trusted until
the analyzer reproduces it. A THIRD error was caught on the re-check
after the draft: the analyzer's last-line pattern used the bare
`f"({pid})"` form and MISSED logcat's padded pid format "(  667)" /
"( 2118)" — it reported L16556 for 667 and L58034 for 2118, but the true
last lines are L17305 (the self-kill line) and L58582 (Done dumping).
The pattern was fixed (regex `\(\s*pid\)`), both analyses re-run, and
the flight-22 regression still reproduces the T744 counts exactly
(last 731 line still L15668 — verified by direct grep). The fix also
surfaced the most important single line of the flight: L17305, 667's
self-kill line, which doubly proves the GateKeeper self-kill — and it
added a new exclusion-table row for 731 (the "Sending signal" check:
no process used the Android API to SIGKILL 731). The lesson: a pattern
that misses a log format misses the evidence that format carries, and
the fix is worth more than the count it corrects. The second lesson is
about the verdict itself: this flight named TWO killers and still FAILS,
because neither is the pid in the question. Naming a death is not the
same as answering the question — the brief's scoping ("the T743 closure
covered the GateKeeper mode only, not this one") exists precisely to
prevent a false PASS on a recurring known mechanism. The guard that
worked here was re-reading the brief's verdict contract verbatim before
deciding, not the summary of it.

## What the evidence determines for the successor (T746)

- The 731 killer is still unknown; the track continues. The zygote
  event is demoted — do not re-hunt it as the prime suspect. The
  remaining unknown is its sender, which no log names.
- The T744 trigger is confirmed recurring and its chain is revised
  (NPE not necessary). No image change is implied by this track.
- Do NOT build the GateKeeper HAL stub on this evidence: the GateKeeper
  mode is a known, recurring, already-named death — building the stub
  would conflate it with the still-unknown 731 killer, exactly what the
  brief forbids.
