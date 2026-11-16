# DEATH NOTE — T723 (EGL dispatch probe flight, recovery lane)

## Verdict: FAIL — probe never ran; EGL dispatch question unanswered

- Track T723, recovery lane. First agent exited cleanly on WAIT-DISK 15:22:01 AEDT 16 Nov 2026; recovery launched 16:24:30 AEDT (wedge 19:24:30, not hit).
- Probe flight `probe-flight`: launched 16:27:17 AEDT, ended 16:37:38 AEDT (guest target 620.2 s ≥ 600 s). QEMU exit 0, supervisor clean exit. UART 26,966 lines, SHA `0b8d8841…`.
- **0 `T723_PROBE` lines. 0 DONE. 0 SP1. 0 SP2. 0 SP3.** The probe binary (`4b275e48`, staged and SHA-verified) never executed.

## First failing link

The early-ramdisk wait-loop (`guest-inputs/early-ramdisk-t723`, lines 252–268) depends on `/system/bin/toybox` (for `sleep 2`) and `/system/bin/getprop` (to detect `sys.boot_completed`). It is backgrounded before `exec /init` (line 274), i.e. before the system image is mounted — and first-stage init mounts `/system` in a mount namespace the pre-forked loop never joins. Both commands failed on every iteration (`inaccessible or not found`, 877 such lines in the UART, spanning the whole flight), so the loop burned all 300 iterations in ≈ 30 s of guest time and printed

```
[T723] boot_completed never detected in 600s; probe not run
```

at UART L2018, guest ≈ 33.4 s — **482.7 s before `sys.boot_completed` was set** (guest 516.118 s, UART L21534). The probe was already gone before its trigger event existed.

Corroboration: the T552 control loop in the same script (identical dependencies) returned empty fields in all 48 status polls and never fired a milestone; the T711 logcat streamer never once executed `/system/bin/logcat` (19 restarts, all status 256); box debugfs proves toybox, getprop and logcat all exist in the flown system image (`0ba6d6d3`) — a context defect, not an image defect. The T720 probe worked because it is ramdisk-local and uses no `/system/bin/*`.

## What survives

- Static dispatch chain (probe (a)) stands; runtime survivors R1/R3/R4 still open — the question is unanswered, not answered negatively.
- Holds all intact: T720_PROBE DONE PASS 1, T721 fence log 1 (`fd=10 tl=9`), Invalid present fence 0, REJECT 0, EPERM 0, mapped 1, Watchdog kills 0, `sys.boot_completed` 8, panic/Oops/BUG 0.
- Incidental: guest boots to `boot_completed`; RenderEngine still dup-fails 4× (`0x3000`, rate 6.449/1000 s, between the T721 baselines — context only, not a differential); system_server crashed late (guest ≈ 560 s, restarted as pid 4367 by flight end).
- Artifacts preserved: UART + flight-end + analyzer report (`flights/probe-flight/analysis-t723.txt`), findings, this note, TIMELINE, progress, CLOSED heartbeat. No agent commits.

## Successor direction

Do not reuse a shell wait-loop that depends on `/system/bin/*` from the ramdisk context. Recommended: **init `.rc` service trigger** (`on property:sys.boot_completed=1` execs `/t723probe`) — Android-native, exact event, correct namespace; probe binary stays byte-identical. Fallback: in-probe raw-syscall polling (no shell, no toybox). Reject host-side GDB injection. The successor brief must require the trigger's own detection line as a step pass criterion, and any wait mechanism must print a ≤ 30 s heartbeat so a spin is visible in the first minute of UART.

Wall-type tag: **RAMDISK-MOUNT-BLIND**.
