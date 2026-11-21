# TIMELINE-T746 — service-flight-24 (guest clock, kernel [t] interpolation)

Flight: service-flight-24. Launched 2026-11-21T08:47:55Z (19:47:55
AEDT), ended 09:12:58Z (20:12:58 AEDT). Guest 1501.937 s; host 1503.0
s; QEMU exit 0. UART 94,908 lines / 12,990,410 B, SHA 8a554c38.

## Boot

- guest ~0–100 s — kernel boot, init starts services. Zygote 506
  starts (ZygoteInit START L2072, wall 01-01 00:01:00.082).
- guest ~100 s — system_server 670 created by 506 (L3694, wall
  01-01 00:01:51.230).
- guest 502.087 s — FIRST sys.boot_completed firing (11 lines total).
- guest 538.48–538.80 s — zygote onrestart block runs after 670's
  death (L16320–L16370: vdc abort_fuse, power/state FAILED,
  restart cameraserver).

## Death 1 — system_server 670, SIGKILL, killer UNIDENTIFIED

- guest 554.501 s — 670 DIES (death report + Exit-zygote L17414,
  logged by 506). Last 670 line L17260 (wall 23:25:43.862).
- wall 23:25:46.652–23:25:47.060 — children self-kill via the Android
  API (1274, 1606, 1904, 1934) after 670's death.
- guest ~554.5–560 s — new zygote 2009 starts (ZygoteInit START
  L17606, wall 23:25:51.330); 2009 creates system_server 2143
  (L18621, wall 23:26:58.035).

## The trigger builds (2143)

- wall 23:28:47–23:30:15 — RenderThread 0x8893 deaths (settings +
  launcher3; 12 lines in the flight, e.g. L27310, L30121, L33044,
  L38583).
- guest 1090.410 s — EVENT (4th zygote-signal-9 instance): zygote
  2009 received signal 9 (L61549). Group-kill L61550, cgroup-kill
  L61562. Onrestart block L62071–L62090 (guest 1093.35–1093.57 s).
  2009 keeps forking: 4374 @ L61618 through 5082 @ L64994 (17
  children). Sender UNNAMED.
- guest 1111.406 s — 2143's binder thread faults: L64161, `Fatal
  signal 11 (SIGSEGV), fault addr 0xd1008403 in tid 2178
  (binder:2143_3)`, wall 23:34:15.711. Preceded ~6.5 s earlier by the
  connectionToken NPE L64005 (wall 23:34:09.207, pid 2143).
- guest 1124.395 s — 2143 DIES (death L65764, logged by 2009). Last
  2143 line L65118 (silence gap 646 lines). Dying window 13.0 s
  (1603 lines); 26 tombstoned lines, 3 intercepts for pid 2143.
- wall 23:34:59.351 — 2525 self-kills via the Android API (L65753),
  just before 2143's death.
- wall 23:35:05.774 — new zygote 5170 starts (ZygoteInit START
  L65883); creates system_server 5312 (L67517, wall 23:36:12.352).
  5312 survives to flight end.
- wall 23:39:50.184 — connectionToken NPE in the surviving
  system_server 5312 (L89068). No SIGSEGV follows it in this flight.

## Flight end

- guest 1501.937 s — guest target reached (1501.9 s >= 1500 s).
  QEMU exit 0. 5312 alive at flight end. Dup-fail 0 in 999.8 s
  presenting.

## Cross-flight pattern (the ~1090 s neighbourhood)

- flight-22: zygote 537 event @ guest 1090.914 s; 731 dies 1145.270 s.
- flight-23: zygote 505 event @ 504.272 s; 667 dies 518.219 s.
  Zygote 2001 event @ 952.368 s; 2118 dies 971.469 s.
- flight-24: zygote 2009 event @ 1090.410 s; 2143 dies 1124.395 s.
- Two events in the same guest-time neighbourhood (~1090 s) across
  flights, 0.5 s apart. Pattern without a mechanism — the next track
  should hunt WHAT happens at guest ~1090 s.
