# DEATH NOTE — Track T715: installd early-boot trigger

Lane verdict: FAIL (honest). The trigger worked; the display wall stands.

## What died

T715 set out to break the SensorService.onBootPhase wall that killed T712,
T713 and T714 by triggering installd during early init, then to fly the
full 2400 s window for the first queueBuffer + first present dispatch.
The first half succeeded completely. The second half failed: the guest
boots, system_server runs, Launcher3 starts — but no frame is ever
presented, because the composer service cannot open the framebuffer.

## The wall (first failing link)

`android.hardware.graphics.composer@2.1-service: simplefb: open /dev/mem
failed: Operation not permitted` — flight-01 L23338, repeated L61491.
The composer backend cannot map the simplefb framebuffer, so
SurfaceFlinger never dispatches a present: queueBuffer 0, presentDisplay
0, 929 `Invalid present fence` lines downstream. Everything upstream of
the composer works (SurfaceFlinger ready L10526, display device added
1280x720 L12019, Launcher3 started + dexopted status=PERFORMED L13561).

## What the trigger proved (this lane's durable result)

- installd starts in early-boot: flight-01 L9319-9320 (guest 88.32 s),
  firing up PID 2429 (L9676). Smoke: L9355-9356 (guest 94.20 s).
- `installd not found`: 0 in both flights (T714: 2,750).
- fixAppsDataOnBoot completes (0 stack traces, installd servicing
  fix-ups, thread pool unblocks).
- SensorService.onBootPhase RETURNS: smoke L13297 (7231 ms), flight-01
  L13333 (6456 ms). Watchdog kills: 0 (T714: 10).
- sys.boot_completed=1: init processes boot_completed actions from guest
  505.43 s (L21346).
- Vector 2 (thread-pool patch) and Vector 3 (onBootPhase decoupling)
  are NOT needed. Do not spend a lane on them.

## The parallel wall

system_server died 5 times (6 lifetimes), 0 by the Watchdog. First
death's cause, proven by the crash file (L23926-23973): NPE in
LockSettingsService$DeviceProvisionedObserver.reportDeviceSetupComplete
— IGateKeeperService is null (no GateKeeper HAL in this virt stack).
Each new system_server re-hits setup completion and dies again
(INFERENCE for the later deaths; only the first crash file was read).

## What is owed (exact next fix)

1. Composer framebuffer access: diagnose why open("/dev/mem") returns
   EPERM for the composer service (user, node permissions, STRICT_DEVMEM
   — SELinux is permissive, so not an AVC denial) and grant the mapping
   by the smallest route (service user, ueventd rule, or fb0-based
   mapping). Oracle: first present dispatch + queueBuffer > 0.
2. GateKeeper stub/guard so system_server survives setup completion;
   verify with 1 system_server lifetime past boot_completed.

## Evidence

- findings-t715.md (Verdict: FAIL) — full evidence + counts.
- flights/smoke/uart.log SHA 0e5221ab… (13,687 lines).
- flights/flight-01/uart.log SHA 8d69ee3b… (244,911 lines).
- flights/flight-ab/uart.log SHA abaf26bc… (575 lines).
- guest-inputs/t715-system.img SHA 1283cbf2… (installd.rc + start
  installd in on early-boot; initramfs + DTB unchanged).
- No commit by the lane agent. Certification, commit, push and vault
  are the driver's.
