# Gates: Track T709 — Enable logcat streamer in early-ramdisk + cure system_server early termination + fly for the first queued buffer + first present dispatch (Mac agyx)

OWNS: guest-inputs/**, tools/**, scripts/**, flights/**, GATES.md, TIMELINE.md, heartbeat.json, progress-t709.txt, findings-t709.md, outcome-t709.txt, DEATH_NOTE-T709.md

Scope: Enable the logcat streamer in early-ramdisk so system_server's Java stack traces stream to the UART console, prove logcat visibility in smoke flight, diagnose the system_server early termination root cause, apply the cure in initramfs if feasible (or document the exact exception with UART evidence), run full 2400s flight and 240s canonical control on Mac QEMU 11.1.2, and deliver complete findings and TIMELINE.md.

- [x] G709-1: Step 0 Disk gate (Mac df >= 8 GiB)
  CHECK: python3 -c "import os; s = os.statvfs('/System/Volumes/Data'); free = (s.f_bavail * s.f_frsize) / (1024**3); assert free >= 8.0, f'Low disk: {free:.2f} GiB'; print(f'DISK_GATE_OK: {free:.2f} GiB available')"
  EXPECT: DISK_GATE_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; path=de7de1880035/13 entries; output=DISK_GATE_OK: 11.07 GiB available

- [x] G709-2: Step 1 Verify all 11 inputs with SHA-256 (no network fetch)
  CHECK: python3 scripts/verify_inputs_t709.py
  EXPECT: INPUTS_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; path=de7de1880035/13 entries; output=VERIFIED: canonical-Image (23073280 bytes, sha256: 05c4391767a6ff44...) | INPUTS_VERIFIED_OK

- [x] G709-3: Step 2 Primary fix: Initramfs repacked with logcat streamer enabled in early-ramdisk and DTB re-aligned byte-exactly
  CHECK: python3 scripts/verify_build_t709.py
  EXPECT: BUILD_T709_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; path=de7de1880035/13 entries; output=VERIFIED: DTB linux,initrd-end matches byte-exactly: <0x01 0x132b598c> | BUILD_T709_VERIFIED_OK

- [x] G709-4: Step 3a Smoke flight (120 s guest): prove /dev/vda mounted as /system, logcat streamer visible on UART console, and identify system_server termination status
  CHECK: python3 scripts/verify_smoke_t709.py
  EXPECT: SMOKE_FLIGHT_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; path=de7de1880035/13 entries; output=Found [T709-STREAMER] in UART! | SMOKE_FLIGHT_VERIFIED_OK

- [x] G709-5: Step 2/3 Secondary fix: Apply cure based on observed exception if possible (or document diagnosis/oracle) and verify updated initramfs/DTB
  CHECK: python3 scripts/verify_secondary_fix_t709.py
  EXPECT: SECONDARY_FIX_EVALUATED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; output=VERIFIED: early-ramdisk contains T709 SettingsProvider directory pre-creation patch | SECONDARY_FIX_EVALUATED_OK

- [x] G709-6: Step 3b Flight-ab (canonical kernel control flight, 240 s guest): prove vda polling timeout and reboot under canonical kernel
  CHECK: python3 scripts/verify_flight_ab_t709.py
  EXPECT: FLIGHT_AB_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; output=VERIFIED: Flight-ab canonical kernel control proves CONFIG_VIRTIO_BLK=y requirement | FLIGHT_AB_VERIFIED_OK

- [x] G709-7: Step 3b Flight-01 (2400 s guest): execute full window flight and preserve UART/evidence
  CHECK: python3 scripts/verify_flight_01_t709.py
  EXPECT: FLIGHT_01_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; output=VERIFIED: Flight-01 completed with host=2417.94s, guest=2414.07s | FLIGHT_01_VERIFIED_OK

- [x] G709-8: Step 4 Adjudication & Deliverables: findings-t709.md (literal Verdict: PASS or Verdict: FAIL, WALL-TYPE-TAG, Rust par bhi lagu), outcome-t709.txt, DEATH_NOTE-T709.md, progress-t709.txt, TIMELINE.md updated per protocol, heartbeat.json CLOSED
  CHECK: python3 scripts/verify_deliverables_t709.py
  EXPECT: DELIVERABLES_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track709-logcat-systemserver-cure; output=DELIVERABLES_VERIFIED_OK
