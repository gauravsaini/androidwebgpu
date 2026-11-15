# DEATH NOTE — Track T716: composer /dev/mem EPERM + first present dispatch

Lane verdict: FAIL (honest). The mapping worked; the display wall stands
(moved downstream).

## What died

T716 set out to break the composer /dev/mem EPERM wall that T715 isolated
(5 EPERM lines, queueBuffer 0, present 0), then to fly the full 2400 s
window for the first queueBuffer + first present dispatch. The first half
succeeded completely: the EPERM is diagnosed, fixed, and proven fixed in
the guest UART. The second half failed: the guest boots, system_server
runs, the composer maps the framebuffer 7 times — but no frame is ever
presented, because no buffer is ever allocated.

## The wall (first failing link)

Two linked failures, both pre-existing in T715, both upstream of any
present:

1. Flight-01 L10878 (guest ~104 s): `E/HWComposer: getAttribute:
   getDisplayAttribute failed for display 0: Unsupported (8)` — the
   composer HAL refuses a display-attribute query (8 occurrences).
2. Flight-01 L10891: `E/BufferQueueProducer: [FramebufferSurface]
   allocateBuffers: failed to allocate buffer (0 x 0, format 1, usage
   0x300)` — 42 `allocateBuffers` lines in the flight, ALL failures,
   ALL with a 0x0 size. No buffer is ever successfully allocated.

Downstream: queueBuffer 0, presentDisplay 0, 742 `Invalid present fence`
lines (first L22854, 0.2 s after the first mapping), and 1 genuine
Launcher3 FATAL `Couldn't removeRegionSamplingListener` (L81124).
Everything upstream of the allocator works (SurfaceFlinger ready L10578,
display device added 1280x720 L12061, Launcher3 dexopted
status=PERFORMED L13593, composer maps the framebuffer L22650).

## What the fix proved (this lane's durable result)

- EPERM root cause proven from evidence: the composer service runs as
  root with capabilities SYS_NICE ONLY (vendor `init.virt.rc` stanza,
  UART-proven effective) — missing CAP_SYS_RAWIO, so `mem_open()`
  returns EPERM. Not DAC, not SELinux-as-blocker (denials exist but
  carry `permissive=1` — premise contradiction recorded + re-derived),
  not STRICT_DEVMEM (mmap of non-RAM MMIO allowed; proven empirically
  by the successful mapping).
- Smallest-route fix shipped: `SYS_RAWIO` added to the capabilities in
  BOTH vendor.img composer definitions, via CPIO member replacement
  (the guest mounts the EMBEDDED vendor.img) + DTB `linux,initrd-end`
  re-align byte-exact. New artifacts: vendor.img `9a2519b1…`, CPIO
  `08eb665b…`, DTB `cbb25094…`. System.img untouched; installd.rc
  verified byte-identical (`3ea3466b…`, 4,958 B, line 109).
- Proof in the guest UART (not just the built image): extended smoke
  PASS — 0 EPERM + L22814 `simplefb: mapped 0x5f000000 1280x720
  stride=1280 bpp=4`. Flight-01: 0 EPERM, 7 mappings.
- The 0x0 allocation wall is INDEPENDENT of the EPERM wall: T715 shows
  the same shapes (54 allocation failures, 5 attribute failures). T716
  broke the second wall; the first wall is the next lane.

## The parallel wall

system_server died 8 times (9 lifetimes), 0 by the Watchdog. First
death's cause, proven by the crash lines (L23309-23310): NPE in
LockSettingsService$DeviceProvisionedObserver.reportDeviceSetupComplete
— IGateKeeperService is null (no GateKeeper HAL in this virt stack),
LockSettingsService.java:3394. Each new system_server re-hits setup
completion and dies again (INFERENCE for the later deaths; only the
first crash stack was read in full). 6 DeadSystemException FATALs are
collateral of the first death. The GateKeeper stub/guard is a separate
lane's work — NOT folded into the next display lane.

## What is owed (exact next fix)

1. T717 — the 0x0 buffer-allocation wall: (a) identify WHICH display
   attribute the T681 simplefb composer backend refuses in
   getDisplayAttribute (backend source / ELF; the UART line does not
   name it); (b) trace why BufferQueueProducer receives 0x0 dimensions
   (composer size-query refusal vs independent allocator failure —
   distinguish from evidence); (c) smallest fix in the composer backend
   (T708 ELF route) and/or size propagation; prove with a nonzero
   allocation + queueBuffer > 0 + first present dispatch. Base: the
   T716 stack (do NOT regress the SYS_RAWIO capabilities). Oracle:
   T716 flight-01 UART L10878/L10891.
2. GateKeeper stub/guard so system_server survives setup completion;
   verify with 1 system_server lifetime past boot_completed.
3. Vectors 2 and 3 from T715 are NOT needed. Do not spend a lane on
   them.

## Evidence

- findings-t716.md (Verdict: FAIL) — full evidence + counts.
- flights/smoke/uart.log SHA d224de95… (13,664 lines) — INCONCLUSIVE.
- flights/smoke-ext/uart.log SHA 2246dca5… (24,291 lines) — PASS.
- flights/flight-01/uart.log SHA b178e4c6… (219,931 lines).
- flights/flight-ab/uart.log SHA ee9f6284… (576 lines).
- guest-inputs/t716-vendor.img SHA 9a2519b1…, t716-fix.cpio.gz SHA
  08eb665b…, t716-modified.dtb SHA cbb25094… (installd trigger
  untouched: t715-system.img SHA 1283cbf2…).
- No commit by the lane agent. Certification, commit, push and vault
  are the driver's.
