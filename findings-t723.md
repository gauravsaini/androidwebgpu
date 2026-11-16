# T723 findings — EGL dispatch probe flight (recovery lane)

## Verdict: FAIL — the probe never ran; the EGL dispatch question is unanswered

- Track: T723 (recovery of the first T723 agent, which exited cleanly on WAIT-DISK at 15:22:01 AEDT 16 Nov 2026).
- Recovery agent launched 16:24:30 AEDT 16 Nov 2026 (driver clock read; wedge 19:24:30, not hit).
- Flight: `probe-flight`, launched 16:27:17 AEDT, ended 16:37:38 AEDT. QEMU exit 0, supervisor clean exit.
- Adjudication (pre-stated, binding): PASS required a `T723_PROBE DONE` summary plus SP1 and SP2 answered plus holds intact. The flight produced **0 `T723_PROBE` lines, 0 DONE, 0 SP1, 0 SP2, 0 SP3**. The probe binary never executed.
- Wall-type tag: **RAMDISK-MOUNT-BLIND** — a background process forked from the early-ramdisk script before `/init` runs can never observe `/system` (or `/apex`, or properties), because first-stage init mounts the system image in a mount namespace the pre-forked loops do not share. The wait-loop's sleep and detector both depended on `/system/bin/*` from that blind context, so the loop burned its whole 600 s budget in ~30 s of guest time and exited ~483 s before `sys.boot_completed` was set.

## 1. Flight record

| Item | Value |
|---|---|
| Worktree / branch / base | `/Users/Shared/wt-track723-egl-dispatch`, `t723`, `c6f9623` |
| Flight | `probe-flight` (supervisor PID 35186, QEMU pid 35194, GDB 1258) |
| Launched (AEDT) | 16:27:17 (05:27:17Z) |
| Ended (AEDT) | 16:37:38 (05:37:38Z), stop reason "Guest target reached (620.2s >= 600s)" |
| Host elapsed / guest max | 620.93 s / 620.21 s |
| QEMU exit code | 0 |
| UART | `flights/probe-flight/uart.log`, 26,966 lines, 3,138,565 bytes, SHA-256 `0b8d8841f562f36d3cc6f345766b75c9b4dd978db3427d38bad6a645705731bf` |
| flight-end.json | written by supervisor; inputs SHAs match the pre-flight verification |
| Analyzer | `scripts/analyze_t723_probe.py` run on the UART, exit 0; report saved as `flights/probe-flight/analysis-t723.txt` (SHA `3800c6f9af2b85132e37318cd98a16ac36820f54025b33546c5a7fb67f5e0250`) |
| Mac df (KiB) | 11,147,184 at launch → 11,100,120 at close; gate 8,388,608 KiB — gate open throughout, no disk event |

Flown stack (verified pre-flight, Step 1, 8/8 SHAs; confirmed again in `preflight.json` and `qemu-command.txt`): kernel Image `70d3859d`, CPIO `a9d6040a`, DTB `73d1904b`, revert system.img `0ba6d6d3`, vendor `5e0607ad` (embedded member PASS). QEMU cmdline: kernel + DTB + CPIO loader + virtio-blk system only, no vendor drive. No rebuild in this lane; no fix build; no control flight owed.

## 2. Adjudication against the pre-stated criteria

| Criterion (brief) | Required | Observed | Result |
|---|---|---|---|
| `T723_PROBE DONE` summary | ≥ 1 | 0 | FAIL |
| SP1 DONE (direct driver call) | ≥ 1 | 0 | FAIL |
| SP2 DONE (META-EGL dispatch call) | ≥ 1 | 0 | FAIL |
| SP3 DONE (sw_sync control) | ≥ 1 | 0 | FAIL |
| DROP_FAIL | 0 | 0 (vacuous — probe never ran) | — |
| SP1 RESOLVE_FAIL | 0 | 0 (vacuous) | — |
| SP2 GETPROCADDRESS_NULL | 0 | 0 (vacuous) | — |

Holds (all intact):

| Hold | Required | Observed |
|---|---|---|
| T720_PROBE DONE status=PASS | ≥ 1 | 1 (UART L228) |
| T720_PROBE DONE status=FAIL | 0 | 0 |
| T721 fence-log lines | ≥ 1 | 1 (L23888: `T721 fd=10 tl=9`, pid 2718) |
| Invalid present fence | 0 | 0 |
| T628 REJECT allocate2 | 0 | 0 |
| EPERM | 0 | 0 |
| `simplefb: mapped` | ≥ 1 | 1 (L23887) |
| Watchdog KILLING | 0 | 0 |
| `sys.boot_completed` | ≥ 1 | 8 (first L21534, guest 516.118 s) |
| Kernel panic / Oops / BUG | 0 | 0 |

The FAIL is exactly "probe never ran": the trigger failed, not the probe, not the guest. SP1/SP2 and survivors R1/R3/R4 remain unanswered. The static dispatch chain proven by probe (a) stands; no runtime evidence was produced for or against it.

## 3. Evidence — the trigger failed, the probe never executed

### 3.1 The only T723 line in the UART is the timeout

UART L2018 (the single `t723` match, case-insensitive):

```
[T723] boot_completed never detected in 600s; probe not run
```

No `boot_completed detected`, no `probe finished rc=`, no `probe binary missing`, no `T723_PROBE` line of any kind. The hook's success branch never printed; the probe binary never executed.

### 3.2 The loop exhausted its budget in ~30 s, ~483 s before the event

The timeout line sits at UART L2018, bracketed by kernel lines `[   33.366712]` and `[   33.414050]` — guest ≈ 33.4 s, i.e. ≈ 16:27:50 AEDT. The loop was launched from the early-ramdisk at guest ≈ 3 s. 300 iterations in ≈ 30 s is ≈ 0.1 s per iteration, not 2 s. `sys.boot_completed=1` was first set at guest 516.118 s (UART L21534, init action; ≈ 16:35:53 AEDT). The loop had already exited 482.7 s earlier. Even a perfectly working detector could not have fired — the loop was gone before the event existed.

### 3.3 First failing link: the hook's sleep and detector depend on `/system/bin/*` from a pre-mount context

Hook source, `guest-inputs/early-ramdisk-t723` lines 252–268:

```sh
# T723: EGL-dispatch probe (background; waits for boot_completed, runs once, never blocks boot)
t723_probe_wait() {
    exec >/dev/console 2>&1
    step=0
    while [ $step -lt 300 ]; do
        "$TOYBOX" sleep 2
        step=$((step + 1))
        bc=$(/system/bin/getprop sys.boot_completed 2>/dev/null || true)
        if [ "$bc" = "1" ]; then
            echo "[T723] boot_completed detected; running /t723probe"
            if [ -x /t723probe ]; then /t723probe >/dev/console 2>&1; echo "[T723] probe finished rc=$?"; else echo "[T723] probe binary missing"; fi
            return 0
        fi
    done
    echo "[T723] boot_completed never detected in 600s; probe not run"
}
t723_probe_wait >/dev/console 2>&1 &
```

with `TOYBOX=/system/bin/toybox` (line 4), backgrounded at line 268, before `exec /init` (line 274). Two dependencies, both on the system image:

- `"$TOYBOX" sleep 2` — the loop's only time source. `/system/bin/toybox` lives in the system image, which init mounts only after the loop is forked.
- `bc=$(/system/bin/getprop sys.boot_completed ...)` — the loop's only detector. Same location, same problem.

When both fail, the shell reports `inaccessible or not found`, the iteration costs ~0.1 s, and 300 iterations complete in ~30 s. The `2>/dev/null || true` on the getprop call swallows the error silently; the sleep failure is likewise silent in the loop (its error goes to the console, buried among 877 identical errors — see §3.4).

### 3.4 The blindness is systematic, not a T723 defect — the T552 control loop proves it

The same script carries an older loop, `run_probe` (the "T552 pattern" the T723 hook was modelled on), with the identical dependencies (`"$TOYBOX" sleep 2`, `/system/bin/getprop` for five `init.svc.*` properties and `sys.boot_completed`). In this flight:

- 52 `T552` lines total; 48 status lines (`t=10s` … `t=480s`, one per 5 steps, loop bound 240 steps — ran to completion).
- **All 48 status lines show every field empty** (`SF: '' | Allocator: '' | HWC: '' | HWSM: '' | SM: ''`). `getprop` never returned a value once.
- **0 `MILESTONE` lines.** It never detected system_server start (pid 2848, running) nor `sys.boot_completed=1` (set at guest 516 s), nor the `/apex/com.android.art` mount it also checks.
- Its timing anchor: `[T552-PROBE t=80s]` (step 40) appears immediately after the T723 timeout line at guest ≈ 33.4 s — ≈ 0.8 s per iteration, not 2 s. Same fast-spin signature.
- **877 UART lines match `inaccessible or not found`**, first at L456, last at L9886 — spanning the whole flight, including `/early-ramdisk[250]: /system/bin/getprop: inaccessible or not found` and `/early-ramdisk[250]: /system/bin/toybox: inaccessible or not found`. A second ramdisk script shows the same blindness (`/t501-keystore2-dump.sh: /system/bin/toybox: inaccessible or not found`).
- The T711 logcat streamer (ramdisk binary, also backgrounded from this script) never once executed `/system/bin/logcat`: 19 UART lines, all `[T711-STREAMER] logcat (pid=...) exited status=256, restarting in 1s...` (pids 693, 785, 876). `/guest_diag` (launched line ~271) printed nothing at all.

Three different `/system/bin` binaries (toybox, getprop, logcat) are all inaccessible to every ramdisk-backgrounded process for the entire flight, while init's own children run them normally (servicemanager, system_server, RenderEngine all up). The defect is the context, not the files.

### 3.5 The files exist in the flown image — this is not an image defect

Box debugfs (read-only) on `/home/muse/t721-work/t721-system-revert.img` (SHA `0ba6d6d3`, the flown image):

- `/bin` is a symlink to `/system/bin`.
- `/system/bin` contains `toybox` (560,720 B), `getprop` (symlink), `logcat` (206,280 B).

All three exist in the image the guest flew. The ramdisk-backgrounded processes cannot see them because first-stage init mounts the system image in a mount namespace the pre-forked loops do not share. (Mechanism inferred from the 877 errors plus the namespace behaviour; the observable facts — errors throughout the flight, zero successful executions — are directly evidenced.)

### 3.6 Contrast: the T720 probe worked because it is ramdisk-local

The T720 probe runs synchronously from the early-ramdisk at guest ≈ 3.8 s and uses only ramdisk-local facilities (it mounts debugfs itself, opens `/dev/sw_sync`). In this flight it printed its lines and `T720_PROBE DONE status=PASS timeline_fd=4 fence_fd=5 poll_rc=1 poll_revents=1 fileinfo_rc=0 fence_status=1 num_fences=1` (UART L228). The T723 hook's failure is precisely the addition of a `/system/bin/*` dependency into a ramdisk-context process. The probe binary itself is exonerated: it was never executed (CPIO member `t723-probe` present, SHA `4b275e48` verified pre-flight), so no verdict on the probe's own correctness is possible — its SP1/SP2/SP3 code paths remain untested.

## 4. What the flight proved incidentally (context, not adjudication)

- **Guest boots to completion on the T723 stack.** `sys.boot_completed` 8 lines (analyzer), first at guest 516.118 s. system_server pid 2848 stable through boot; Dexopt PERFORMED (108 Dexopt lines).
- **T720 fence source still live:** T720_PROBE DONE PASS (L228). **T721 retire fence still live:** `T721 fd=10 tl=9` (L23888), `simplefb: mapped 0x5f000000 1280x720` (L23887).
- **RenderEngine still dup-fails:** 4 × `failed to dup EGL native fence sync: 0x3000` (pid 2793, L24145/24276/24454/24673), rate 4 / 620.2 s × 1000 = **6.449 per 1000 s** — between the T721 flight-01 baseline (4.058) and the T721 revert baseline (6.660). Consistent with T722 (patch not reached), but this is a 600 s probe flight, not a differential — reported as context only, not a verdict. META-EGL banner present (L10911: `version   : 1.4 Android META-EGL`); T628 CALL allocate2: 12 lines.
- **Guest instability after boot (new observation):** system_server (2848) crashed late in the flight — dropbox crash file `system_server_crash@1699658742288.txt` (23:25:43), zygote received signal 9 at guest 560.16 s, 7 × `FATAL EXCEPTION` / `DeadSystemException: The system died` (systemui, acore, settings, others; cluster 23:25:47.013–.599, L25394–25591). init restarted the stack; new system_server pid 4367 was up by flight end (23:26:47). Kernel clean: 0 panic / Oops / BUG. Watchdog KILLING: 0. A successor probe must complete between `boot_completed` (≈ 516 s) and any late system_server crash, or tolerate the restart (the probe is an independent process; a system_server restart does not kill it, but EGL state may reset).
- **Futex regression vector:** 0 occurrences in this UART. Not applicable as a gate: the vector is defined on canonical Image `05c43917`; this flight flew rebuilt Image `70d3859d`, and this lane did no rebuild.
- avc denials: 2,106 total (permissive-mode audit noise), of which `/dev/mem`: 6, sensorservice: 0.

## 5. Reflection (standing rule: reflect before closing)

1. **What happened (evidence):** the probe flight ran cleanly to its 600 s guest target, but the probe never executed. The wait-loop printed its timeout at guest ≈ 33.4 s, 482.7 s before `sys.boot_completed` was set. Adjudication FAIL on the pre-stated criterion (probe never ran).
2. **Root cause:** the wait-loop's two dependencies — `"$TOYBOX" sleep 2` and `/system/bin/getprop` — resolve inside the system image, which is mounted by first-stage init in a mount namespace the pre-forked ramdisk loops never join. Both failed on every iteration, so the loop spun 300 iterations in ~30 s. The `|| true` / `2>/dev/null` error-swallowing hid the failure inside the loop; the only visible symptom was the timeout line itself.
3. **Missed signal:** the T552 control loop in the same script has the same dependencies and the same failure mode, and it has never detected `boot_completed` in any flight — its 48 status lines have shown empty fields in every UART that flew this script (T719–T723). That persistent blindness was never read as a warning. The first agent built the hook by analogy to "the T552 pattern" without verifying that the pattern had ever produced its claimed effect in a flown UART. A pattern's name carried unearned trust.
4. **What changes:** (a) A reused pattern must cite a flown UART line proving it produced its claimed effect — a name is not evidence (proposed standing rule PATTERN-PROOF, for driver disposal at certification). (b) Any guest wait-loop must emit a periodic progress line (≤ 30 s) so a fast spin or a stuck wait is diagnosable from the UART within the first minute (proposed standing rule WAIT-HEARTBEAT). (c) A wait-loop's sleep and detector must be proven in the loop's own context — the T720 probe's ramdisk-local design is the model, not the T552 shell loop. (d) Error-swallowing (`2>/dev/null || true`) around a loop's detector must be paired with a periodic diagnostic print, or removed — silent failure inside a trigger is the most expensive kind.
5. **Where it lives:** this findings §5 and §7, the T723 death note, and the two proposed standing-rule additions for the driver to dispose of at certification. The successor brief must carry the trigger design as an explicit step with its own pass criterion (trigger printed its detection line), not as an assumed preamble.

## 6. Successor options (for the driver; not a launch)

- **A. In-probe raw-syscall polling.** Extend the probe binary so it waits for `boot_completed` itself: raw `nanosleep` syscall for timing, direct reads of `/dev/__properties__` (or a `/proc` scan for system_server) for detection. No shell, no toybox, no getprop — ramdisk-local like the T720 probe. Verdict: viable fallback. Weakness: property-area parsing is fiddly, and a `/proc` scan detects system_server start, not `boot_completed` — a proxy, not the event.
- **B. init `.rc` service trigger. ⭐ RECOMMENDED.** Add an `on property:sys.boot_completed=1` service (in the ramdisk `init.rc` or the system init `.rc`) that execs `/t723probe` once. Android-native trigger, exact event, runs in init's mount namespace (sees `/system` if needed), no polling, no sleep dependency, no getprop. Cost: one `.rc` edit via CPIO member replacement — a proven operation in this chain. The probe binary stays byte-identical (`4b275e48`); it already opens `/dev/kmsg` as root and drops to uid 1000 itself.
- **C. Host-side trigger via UART watch + GDB injection.** Supervisor greps the UART for `sys.boot_completed`, then starts the probe via GDB/QMP. Verdict: reject. No guest execution channel exists (no adb, no vsock agent); GDB exec of a guest binary is not a supported path in this harness; most moving parts, least proven.

The successor reuses the staged probe binary (`4b275e48`) and the T723 stack unchanged (kernel `70d3859d`, CPIO `a9d6040a`, DTB `73d1904b`, revert system `0ba6d6d3`, vendor `5e0607ad`) unless the trigger change itself requires a rebuild — in which case SHA discipline (including the embedded-vendor check) applies again. The successor brief must require the trigger's own detection line as a step pass criterion, and the wait mechanism must print a progress heartbeat (≤ 30 s) so a spin is visible in the first minute of UART.

## 7. Artifact inventory (this lane)

| File | Location | SHA-256 (first 8) / size |
|---|---|---|
| UART | `flights/probe-flight/uart.log` | `0b8d8841…`, 3,138,565 B, 26,966 lines |
| flight-end.json | `flights/probe-flight/` | QEMU exit 0, host 620.93 s, guest 620.21 s |
| preflight.json | `flights/probe-flight/` | inputs SHAs match Step 1 |
| qemu-command.txt | `flights/probe-flight/` | flown stack as §1 |
| analysis-t723.txt | `flights/probe-flight/` | `3800c6f9…` (analyzer exit 0) |
| uart.log.numbered.txt / uart.log.gz | `flights/probe-flight/` | supervisor-generated |
| findings-t723.md | worktree root | this file |
| outcome-t723.txt | worktree root | `FAIL` (5 B) |
| DEATH_NOTE-T723.md | worktree root | death note |
| TIMELINE.md | worktree root | this lane's timeline |
| progress-t723.txt | worktree root | appended (flight + adjudication) |
| heartbeat.json | worktree root | CLOSED FAIL |

No agent commits were made in this lane. No rebuild. No control flight flown (none owed). Gates G723-1/2 remain UNCHECKED — adjudication is driver work.
