# T744 findings — the 3235 SIGSEGV trigger, named and flight-tested

Track T744, recovery lane 1. Flight: service-flight-22 (Mac worktree
`/Users/Shared/wt-track744-sigsegv-binder-thread`, branch `t744`, base `b78ac6e`).
Analyzer: `scripts/analyze_t744.py` (SHA `b7cd4389`), run on flight-22 UART
16:13 AEDT 21 Nov 2026, exit 0, 674 lines (`analysis-t744.txt`, SHA `d224e174`).

Verdict: PASS

## Flight-22 summary

- Launched 15:46:14 AEDT 21 Nov 2026 (supervisor pid 68738, QEMU pid 68806,
  QEMU 11.1.2 `8e5c46dc`). Ended 16:11:17 AEDT.
- Guest 1500.555 s of the 1500 s target; host 1501.98 s; QEMU exit code 0.
- Stop reason: "Guest target reached (1500.6s >= 1500s)".
- UART: 27,606 lines / 3,740,533 B, SHA `9653b78c`.
- Boot completed: first `sys.boot_completed` firing guest 1044.345 s (L13688,
  concatenated line — the firing text rides inside a binary DropBox-dump line,
  the T742 lesson); 22 `sys.boot_completed` lines in total, including a second
  batch (L25619, L25648) after the system_server restart.
- Presenting time (guest max minus first boot): 456.2 s.
- RenderEngine dup-fail: 0 in 456.2 s presenting — 0.000 per 1000 s, below both
  T722 baselines (4.058 / 6.660). The dup regression HOLDS.
- All 5 flight inputs verified by the supervisor at launch (kernel `70d3859d`,
  system.img `af40403a`, vendor `fe0bc359`, CPIO `c1203336`, DTB `4ecab141`).

## Verdict contract — adjudication

PASS prongs (all met):

| Prong | Evidence |
|---|---|
| 3235 SIGSEGV trigger NAMED with evidence | Mining of flight-21 (see § "The trigger"). Full chain, every link a flight-21 UART line. The brief states this prong was already met by the mining; flight-22 was confirmation + capture + holds. |
| Tombstone / equivalent capture for a system_server SIGSEGV exists, OR designed and proven unachievable without an image change, with the reason stated | Proven unachievable without an image change (see § "Tombstone capture audit"). Flight-22 corroborates: the native-backtrace intercept failed "due to input" twice more for system_server pid 731. |
| Full 1500 s flight runs (service-flight-22, QEMU exit 0) | 1500.555 s guest, 1501.98 s host, exit 0. |
| Boot completes | First firing guest 1044.345 s; 22 `sys.boot_completed` lines. |
| Dup regression holds | 0 fails in 456.2 s presenting; 0.000/1000 s < 4.058 and < 6.660. |
| All holds intact | See holds table below — every hold met. |
| Unmount hooks fire | [T739] 7 lines, all rc=0, handoff verified. |
| Post-handoff spam ~0 | 0 / 0 (`can't create /dev/null` and `inaccessible or not found` after L589). |

FAIL prongs (none fires): the flight ran; the trigger is identified (mining);
the dup rate did not regress; handoff state was restored ([T739] L584–585);
evidence is sufficient for the track question.

Two caveats, stated plainly (they do not overturn the verdict — see
§ "Reflection"):

1. Flight-22 did NOT confirm the trigger's one inferential link. The causal
   chain did not recur at all in flight-22 (see § "Flight-22 confirmation"),
   so the NPE-thread-vs-faulting-thread-4302 link remains inferential. The
   trigger stands named on flight-21 mining evidence alone.
2. Flight-22 produced a NEW unidentified death: system_server pid 731 died by
   SIGKILL at guest ~1145.27 s with the killer UNIDENTIFIED and the T743
   GateKeeper mechanism ruled out. It is recorded below as a NEW WALL for the
   successor track (precedent: T743 PASSed with the 3235 wall open; T741
   PASSed with pid 678 open).

## The trigger — NAMED by mining flight-21

Causal chain, each link a flight-21 UART line (mining record
`t744-mining-record.md`, SHA `b9cc7d97`; all 5 counts re-verified by this lane
on the flight-21 UART, SHA `c00a068c`: `connectionToken must not be null` 1,
`0xd1008403` 1, `0x8893` 5, `createPatternedIndexBuffer` 10,
`disposed without first being removed` 7):

1. launcher3 pid 4618 (foreground TOP) dies by RenderThread SIGSEGV —
   libhwui `GrResourceProvider::createPatternedIndexBuffer`, fault addr
   `0x8893` — 23:38:10.261 (L48391).
2. Death reported L48788, 23:38:23.991.
3. Binder death notification → uncaught NPE L48789–48793, 23:38:23.996 —
   `NullPointerException: connectionToken must not be null` at
   `InputManagerService.removeInputChannel:802` ←
   `removeSpyWindowGestureMonitor:739` ← `binderDied` — the ONLY such NPE in
   the flight (count 1); the spy-monitor path appears only here.
4. WIN DEATH L48804, channel disposed without removal L48805, 23:38:24.015 —
   "Input channel object 'bd24b2d …QuickstepLauncher (client)' was disposed
   without first being removed with the input manager!".
5. Binder thread 4302 (`binder:3235_F`) SIGSEGVs at `0xd1008403` L48852,
   23:38:26.098 — 2.1 s after the NPE; death L50040, 23:39:18.939 (53 s
   window; the process kept running — Looper, AM, OomAdjuster alive).

Mechanism: use-after-free in the input-channel teardown path. ONE inferential
link: the NPE's thread is unnamed (BinderNative logs pid only) vs the faulting
thread 4302 — 2.1 s proximity + unique NPE + shared channel object (`bd24b2d`)
make it strong but not conclusive. Flight-22 was flown to confirm it; it did
not recur (next section).

## Flight-22 confirmation — the chain did NOT recur

| Signal | Flight-21 count | Flight-22 count |
|---|---|---|
| `connectionToken must not be null` NPE | 1 | 0 |
| `0xd1008403` (3235 fault addr) | 1 | 0 |
| `0x8893` (RenderThread fault addr) | 5 | 0 |
| `createPatternedIndexBuffer` | 10 | 0 |
| `disposed without first being removed` | 7 | 0 |
| `Uncaught exception from death notification` | 1 | 0 |
| `removeSpyWindowGestureMonitor` | 1 | 0 |
| WIN DEATH (launcher3 Quickstep) | 1 | 0 |
| RenderThread Fatal signal 11 | 5 | 0 |
| Binder-thread Fatal signal 11 (3235) | 1 | 0 |
| Total Fatal signal 11 | 10 | 2 (both bluetooth) |
| Binder-death tap lines (beyond header) | 18 (flight-21 test) | 0 |

The supervisor's live binder-death tap for flight-22 is EMPTY (header only,
108 B): no death-notification NPE, no disposed channel, no WIN DEATH. The
upstream event never happened — no launcher RenderThread crash in flight-22 —
so the cascade never started. The launcher RenderThread crash is
NON-DETERMINISTIC across flights (5× in flight-21, 0× in flight-22), and the
3235 chain is a contingent cascade, not a deterministic one. Consequence: the
inferential link stays inferential; the trigger's naming rests on the flight-21
mining evidence, which is complete and line-cited. No recurrence was available
to confirm or refute the thread identity, and none is claimed.

## Flight-22 deaths — every death named

Three SIGKILL deaths and two SIGSEGV deaths. System_server died once and was
re-created; the flight continued and completed.

| Pid | What | Death | Killer / cause |
|---|---|---|---|
| 731 | system_server | SIGKILL, 23:30:38.232, guest ~1145.27 s (L16111–16112) | UNIDENTIFIED — see NEW WALL below |
| 1975 | app (uid 10079) | SIGKILL, 23:28:30.278 (L13919) | system_server 731 itself, via libprocessgroup ("Successfully killed process cgroup uid 10079 pid 1975", 23:28:29.481) — routine |
| 1885 | com.android.phone | SIGKILL, 23:30:09.815 (L15606) | system_server 731, "bg anr" (am_kill L15414) — routine |
| 1848 | droid.bluetooth | SIGSEGV 0x67750, 23:27:46.652 (L13023); exit signal 11 L13882 | bluetooth cluster (independent defect, recurring) |
| 3022 | droid.bluetooth | SIGSEGV 0x9b51e, 23:36:12.632 (L26989) | bluetooth cluster (independent defect, recurring) |

System_server instances: 731 created L3961, died 23:30:38; 2437 created L16930
(23:32:06) and ran to flight end. T733_PROBE_OK × 3 (L3138 guest 277.2 s,
L14095 guest 1072.3 s, L26009 guest 1474.2 s) and T728_PROBE_OK × 3 (L3133,
L14093, L26004) — two firings under 731 (leg-1 + leg-2 repeat, the T741
flight-18 pattern) and one under 2437 after the restart; within the brief's
allowance (== 2, or 3 with a leg-2 repeat explained by a system_server
restart).

The 4 `FATAL EXCEPTION` lines (L15786, L16115, L16118, L16124) are all
`DeadSystemException: The system died; earlier logs will point to the root
cause` in OTHER processes (android.ext.services 2223, pid 2281, systemui 1548,
pid 2099) — consequences of the 731 death, not causes. The earlier logs point
nowhere: no killer is named anywhere in the UART.

### NEW WALL — system_server 731 SIGKILL, killer UNIDENTIFIED

Death window (wall clock 11-10, guest ~1145 s):

- 23:30:36.739 — lmkd data connection dropped (meminfo tap + UART L~15783).
- 23:30:38.231 — `Zygote failed to write to system_server FD: Connection refused` (L16110).
- 23:30:38.232 — `Process 731 exited due to signal 9 (Killed)` (L16111).
- 23:30:38.233 — `Exit zygote because system server (pid 731) has terminated` (L16112).
- 23:30:38.780–39.581 — DeadSystemException cascade in 2223 / 2281 / 1548.
- 23:32:06.030 — `System server process 2437 has been created` (L16930).

Killer exclusion, each with its count (analyzer + independent greps):

| Candidate | Evidence | Verdict |
|---|---|---|
| GateKeeper-NPE self-kill (T743 mechanism) | NullPointerException 0; system_server_crash DropBox 0; KillApplicationHandler 0; reportDeviceSetupComplete 0 | RULED OUT — the T743 mechanism did NOT recur |
| Watchdog | W/Watchdog 0; Watchdog KILLING 0 | RULED OUT |
| OOM / LMK | `Killed process` 0; OOM-killer 0; lowmemorykiller kill-decision 0; OomAdjuster `Not killing` 8× (last 23:27:53, ~2.5 min before the death); the lmkd data-connection drop at 23:30:36 is a death SYMPTOM (T742 lesson), not LMK involvement | RULED OUT |
| init | `Sending signal 9 to service system_server` 0 (32 init signal-9 lines total, none names system_server) | RULED OUT |
| ActivityManager | `Killing ... system_server` 0; `Force finishing` 0; am_kill victim is only 1885 (killed BY 731) | RULED OUT |
| Flood loss hiding the kill line | 148 timestamped lines in ±60 s around the death = 1.2 lines/s; a kill line naming the pid would be visible | RULED OUT |
| `Killing 731` anywhere | grep count 0 | No such line exists |

First failing link for the successor track: system_server pid 731 SIGKILL at
guest ~1145.27 s, killer UNIDENTIFIED, with the T743 GateKeeper mechanism
excluded. The wall is the T742 class
(SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED) in a new instance — the T743
closure covered the 1960/1970 GateKeeper mode only, not this one.

## Holds — all met

| Hold | Count | Lines |
|---|---|---|
| T723_PROBE executed (start lines) | 2 | 356, 357 |
| T723_PROBE DONE summary | 1 | 387 |
| T723_PROBE SP1 DONE / SP2 DONE / SP3 DONE | 1 / 1 / 1 | 378 / 386 / 372 |
| T723_PROBE DROP_FAIL / RESOLVE_FAIL / GETPROCADDRESS_NULL | 0 / 0 / 0 | — |
| SP1 dup rc=-1, SP2 dup rc=-1 | 1 / 1 | 376 / 384 |
| T720_PROBE DONE status=PASS | 1 | 230 |
| T721 fence log | 1 | 14631 |
| Invalid present fence | 0 | — |
| REJECT allocate2 | 0 | — |
| EPERM | 0 | — |
| simplefb: mapped | 1 | 14629 |
| sys.boot_completed | 22 | 13688 … 25648 |
| Kernel panic / Oops / BUG | 0 | — |
| [T737] hooks | 9 | 346 … 392 (mounts rc=0, unmount proc rc=0, probe finished rc=0) |
| [T739] unmount hooks | 7 | 524 … 589 (all rc=0, handoff verified, /data mounted) |
| [T740] orphan kill hooks | 3 | 493–495 (t493 pid 202, t501 pid 203 killed) |
| Post-handoff spam | 0 / 0 | after L589 |
| T734_KERNEL_OK | 1 | 193 |
| T733_PROBE_OK / T728_PROBE_OK | 3 / 3 | explained by restart + leg-2 repeat (see above) |
| Watchdog KILLING (recorded either way; NOT a hold) | 0 | — |

## Trigger-file parser verdict

`scripts/analyze_t744.py` extends the T743 analyzer with 7 new T744 sections
(SIGSEGV inventory, RenderThread cluster, 3235 dying window,
death-notification chain, tombstone capture audit, candidate exhaustion,
bluetooth cluster). Regression on flight-21 reproduces the T743 counts exactly
(T720 PASS 1, Invalid 0, REJECT 0, EPERM 0, simplefb 1, sys.boot_completed 13,
FATAL EXCEPTION 11, "Not killing" 28).

Parser verdict on flight-22: TRIGGER ABSENT. Zero 3235 fault lines, zero
death-notification chain lines, zero RenderThread-cluster lines, zero
binder-thread Fatal signal 11 lines. The parser's SIGSEGV inventory for
flight-22 is exactly 2 lines, both bluetooth (1848 @ 0x67750, 3022 @ 0x9b51e).
Parser verdict on flight-21 (regression): TRIGGER PRESENT, all 5 mining counts
reproduced. The parser is sound; the difference is the flights, not the tool.

## Dying-window timeline — the 53 s window (flight-21, L48852–L50040)

The 3235 SIGSEGV dying window, from the mining record (flight-21; flight-22
has no dying window — no fault occurred):

- 23:38:26.098 — binder thread 4302 (`binder:3235_F`) SIGSEGVs at `0xd1008403`
  (L48852). Fault line: the ONLY record of the fault itself.
- 23:38:27.229 — tombstoned crash request for pid 4760 (systemui ANR,
  unrelated).
- 23:38:32.347 — tombstoned Java-backtrace intercept for pid 3235 registered.
- 23:38:40.365 — crash_dump64 obtains output fd.
- 23:38:40.368 — tombstoned receives crash request for pid 4302.
- 23:38:42.366 — Java backtrace intercept TERMINATED due to timeout (10 s).
- 23:38:42.369 — libdebuggerd_client "failed to read status response from
  tombstoned: timeout reached?".
- 23:38:42.374 — ActivityManager "Dumping Java threads failed, initiating
  native stack dump".
- 23:38:43.103 — native backtrace intercept registered.
- 23:38:45.105 — native backtrace intercept TERMINATED due to input (2 s) —
  the dump produced NO data.
- 23:39:18.939 — system_server 3235 death (L50040–50041, signal 11), ~53 s
  after the fault. The process kept running through the window (Looper, AM,
  OomAdjuster alive).

Capture audit for that window: NOTHING captures a system_server native crash's
stack in this guest. No F/DEBUG block for pid 3235 (count 0); no tombstone file
(tombstone_02+: 0); no DropBox entry for 3235's native crash; debuggerd /
crash_dump64 produced nothing — Java intercept timed out TWICE (23:37:25,
23:38:42), native intercept "due to input" TWICE (23:37:29, 23:38:45); 7
tombstoned intercepts for pid 3235 total (4 ANR Java dumps + 1 native + 2
dying-window). Two layers: (1) debuggerd/crash_dump64 produce nothing;
(2) NativeTombstoneManager would drop it anyway (UID 1000).

## RenderThread 0x8893 cluster — table (flight-21; flight-22: 0)

A separate, fully-named defect: 5× identical RenderThread SIGSEGVs, all fault
addr `0x8893`, pc `0x4fe7a4` in libhwui `createPatternedIndexBuffer`, with
x0=x1=x12=x15=`0x8893` in all 5 — a deterministic Skia/Ganesh defect in the
QEMU software-GL environment. Full F/DEBUG blocks exist for all 5 (debuggerd
DOES capture app native crashes).

| # | Pid | Call path into createPatternedIndexBuffer | Fault addr |
|---|---|---|---|
| 1 | 3824 | text — `AtlasTextOp::onPrepareDraws` | 0x8893 |
| 2 | 4022 | text — `AtlasTextOp::onPrepareDraws` | 0x8893 |
| 3 | 4204 | quads — `LatticeOp::NonAALatticeOp` → `refNonAAQuadIndexBuffer` | 0x8893 |
| 4 | 3851 | quads — `LatticeOp::NonAALatticeOp` → `refNonAAQuadIndexBuffer` | 0x8893 |
| 5 | 4618 | quads — `LatticeOp::NonAALatticeOp` → `refNonAAQuadIndexBuffer` (launcher3; the one whose death started the 3235 chain) | 0x8893 |

Flight-22: the cluster is ABSENT (0 in every count — no
`createPatternedIndexBuffer`, no `AtlasTextOp`, no `refNonAAQuadIndexBuffer`,
no 0x8893 registers, no F/DEBUG RenderThread blocks). The defect is real on
flight-21 evidence but does not fire every flight. Plus 4 independent bluetooth
SIGSEGVs in flight-21 at distinct addrs (0x669c4, 0x6ef20, 0x8b81e, 0x959e0;
pids 2550/2844/3770/4321; no inline backtrace) — a separate defect, which DID
recur in flight-22 (2×: 1848 @ 0x67750, 3022 @ 0x9b51e).

## Tombstone capture audit (flight-22 corroboration)

- F/DEBUG lines in flight-22: 60 (app crashes have blocks; no block for a
  system_server native crash — none occurred).
- Tombstone lines, case-sensitive: 664. Only tombstone_00 exists (35 mentions);
  tombstone_01+: 0. tombstone_00 is the fingerprint HAL crash (SIGABRT), same
  as flight-21.
- SYSTEM_TOMBSTONE DropBox lines: 5 (the fingerprint HAL's tombstones).
- `not an app` lines: 1 (L25731, 23:35:32 — NativeTombstoneManager for
  system_server 2437: "Tombstone's UID (1000) not an app, ignoring").
- system_server_crash DropBox lines: 0 (flight-21 had 6 = ONE file, pid 1970's
  GateKeeper-NPE self-kill).
- Death-window tap (55,774 B): the tombstoned intercepts for system_server pid
  731 show the two failure modes AGAIN — Java-backtrace intercepts terminated
  "due to timeout" twice (23:28:41, 23:29:40) and native-backtrace intercepts
  terminated "due to input" twice (23:28:52, 23:29:54). The Java path DID work
  once (23:20:11–12: "Wrote stack traces to tombstoned"). The capture gap is
  specific to NATIVE crashes: the native-backtrace intercept fails "due to
  input" for system_server in both flights, while the Java-backtrace path
  works. A system_server native crash's stack is therefore NOT capturable in
  this guest without an image change — no guest-side writer exists, the serial
  is output-only, and no `/proc/<pid>/maps` dump is emitted (the only `path=`
  maps line is the T723 SP2 libEGL line, L383). `0xd1008403` cannot be resolved
  to a library/offset from either flight.
- Kmsg tap (2,477 lines): 0 "Out of memory", 0 "Killed process" — agrees with
  the UART (2,476 kernel-format lines).

## GateKeeper-HAL-stub assessment (recommendation only — NOT built this track)

- The HAL: GateKeeper (`android.service.gatekeeper.IGateKeeperService`). The
  guest's GateKeeper HAL service never registers under QEMU, so the service
  stays null and the device-provisioned observer's
  `IGateKeeperService.reportDeviceSetupComplete()` call throws the NPE that
  kills each system_server instance ~5 min in (T743 mechanism, confirmed
  recurring as pid 1970 in flight-21).
- The stub: a minimal GateKeeper HAL service that registers
  `IGateKeeperService` and answers `reportDeviceSetupComplete` as a no-op /
  success. Image change: add the HAL service binary + an init rc entry to the
  vendor image or the ramdisk.
- Effect: it would stop the recurring GateKeeper-NPE self-kill loop, letting a
  system_server instance live long enough to observe later deaths cleanly.
- It would NOT fix the 3235 SIGSEGV trigger (an independent input-channel
  UAF — the `connectionToken` NPE in `removeInputChannel` is a DIFFERENT NPE
  from the GateKeeper one), and it would NOT fix the 731 SIGKILL (the
  GateKeeper mechanism is ruled out for 731; a stub changes nothing about an
  unidentified signal-9 killer).
- Recommendation: build the stub in a LATER track, once the 731 killer is
  named — the stub removes a known recurring death and lengthens clean
  observation windows, but building it now would conflate two independent
  deaths in one image change. NOT built this track, per the brief.

## WALL-TYPE-TAG

- SYSTEM_SERVER-SIGSEGV-BINDER-THREAD (guest, named, trigger) — the 3235
  input-channel UAF, named by flight-21 mining. Wall type: guest-Android
  defect (use-after-free in the input-channel teardown path, downstream of a
  non-deterministic launcher RenderThread crash). The trigger is NAMED; the
  wall is CLOSED as a question by this track, with the one inferential link
  (NPE thread vs faulting thread 4302) recorded as still inferential.
- SYSTEM_SERVER-SIGKILL-731-KILLER-UNIDENTIFIED (guest, new, open) — the
  successor track's question. Wall type: guest-Android SIGKILL with no named
  killer; GateKeeper-NPE, watchdog, OOM/LMK, init, and ActivityManager all
  excluded with counts. First failing link: pid 731 SIGKILL, guest ~1145.27 s,
  L16111–16112.
- RENDERTHREAD-0x8893-LIBHWUI (guest, named, non-deterministic) — the Skia /
  Ganesh `createPatternedIndexBuffer` defect, 5× in flight-21, 0× in flight-22.
  Wall type: guest graphics-stack defect in the QEMU software-GL environment.
- BLUETOOTH-SIGSEGV-CLUSTER (guest, named, recurring, separate) — 4× in
  flight-21, 2× in flight-22 (1848 @ 0x67750, 3022 @ 0x9b51e), distinct addrs,
  no inline backtrace. Wall type: guest bluetooth-service defect, independent
  of the 3235 chain.
- TOMBSTONE-CAPTURE-GAP-SYSTEM-SERVER-NATIVE (guest, image-change-needed) —
  a system_server native crash's stack is not capturable in this guest
  (debuggerd native intercept fails "due to input"; NativeTombstoneManager
  drops UID-1000 tombstones; no maps dump). Wall type: capture gap requiring
  an image change to close; the Java-backtrace path works, the native path
  does not.

## Rust par bhi lagu

Yes — this track's findings apply to the Rust emulator as well. The 3235
trigger is a guest-Android behavior (binder death notification → input-channel
teardown UAF), not a QEMU artifact: the Rust emulator must reproduce the same
binder death-notification and input-channel teardown sequence for the trigger
to be observable, and the same RenderThread/libhwui defect will fire under any
software-GL environment that runs the same guest. The tombstone-capture gap is
a guest property (debuggerd + NativeTombstoneManager behavior), independent of
the emulator. The dup-regression vectors (T723_PROBE SP1/SP2 dup rc, T720
PASS, T721 fence, Invalid/REJECT/EPERM zeros) carry over unchanged as the
Rust emulator's acceptance checks. The 731 SIGKILL with an unidentified killer
is a guest-side question the Rust build inherits: if the Rust emulator shows
the same death with the same exclusions, the killer hunt continues on the
guest side, not the emulator side.

## Reflection

- The non-recurrence is the lesson: a chain named from one flight is not a
  deterministic chain. The launcher RenderThread crash fired 5× in flight-21
  and 0× in flight-22, so the whole 3235 cascade is contingent on a
  non-deterministic upstream. The findings say so plainly instead of claiming
  flight-22 confirmed what it did not confirm.
- The 731 death is the second lesson: closing one killer question (T743 named
  the 1960 killer) does not close the death class. A new instance arrived with
  the known mechanism excluded, so the killer hunt restarts from the exclusion
  table rather than from zero — the exclusions ARE the progress.
- What changes: the successor track starts from the 731 exclusion table and
  the GateKeeper-stub recommendation (build the stub only after the 731 killer
  is named, so one image change does not conflate two deaths). The
  bluetooth cluster and the RenderThread 0x8893 defect stay open as separate,
  already-named defects.

## Evidence

- Flight-22 UART `flights/service-flight-22/uart.log` — 27,606 lines,
  3,740,533 B, SHA `9653b78c` (flight-end.json agrees).
- `flights/service-flight-22/flight-end.json` SHA `3c8044d5`; `preflight.json`
  SHA `297bd4b3`; `qemu-command.txt` SHA `79d4978f`; `qemu-stderr.log` SHA
  `3c55a1c4` (supervisor SIGTERM at target — clean shutdown); 
...[truncated 1409 chars]