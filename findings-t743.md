# T743 findings — the killer of system_server pid 1960, NAMED

**Verdict: PASS**

**Track:** T743 — identify the killer of system_server pid 1960 (SIGKILL at guest ~1230.1 s in T742 flight-20, UART L24670). Killer was UNIDENTIFIED after full candidate exhaustion in flight-20.

**Answer in one line:** system_server killed ITSELF. An uncaught `NullPointerException` — `IGateKeeperService.reportDeviceSetupComplete()` on a null object reference, `LockSettingsService.java:3394`, main thread — is caught by `RuntimeInit$KillApplicationHandler`, which writes a `system_server_crash` DropBox report and then kills its own process with SIGKILL. No external killer exists: no AM kill, no init kill, no kernel OOM, no LMK kill. The mechanism was found by tombstone mining on flight-20 (Step 2) and **confirmed recurring in flight-21 with pid 1970, byte-identical signature**.

## Flight

- **service-flight-21**, full 1500 s target. Started 2026-11-21T00:51:17Z (11:51:17 AEDT), ended 01:16:44Z (12:16:44 AEDT). Stop reason: "Guest target reached (1523.9s >= 1500s)". Host elapsed 1527.76 s, guest log max 1523.924 s, **QEMU exit code 0**.
- UART: **50,960 lines / 6,726,819 B**, SHA256 `c00a068caeb2ac6575d294318a54bd5c5048417d1cb773843632cc8b63b3d536`.
- Supervisor PID 96645, QEMU PID 96666. Disk gate OPEN at launch (10,570,672 KiB free, 96%).

## Inputs — identical stack, all 6 SHAs match Step 1 verification

| input | SHA256 (first 8) | bytes |
|---|---|---|
| kernel t720-Image | `70d3859d` | 20,396,544 |
| qemu-system-aarch64 | `8e5c46dc` | 31,498,864 |
| dtb t741-modified.dtb | `4ecab141` | 2,049 |
| boot image t741-fix.cpio.gz | `c1203336` | 53,753,325 |
| system t733-system.img | `af40403a` | 2,059,816,960 |
| vendor t741-vendor.img | `fe0bc359` | 134,217,728 |

No kernel rebuild, no vendor/CPIO/DTB change. Dispatch (T736 PASS) and the dup path (T741 PASS) were not re-diagnosed.

## Boot, probe, dup regression — all green

- **Boot completes.** First `sys.boot_completed` firing at **guest 882.325 s** (UART L23906 — the same concatenated-line pattern as T742 L20510: a logcat line fused with a `[  882.324860] init: processing action (sys.boot_completed=1)` kernel line). **13** `sys.boot_completed` lines total (L23906, 23953, 24359, 24412, 24449, 25277, 25321, 25328, 25340, 25355, 25356, 39743, 39783). Literal first line:
  `11-10 23:30:30.588 W/HidlServiceManagement( 1827): Waited one second for android.hardware.media.c2@1.0::IComponentStore/software[  882.324860] init: processing action (sys.boot_completed=1) fr...`
- **T723 probe: 27 lines**, identical shape to every prior flight (start L358–359, DONE L389, SP1/SP2/SP3 DONE, DROP_FAIL 0, RESOLVE_FAIL 0). SP1 dup rc=-1, SP2 dup rc=-1 error=0x3000 — the known T736 baseline (sw_sync EACCES as uid 1000), not a dispatch failure.
- **Dup regression HOLDS.** RenderEngine EGL dup-fail lines: **0** in **641.6 s** presenting (guest max 1523.924 − first boot 882.325). Dup-fail rate **0.000 per 1000 s**, below BOTH T722 baselines (4.058 / 6.660). Presenting window is nearly double flight-20's (349.4 s) with the same zero.
- **Holds (all intact):** T720_PROBE DONE status=PASS 1 (L227); T721 fence-log 1 (L12069); Invalid present fence 0; T628 REJECT allocate2 0 (CALL 46); EPERM 0; simplefb mapped 1 (L12068); sys.boot_completed 13; kernel panic/Oops/BUG 0. Watchdog KILLING lines: 1 (pid 683, L13549) — explicitly NOT a hold in T743; it names its victim (see below).
- **Unmount hooks fire, all rc=0:** [T737] mount /sysimg, /vendor, debugfs, proc all rc=0; unmount proc rc=0 (L393); [T739] unmount /sysimg (L526), /vendor (L555), debugfs (L584) all rc=0; handoff state verified (L586), /data mounted (L587), unmount proc verify rc=0 (L590), handoff line L591. **Post-handoff spam 0 / 0** (T739 flight-14 was 4480 / 337).
- [T740] orphaned-script kill hook fired (L495–497: t493 pid 201, t501 pid 202 killed).

## The killer, named — pid 1970 dies the 1960 way (flight-21 recurrence)

Three system_server incarnations died in flight-21 (created: 683 → 1970 → 3235 → 5304, the last alive at flight end). Each death has a different, named mechanism. The middle one is the 1960-mode recurrence.

### Death 2 — pid 1970: GateKeeper NPE self-kill (THE ANSWER, confirmed recurring)

Timeline (all 11-10 23:31, guest ~954 s):

- **23:31:45.142, L28993–28994** — `E/AndroidRuntime( 1970): *** FATAL EXCEPTION IN SYSTEM PROCESS: main` — `java.lang.NullPointerException: Attempt to invoke interface method 'void android.service.gatekeeper.IGateKeeperService.reportDeviceSetupComplete()' on a null object reference`, at `com.android.server.locksettings.LockSettingsService$DeviceProvisionedObserver.reportDeviceSetupComplete(LockSettingsService.java:3394)`.
- **23:31:45.146, L29022** — `I/am_crash( 1970): [1970,0,system_server,-1,java.lang.NullPointerException,...,LockSettingsService.java,3394,0]`.
- **Crash file, L27538–27570** — `system_server_crash@1699659105424.txt` (DropBox ts 23:31:45.424, ~0.3 s after the NPE). Contents: `Crash-Handler: com.android.internal.os.RuntimeInit$KillApplicationHandler` (L27541); the NPE + LockSettingsService frame (L27545–27546); full stack through ContentObserver dispatch → ContentService.notifyChange → SettingsProvider → Looper → `SystemServer.run` → `ZygoteInit.main` (L27547–27569).
- **23:31:51.754, L29526** — `I/Zygote ( 1829): Process 1970 exited due to signal 9 (Killed)`, followed by L29529 `Exit zygote because system server (pid 1970) has terminated`. NPE → death gap ≈ 6.6 s; crash-file → death gap ≈ 6.3 s. The handler writes the report, then kills its own process — that is the SIGKILL.

Root-cause chain: the GateKeeper HAL never comes up under QEMU → `IGateKeeperService` stays null → the device-provisioned observer fires on setup-complete → NPE on the main thread → KillApplicationHandler self-kill. The storaged/IStoraged NPE elsewhere in the log is a separate CAUGHT exception, not fatal.

**This is the same death flight-20's pid 1960 died** (crash file L23203–23245, ts 1699659367335, death L24670 ~3.4 s later, identical NPE + handler + file) and the same death flight-18's pid 678 died (crash file L16807, ts 1699658755049, death L18508 ~5.2 s later). **The T741 record's "killer unidentified" for pid 678 is corrected by this finding** — 678 was a GateKeeper-NPE self-kill too. (T741's findings file itself is not rewritten here; the driver owns the record.)

### Death 1 — pid 683: watchdog kill (known mode, third occurrence in the chain)

- **23:26:36.222, L13549** — `W/Watchdog(  683): *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in handler on  on main thread (main) for 104s`. Dump file `system_server_watchdog@1699658796217.txt.gz` (L13143–13307, ts 23:26:36.217 — 0.005 s before the KILLING line).
- **23:26:37.765, L13972** — `Process 683 exited due to signal 9`, L13973 Exit zygote. KILLING → death gap ≈ 1.5 s.
- Same mode as T742's pid 705 (111 s block). Watchdog kills are a named mechanism, not an unidentified killer.

### Death 3 — pid 3235: native SIGSEGV in a binder thread (NEW question, not this track's mission)

- **23:38:26.098** — `F/libc ( 3235): Fatal signal 11 (SIGSEGV), code 1 (SEGV_MAPERR), fault addr 0xd1008403 in tid 4302 (binder:3235_F), pid 3235 (system_server)`.
- **23:39:18.939, L50040** — `Process 3235 exited due to signal 11 (Segmentation fault)`, L50041 Exit zygote. Fault → death report gap ≈ 53 s: the process spent ~53 s dying while debuggerd dump attempts timed out (tombstoned intercepts for pid 3235 at 23:38:32, 23:38:42 timeout, 23:38:43, 23:38:45).
- No tombstone file for 3235 exists (tombstone_02+: 0; zero tombstone lines within ±200 of L50041; no `system_server_crash` file for 3235 — the flight carries exactly ONE, pid 1970's). Mechanism for the missing dump: `NativeTombstoneManager` logs `Tombstone's UID (1000) not an app, ignoring` — system_server's own tombstones are dropped because UID 1000 is not an app.
- Root-cause trigger of the binder-thread segfault is **unidentified from this UART** — faulting thread and address are known, the trigger is not. This is the successor track's question (proposed wall below). It is NOT a T743 failure: the mission is the 1960 killer, which is named and confirmed recurring.
- Context: the late flight also carries a systematic app fault — 5× RenderThread SIGSEGV at fault addr 0x8893 (settings/launcher, pids 3824/4022/4204/3851 + one more) — and 5 app SIGSEGV deaths (3824, 4022, 4204, 3851, 4618) before 3235's own fault. Apps segfaulting first, then system_server, is noted, not explained.

### Every other killer candidate: zero, again

- ActivityManager `Killing ... system_server`: **0**. am_kill lines: 10, victims are apps only (system_server appears as KILLER, never victim). `Force finishing`: 0.
- init `Sending signal 9 to service system_server`: **0** (33 init signal-9 lines total, all other services).
- Kernel OOM: **0** `Out of memory`, **0** `Killed process`, **0** `oom-killer` — in the UART and independently in the live kmsg tap (see below).
- `Low on memory`: 0. onTrimMemory: 0. Numeric Mem-Info lines: 0.
- Flood check around all 20 SIGKILL deaths: 1.3–4.2 lines/s in ±60 s — a kill line naming the pid would still be visible; nothing is flood-masked.

## Tombstone mining (Step 2, flight-21 confirmation)

- Tombstone lines (case-sensitive): **1,346** (flight-20: 631). Exactly TWO tombstone files exist:
  - **tombstone_00** (L23911–24730 + .pb L23634): fingerprint HAL service, **pid 2658**, SIGABRT, `Could not open the sensor service: 'fingerprintlisten'` — same HAL, different pid than flight-20's 2688.
  - **tombstone_01** (L37667–38583 + .pb L37347): same fingerprint HAL service, **pid 3845**, SIGABRT, same abort message — the HAL reforks and aborts again.
  - **tombstone_02+: 0.** No tombstone names any system_server pid.
- Tombstone lines within ±200 of each system_server death (Exit-zygote L13973, L29529, L50041): **0 / 0 / 0**.
- Crash-file inventory: **74 header/footer lines = 37 files** — pre_watchdog ×3, system_server_watchdog ×1 (683), system_app_anr ×13, SYSTEM_TOMBSTONE ×2 + SYSTEM_TOMBSTONE_PROTO_WITH_HEADERS ×2, system_app_native_crash ×5, system_server_crash ×1 (1970), tombstone_00/01 (+ .pb + .pb tmp) ×6, drop tmp ×4. `system_server_pre_watchdog` lines: 20 (watchdog noticed slowdowns but did not kill — including for 1960 in flight-20, L18158).

## The two new evidence streams (Step 3 design) — both delivered, both cross-checked

### (a) kmsg tap — kernel lines, live

- `flights/service-flight-21/kmsg-service-flight-21.log`: **3,249 lines** (1 header comment + 3,248 kernel-format lines), 438,193 B, SHA256 `226ad7b83d2a9e8b6a79411b686e2ebbee7dae6798316732bac0ff1270d2ecf6`.
- Sorted diff against the UART's kernel-format lines: **exact match** — the only diff line is the tap's own header comment. Tap ⊆ UART, verified, not assumed.
- **0** `Out of memory`, **0** `Killed process`, **0** `oom-killer` in the tap and in the UART's 3,248 kernel-format lines. A kernel-side kill line cannot be logcat-dropped (printk bypasses logcat) — and none exists, including around all three deaths.

### (b) meminfo-pressure tap — the guest's own memory kill-decision state, live

- `flights/service-flight-21/meminfo-pressure-service-flight-21.log`: **60 lines** (1 header + 59 captured), 7,619 B, SHA256 `bf4819188dc498e2d769de7a9e7b8d4e61baabf660d9bb7da1d37f7b02bd92c1`.
- **28× `OomAdjuster: Not killing cached processes`** spanning all three incarnations (683 ×6, 1970 ×8, 3235 ×14) — ActivityManager evaluated memory repeatedly through the whole flight and chose NOT to kill. Last "Not killing" before each death: 23:26:33.108 (3.1 s before the 683 KILLING line), **23:31:43.345 (1.8 s before the 1970 NPE)**, 23:38:40.527 (38 s before the 3235 fault report; the fault itself was 23:38:26).
- 3× `lmkd data connection established` + `Connection with lmkd established` (one per refork) and 3× `lmkd data connection dropped` + `closing lmkd data connection` (one per death). The drops are death symptoms, not causes — each follows its fatal trigger and precedes its death report by <1 s:

| death | fatal trigger | lmkd conn-drop | death report | drop → death |
|---|---|---|---|---|
| 683 (watchdog) | KILLING 23:26:36.222 | 23:26:37.394 | 23:26:37.765 | 0.37 s |
| 1970 (NPE self-kill) | NPE 23:31:45.142 | 23:31:50.919 | 23:31:51.754 | 0.84 s |
| 3235 (SIGSEGV) | fault 23:38:26.098 | 23:39:17.934 | 23:39:18.939 | 1.01 s |

- The 12 `lowmemorykiller` lines in the UART are all connection setup/teardown + psi-monitor info (`Using psi monitors...`, `Process polling is supported`, `Process reaping is not supported`) — no kill decision anywhere. 13 odrefresh AVC-denied lines about lmkd properties are permissive=1 noise (logged, not enforced).
- Numeric `/proc/meminfo` was NOT streamed, as designed and recorded in the brief: no guest-side writer can be added without an image change, and the harness serial is output-only (no SysRq injection). The tap measures the operative question — the guest's own memory subsystems' kill-decision state — and the answer is unanimous: not killing, all flight.

## Parser verdict

- Extended analyzer `scripts/analyze_t743_probe.py` (SHA256 `8924a3bb05ce055981cb9bb5cc59e18a3f619a56cb0448646d102c2b6e6a196c`) ran on flight-21's UART, exit 0 → `analysis-t743.txt` (662 lines, 43,274 B, SHA256 `4559249c055c62ab7670d58730c4f8afe148d5c7246c6585efa3d17ebcbf6723`). Supervisor `scripts/supervisor_t743.py` SHA256 `265464e485bbdd3fb065d9421cf4b85ed48d91fe853328ee97291a1f03d70c46`.
- **Regression vs flight-20 reproduces T742 counts exactly:** the same extended analyzer run on flight-20's UART (`analysis-t743-regression-flight20.txt`, 554 lines) differs from `analysis-t742.txt` by exactly 109 lines — the 3-line first-boot correction (1167.684 → **1167.317 s**, presenting 349.1 → **349.4 s**, rate still 0.000) plus the 4 new sections only. All T742 counts reproduce (tombstone 631, watchdog 9, pre_watchdog 18, SYSTEM_TOMBSTONE 14, tombstone_00 39, tombstone_01+ 0, crash file L23203, KillApplicationHandler 1, NPE 2, kmsg lines 1920, OOM 0, OomAdjuster "Not killing" 15, meminfo 0).
- Note for the reader: `analysis-t743.txt` contains raw binary bytes (the gzipped DropBox dumps are binary in the UART), so `grep` treats it as binary — use `grep -a`. Same for the UART itself.

## WALL-TYPE-TAG

**SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED (guest) — CLOSED.** The killer is named (GateKeeper-NPE self-kill via KillApplicationHandler) and confirmed recurring in flight-21 (pid 1970, byte-identical to flight-20 pid 1960 and flight-18 pid 678).

**New open wall for the successor: SYSTEM_SERVER-SIGSEGV-BINDER-THREAD (guest).** system_server pid 3235 segfaulted in a binder thread (tid 4302, fault addr 0xd1008403, SEGV_MAPERR); no tombstone was written (UID-1000 dumps are ignored by NativeTombstoneManager); trigger unidentified from this UART. Proposed next track: mine the 53 s dying window + the binder-thread fault for a trigger, and/or stub the GateKeeper HAL so the NPE self-kill stops masking later deaths.

## Rust applicability

The finding is guest-Android behavior, not emulator dispatch: the GateKeeper HAL never comes up under QEMU, so any emulator (QEMU scaffold or the Rust destination) that boots this guest without a GateKeeper HAL — real or stubbed — will hit the same NPE self-kill loop; the Rust emulator inherits the requirement to provide/stub the HAL, and its dispatch path is unaffected by this track.

## Files (this worktree, branch t743, base 6e716e5 — no commit; certification is the driver's)

- `findings-t743.md` (this file)
- `outcome-t743.txt` — `PASS`
- `TIMELINE-t743.md`
- `progress-t743.txt` (append-only)
- `heartbeat.json` — CLOSED, verdict PASS
- `analysis-t743.txt` — extended analyzer output on flight-21
- `analysis-t743-regression-flight20.txt` — regression vs flight-20
- `scripts/analyze_t743_probe.py`, `scripts/supervisor_t743.py` — extended analyzer + supervisor
- `flights/service-flight-21/` — `uart.log` (SHA `c00a068c…`), `uart.log.gz`, `uart.log.numbered.txt`, `kmsg-service-flight-21.log` (SHA `226ad7b8…`), `meminfo-pressure-service-flight-21.log` (SHA `bf481918…`), `flight-end.json` (SHA `819b29d8…`), `preflight.json`, `qemu-command.txt`, `qemu-stderr.log`, `supervisor.log`
