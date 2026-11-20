# T736 findings — EGL-dispatch probe via Option A (/sysimg mount + LD_LIBRARY_PATH)

**Verdict: PASS**

- Track: T736 (sysimg mount). Lane agent `2e7308b1-db96-44b1-9eae-4ace95dfef86`, launched 00:54 AEDT Sat 21 Nov 2026, wedge 03:54 AEDT 21 Nov.
- Brief: `hidden_files/briefs/track736-sysimg-mount.md`, SHA `f806a630` (37,429 B), brief-gate READY.
- Flight: `service-flight-11`, 01:13:13–01:13:28 AEDT 21 Nov (14:13:13–14:13:28Z). Host 15.02 s, guest 11.251 s, QEMU exit 0.
- Findings written 01:38 AEDT 21 Nov. Branch: INVESTIGATION-ONLY (recorded at Step 2; no fix built).

## 1. The dispatch answer (the question, answered)

**Dispatch FORWARDS. The call DOES reach the driver stub at `0x5cb0`.**

- SP1 (direct driver call): `load_stub("/vendor/lib64/egl/libGLES_swiftshader.so")` rc=0, base=0x7d929e9000, size=52416; `create=0x7d929eec84` (base+0x5c84), `dup=0x7d929eecb0` (base+**0x5cb0** — the stub, resolved by name from the driver's `.dynsym`); `create sync=0x5001`; `dup rc=-1`. The stub function was reached and called; it returned -1. (UART L372–L377.)
- SP2 (META-EGL): `eglGetDisplay=0x3141`; `eglInitialize ok=1 major=1 minor=4 error=0x3000`; `create sync=0x5001 error=0x3000`; `getprocaddress dup=0x7fad02d7a8`; `map addr=0x7fad02d7a8 start=0x7fad024000 path=/system/lib64/libEGL.so`; `dup rc=-1 error=0x3000`. The META-EGL resolved the function and called it; it returned -1. The META-EGL loaded the driver during `eglInitialize` (L379: `libEGL D ... loaded /vendor/lib64/egl/libGLES_swiftshader.so`). (UART L380–L388.)
- SP3 (sw_sync control): `open timeline fd=-13` (EACCES, errno 13); `SP3 DONE control_fd=-999`. The uid-1000 context cannot open the sw_sync timeline. (UART L370–L371.)

**Where it stops, and the fix point.** The stub at `0x5cb0` is reached, and it stops inside the stub's own sw_sync trampoline — not in dispatch:

- Disassembly of the FLOWN driver (`libGLES_swiftshader.so`, SHA `34b79da2`, 52,416 B, extracted from the flown vendor image `5e0607ad` via box debugfs):
  - `eglDupNativeFenceFDANDROID` at `0x5cb0`: `b 56dc` (unconditional branch to the trampoline; the `ret` at `0x5cb4` is unreachable).
  - The trampoline at `0x56dc` opens `/sys/kernel/debug/sync/sw_sync` (openat, `O_CLOEXEC`, path bytes visible at `0x57a8`), then `SW_SYNC_IOC_CREATE_FENCE` (`0xc0285700`) + `SW_SYNC_IOC_INC` (`0x40045701`), returning the fence fd — or `-1` if any step fails (`mov w0, #-1; ret` at `0x57a0`).
  - `eglCreateSyncKHR` at `0x5c84`: `mov w0, #0x5001; ret` — a constant. That is why both SP1 and SP2 report `create sync=0x5001`: no real sync object is created at create time; the real fence is only minted at dup time.
- The dup returns -1 because the trampoline's FIRST step — opening the timeline — fails with EACCES in the uid-1000 context. SP3 proves exactly that open fails with the same errno (-13) on the same path, from the same uid-1000 context. The T720 probe, running as root, opens the same interface fine (fd=4, CREATE_FENCE rc=0, INC rc=0, DONE status=PASS, L198–L226) — so the timeline itself works; the uid-1000 context is what is denied.
- **T722's zero effect, mechanism named:** T722's dup returned -1 (zero effect) because the stub's trampoline cannot open the sw_sync timeline from the surfaceflinger (uid 1000) context. It was never a dispatch failure. Dispatch is EXONERATED.
- **Fix point (not dispatch):** the sw_sync timeline access for the fence-minting context. The stub needs the timeline opened — e.g. opened as root and the fd handed down, the debugfs file's permissions relaxed, or the fence mint run in a privileged context. No dispatch fix is owed; no META-EGL wrapper / function-table patch is owed. The dispatch-fix disposition: NOT APPLICABLE — dispatch forwards.
- **libEGL identity caveat, closed by disassembly:** SP2's resolved function lives in `/system/lib64/libEGL.so` — the RAMDISK's libEGL (`a9641828`, 290,104 B), not the image's (`fed36a9b`, 290,120 B); `LD_LIBRARY_PATH=/sysimg/lib64` did not cause the linker to load the image's file (mechanism unexplained — see §6). This does NOT change the answer: `eglDupNativeFenceFDANDROID` (0x297a8, 56 B) is BYTE-IDENTICAL in the two files (`diff` clean), and `eglGetProcAddress` (0x28b5c, 96 B) is logic-identical — the only differences are two data-address constants shifted by the 16-byte file-size difference. Both are the same META-EGL dispatch (table load + tail-call `br x2`). SP2's answer transfers to the image's libEGL.

## 2. WALL-TYPE-TAG

**WALL-TYPE-TAG: guest — INIT-PROC-MOUNT-CONFLICT (NEW in T736).**

- The early-ramdisk script mounted `/proc` (hook `[T736] mount proc (/proc) rc=0`, L352). First-stage init then fails its own proc mount and aborts:
  - L435: `init: mount("proc", "/proc", "proc", 0, "hidepid=2,gid=...") failed Device or resource busy`
  - L436: `init: Init encountered errors starting first stage, aborting`
  - L437: `init: InitFatalReboot: signal 6`
  - L446: `reboot: Restarting system with command 'bootloader'`
- First failing link: init's `mount("proc", ...)` EBUSY at L435. Root cause: the script's `mount proc (/proc) rc=0` at L352 — an already-mounted plain `/proc` makes init's hidepid=2 mount fail, and first-stage init treats that as fatal.
- This is a T735-script regression: the `/proc` mount was added in the T735 early-ramdisk block; in T735 it FAILED (rc=127, toybox dead) so it never took effect; in T736 it SUCCEEDED (rc=0) and broke init. The T734 script did not mount `/proc` (flights 8/9 booted cleanly).
- **Rust par bhi lagu: haan** — any emulator whose ramdisk pre-mounts `/proc` before handing off to Android first-stage init will hit the same abort: init mounts `/proc` itself with `hidepid=2` and treats an existing mount as fatal. The fix (for the Rust ramdisk and the successor lane alike) is: do not pre-mount `/proc`, or mount it for the probe and unmount it before the handoff.
- Effect on this lane: the guest stopped at 11.251 s. The probe evidence is COMPLETE and unaffected (the probe ran and finished rc=0, L391, before the handoff at L413). Every post-probe hold is VOID, not failed (§4).

## 3. Option A verdict — the T735 refutation is answered

- `[T736] mount /sysimg (vda) rc=0` (L345). `[T736] INTERP tripwire: toybox alive after /sysimg mount`, rc=0 (L347–L348) — the tripwire PASSED; the ramdisk's real linker64 stayed live. `[T736] mount /vendor (loop1) rc=0` (L350), `mount debugfs rc=0` (L351), `mount proc rc=0` (L352). `t723-probe finished rc=0` (L391). Zero `inaccessible or not found` lines, zero ELOOP lines.
- Option A works: mounting the system image at a scratch point keeps the linker alive (T735's RAMDISK-MOUNT-BREAKS-LINKER does not occur), the probe execs, and all three sub-probes answer. The only defect in the T736 script is the `/proc` mount (§2).

## 4. Counts (driver recounts — bytes, split on `\n`, literal patterns; analyzer agrees)

| Pattern | Count | Lines |
|---|---|---|
| T723_PROBE lines | 27 | — |
| T723_PROBE: start | 2 | 355, 356 |
| T723_PROBE DONE | 1 | 389 |
| SP1 DONE | 1 | 377 |
| SP2 DONE | 1 | 388 |
| SP3 DONE | 1 | 371 |
| T734_KERNEL_OK (exec-path control) | 1 | 194 (guest 4.702 s) |
| [T736] hook lines | 8 | 345, 347, 348, 350, 351, 352, 391, 413 |
| T720_PROBE DONE status=PASS | 1 | 226 |
| sys.boot_completed | 0 | VOID — init aborted at 11.25 s; system_server never forked |
| boot_completed | 0 | VOID |
| T733_PROBE_OK | 0 | VOID |
| T728_PROBE_OK | 0 | VOID |
| T721 fence-log (I/T721) | 0 | VOID |
| simplefb: mapped | 0 | VOID |
| Kernel panic / Oops / BUG: | 0 / 0 / 0 | — |
| FATAL EXCEPTION | 0 | — |
| InitFatalReboot | 2 | 437, 438 |
| Device or resource busy | 1 | 435 |
| inaccessible or not found | 0 | — |
| ELOOP | 0 | — |

- UART: 446 lines, 32,545 bytes, SHA `d46e2ae1c6b71ebd300a35d3ce5a2ee7d9cb710bbdf6f73db75d66c331ff511e` (matches flight-end.json).
- Guest kernel time: first 0.0 s, last 11.251232 s, max 11.251232 s.
- Dup-fail rate: 0.000 per 1000 s — VOID (no presenting time; guest died at 11.25 s before surfaceflinger presented). Not a pass, not a fail.
- **parser verdict (analyzer `scripts/analyze_t736_probe.py`, SHA `ad77e7a4`):** exit 0, `=== REPORT COMPLETE ===`; the parser's counts agree with the independent recounts above on every line (T723_PROBE 27, SP1/SP2/SP3 DONE 1 each, T734_KERNEL_OK 1, sys.boot_completed 0, panic 0). The parser names the flight's outcome explicitly: probe lines present, boot holds void on the 11.25 s abort.
- `system_server` fork state: NEVER FORKED — the guest died in first-stage init (L435–L446), before second-stage init could fork system_server. Reported explicitly per the brief's proof requirement (5), with the literal string `sys.boot_completed` above (count 0, VOID).

## 5. Flown stack (all SHAs verified)

- Kernel Image `70d3859d` (20,396,544 B). QEMU `8e5c46dc` (31,498,864 B, 11.1.2/11.2).
- CPIO `t736-fix.cpio.gz` `da6186e5` (53,745,730 B, 241 entries). DTB `t736-modified.dtb` `bc44f717` (2,049 B; initrd-end `0x113341842` = start + 53,745,730).
- System image `af40403a` (2,059,816,960 B). Vendor image `5e0607ad` (134,217,728 B).
- Early-ramdisk `a467a546` (13,082 B). Supervisor `a4db25c0`. Analyzer `ad77e7a4`. Rebuilder `800de584`. DTB patcher `1f896a01`.
- Probe `t723-probe` `4b275e48` (68,792 B). t734probe `b7440e1c` (1,640 B).
- Flight evidence `flights/service-flight-11/`: `uart.log` `d46e2ae1`, `analysis-t736.txt` `85738cfd`, `flight-end.json` `de6d0a7a`, plus `preflight.json`, `qemu-command.txt`, `qemu-stderr.log` (0 B), `supervisor.log`.
- Disk gate at launch: 16,610,376 KiB free (93%) — OPEN. (Gate: 9,769,216 KiB.)

## 6. Honest gaps (recorded, not hidden)

1. **LD_LIBRARY_PATH did not load the image's libEGL.** The script execs `LD_LIBRARY_PATH=/sysimg/lib64 /t723-probe`; the probe has NO DT_RPATH/DT_RUNPATH (readelf: only `NEEDED libEGL.so`); yet the linker loaded `/system/lib64/libEGL.so` (ramdisk's) per the SP2 maps line. The mechanism is UNEXPLAINED from this flight's evidence. It does not change the verdict (§1: the dispatch code is byte-identical in both files), but the successor should determine why — e.g. test `export` vs prefix assignment, or swap the image's libEGL into the CPIO — before relying on LD_LIBRARY_PATH in a later lane.
2. **The full 850 s flight did not run.** The guest aborted at 11.25 s on the proc-mount wall (§2). All post-probe holds are VOID. The successor must fix the script (§7) before the dup-fail differential vs the T721/T722 baselines can be measured.
3. **The `0x56dc` trampoline's label.** objdump labels `0x56dc` as `soft_egl_destroy_surface`, but the code there is the sw_sync open + CREATE_FENCE + INC sequence (path bytes `/sys/kernel/debug/sync/sw_sync` at `0x57a8`, ioctls `0xc0285700`/`0x40045701`) — it is the fence-mint trampoline the `0x5cb0` stub branches to, not a destroy-surface implementation. The label is a symbol-table artefact; the code is what is cited here.

## 7. Successor direction (T737)

1. **Fix the script first:** drop the `/proc` mount from the early-ramdisk — or mount `/proc`, run the probe, then unmount it before handing off to `/init` (the probe's maps_lookup is the only `/proc` consumer). This unblocks the full 850 s flight.
2. **Resolve the LD_LIBRARY_PATH mechanism** (§6.1) so the image's libEGL is the one actually loaded — or CPIO-swap the image's libEGL in — then re-fly the probe to confirm SP2 against the image's file directly.
3. **The fence-mint question (the real remaining work):** with a full boot, confirm the dup -1 persists in the surfaceflinger context, then test a fix route for the sw_sync timeline access (open as root and hand the fd down, or relax the debugfs file's permissions). The fix point is the timeline access, not dispatch.
4. **Do NOT re-run the T734 bisect** (void). **Do NOT mount onto /system** (refuted, T735). **Do NOT use an init .rc trigger/service** (refuted, T733/T734).

## 8. Reflection (REFLECT-ON-EVERY-PROBLEM)

- **What happened:** the probe ran first try and answered the dispatch question completely (SP1+SP2+SP3, all DONE, all guest-origin), and the flight then died at 11.25 s on a script regression the lane itself introduced one step earlier.
- **Root cause of the flight abort:** the T735 early-ramdisk block mounted `/proc`, and the T736 lane carried that line forward without asking whether first-stage init tolerates a pre-mounted `/proc`. It does not — init's hidepid=2 mount fails EBUSY and init aborts. The T734 script (which booted cleanly) never mounted `/proc`; the difference was visible in the script diff and was not checked against init's first-stage mount list.
- **Missed signal:** the T735 findings recorded `mount proc FAILED rc=127` as a linker casualty and moved on; nobody asked what would happen if that mount SUCCEEDED. The success case was the dangerous one. A mount that fails harmlessly in one lane can kill the next lane when the underlying fault is fixed.
- **What changes:** (a) every early-ramdisk mount added for a probe gets a one-line justification naming which guest consumer needs it AND whether init re-mounts the same point (the `/proc` mount's only consumer was the probe's maps_lookup — it should have been scoped to the probe and unmounted before handoff); (b) the successor brief carries this as an explicit step. Where it lives: this section + the T737 brief's Step 2.
- **What went right:** the INTERP tripwire (T735 lesson) worked exactly as designed — toybox exec'd after the mount, proving the linker alive before the probe ran. Option A is validated. The disassembly cross-check caught the cpio-unpack driver copy differing from the flown one (`fafb59a7` vs `34b79da2`) before any conclusion was drawn from it — verify the artifact the guest ACTUALLY consumes, every time.
