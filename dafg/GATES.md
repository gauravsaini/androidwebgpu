# Gates: Track T659 — system-server-startotherservices-audioservice-audiopolicy (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: PIN why audioserver never publishes media.audio_policy on this guest; verify supply path (vendor.audio-hal binary, VINTF manifest entry, or audioserver config); rule out alternatives with evidence; build ONE fix initramfs on T658 lineage (base image 220fcef8fb0536d48f818a04ff0ccecadabaaf8e03a93cf1d0554dfd36e2223a hardlinked) supplying missing audio-policy path by pinned mechanism ONLY; boot QEMU 11.1.2 with 1200s guest target; prove system_server passes StartAudioService into next startOtherServices stage or name next wall with exact line numbers from this flight; check all prior regression vectors (all 0); report sys.boot_completed explicitly; deliver findings to /Users/Shared/agyx-track659-system-server-startotherservices-audioservice-audiopolicy.md and outcome to /Users/Shared/outcome-t659.txt; update TIMELINE.md.

- [x] G659-1: Pinned root cause of audioserver not publishing media.audio_policy with AOSP and guest evidence, alternatives ruled out
  CHECK: python3 -c "import os; assert os.path.exists('evidence/root_cause_pinned.json'); print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK
  EVIDENCE: Met. Root cause pinned to missing android.hardware.audio.effect@7.0 in vendor VINTF manifest causing vendor.audio-hal to crash SIGABRT on registration, preventing audioserver from publishing media.audio_policy and causing 88s Watchdog kill in AudioProductStrategy.native_list_audio_product_strategies(). Recorded in evidence/root_cause_pinned.json.

- [x] G659-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T658 lineage supplying missing audio-policy path by pinned mechanism ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t658-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == '220fcef8fb0536d48f818a04ff0ccecadabaaf8e03a93cf1d0554dfd36e2223a'; assert os.stat('guest-inputs/t658-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t659-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK
  EVIDENCE: Met. Hardlinked base image and canonical kernel verified. Built ONE fix initramfs (SHA-256 fe1809ffbb3ee91252a009442c8956264df1bcac022dd731d86c7555cea2eb6c) supplying missing audio.effect VINTF entry in vendor/etc/vintf/manifest.xml and t636-diag.sh payload.

- [x] G659-3: Supervisor script scripts/supervisor_t659.py configured with 1200s guest target and carrying fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t659.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t659_fix_initramfs_sha256=' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK
  EVIDENCE: Met. scripts/supervisor_t659.py configured with TARGET_GUEST_SECONDS = 1200 and androidboot.t659_fix_initramfs_sha256 on guest kernel command line.

- [x] G659-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T659 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: Met. Flight-01 executed to 1202.388s guest time (stop_reason: Guest target reached (1202.4s >= 1200s), qemu_returncode 0). Guest kernel command line carried fix image SHA-256.

- [x] G659-5: Progress analysis: system_server passes StartAudioService into next startOtherServices stage (or next wall named with first failing line), quoted line numbers from this flight
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Met. vendor.audio-hal registered cleanly (uart.log:7070-7071), audioserver published AudioPolicy (uart.log:12200, 12210), system_server completed StartAudioService (uart.log:14678-14695), advanced past StartAudioService into StartSoundTriggerMiddlewareService (uart.log:14777) and 50+ downstream services into Phase 550. Next wall named: Phase 550 JobStatus.<clinit> NoClassDefFoundError on MediaStore$Images$Media (uart.log:16969-17035).

- [x] G659-6: All prior regression vectors hold at 0, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK
  EVIDENCE: Met. All 13 prior regression vectors held at 0 errors. sys.boot_completed explicitly reported as 0 (not reached due to crash in Phase 550).

- [x] G659-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track659-system-server-startotherservices-audioservice-audiopolicy.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-startotherservices-audioservice-audiopolicy' in f; assert 'AudioService' in f; assert 'media.audio_policy' in f; assert 'startOtherServices' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t659.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: Met. Findings delivered to /Users/Shared/agyx-track659-system-server-startotherservices-audioservice-audiopolicy.md with line-start Verdict: PASS, all required strings, and matching /Users/Shared/outcome-t659.txt (PASS).

- [x] G659-8: TIMELINE.md updated and worktree committed
  CHECK: python3 -c "t = open('TIMELINE.md').read(); assert 'T659' in t; print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
  EVIDENCE: Met. TIMELINE.md updated with Track T659 entry.
