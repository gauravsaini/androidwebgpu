# Death note — Track T727: toybox-service discriminator (service-flight)

## Question asked

Does the toybox service (`t727probe`, `/system/bin/toybox sh -c "echo T727_SERVICE_OK
> /dev/kmsg"`, disabled/oneshot/class main/root) run when init starts it from a
`sys.boot_completed=1` property action — i.e. is the T726 service-start failure in
the probe path (a) or in the start leg itself (b)?

## Last observed state

- Full 600 s guest flight COMPLETE (service-flight, launched 23:55:16 AEDT 17 Nov 2026,
  ended 00:05:24 AEDT 18 Nov; guest 604.944 s, host 606.19 s, QEMU exit 0; UART
  27,482 lines, SHA `8b1dbb2e`). Findings: `findings-t727.md` (SHA `edab179b`,
  14,433 B). Outcome: FAIL (`outcome-t727.txt`, 5 B).
- Trigger FIRED: `sys.boot_completed` 9 lines; first processing guest 503.881 s
  (hw/init.rc:1224). Action marker `T727_PROBE_OK` x2 (guest 101.885 s on-boot,
  509.652 s property action, post-flood). Registration re-proven.
- The service DID start: `init: Service 't727probe' (pid 3992) exited with status
  127 oneshot service took 2.466000 seconds in background` (L21969, guest
  511.560 s). Start leg (b) in its no-op form is REFUTED.
- The payload FAILED: `T727_SERVICE_OK` 0; no `starting service 't727probe'` line
  (logging gap — 31 starting-service lines exist for other services); exit 127
  unique to t727probe; no observable stderr. New wall SERVICE-PAYLOAD-EXIT-127
  (guest). The 127 mechanism is UNPROVEN (prime suspect: init's argv handoff of
  the quoted `sh -c` string; the 2.466 s runtime is unexplained).
- Probe-path question (a) UNANSWERED: no payload has produced a marker from a
  service context. EGL dispatch (R1/R3/R4) stays open.
- Holds intact: T720 DONE PASS 1, T721 fence 1, Invalid present fence 0,
  REJECT 0, EPERM 0, simplefb mapped 1, Watchdog KILLING 0, panic/Oops/BUG 0,
  dup-fail 4 (6.612/1000 s, T721 revert baseline 6.660).

## Open items (for the successor, T728)

1. Identify the exit-127 mechanism: observable-payload service that dumps its
   received argv + exit path to a file under /data (or a static helper binary
   instead of `sh -c`), so the fault is proven, not guessed.
2. If the quotes are the fault, replace the `sh -c "..."` form with a direct exec
   and re-fly; then re-fly the T723 probe to answer the probe-path question (a).
3. Adjudication lesson carried forward: judge "started" from the service exit
   line, not the "starting service" line; any future payload must be observable.
