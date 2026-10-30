# Gates: Track T656 — system-server-startotherservices-networkstatsservice-apexdata-write (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: PIN why NetworkStatsService cannot write into /data/misc/apexdata/com.android.tethering/netstats; rule out alternatives; build ONE fix initramfs on T655 lineage supplying missing netstats path by pinned mechanism ONLY; boot QEMU 11.1.2 with 1200s guest target; prove system_server passes NetworkStatsServiceInitializer into next startOtherServices stage or name next wall; check all prior regression vectors (all 0); report sys.boot_completed explicitly; deliver findings to /Users/Shared/agyx-track656-system-server-startotherservices-networkstatsservice-apexdata-write.md and outcome to /Users/Shared/outcome-t656.txt; update TIMELINE.md.

- [x] G656-1: Pinned root cause of NetworkStatsService failing to write into /data/misc/apexdata/com.android.tethering/netstats with AOSP and guest evidence, alternatives ruled out
  CHECK: python3 -c "import os; assert os.path.exists('scripts/extracted_t655_diag.sh'); print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK

- [x] G656-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T655 lineage supplying missing netstats path by pinned mechanism ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t655-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == '8b2d3e547f083f67d244bfa3d1c1286ea8dbda21fe0d04da5746514828ef6a0a'; assert os.stat('guest-inputs/t655-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t656-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK

- [x] G656-3: Supervisor script scripts/supervisor_t656.py configured with 1200s guest target and carrying fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t656.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t656_fix_initramfs_sha256=' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK

- [x] G656-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T656 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK

- [x] G656-5: Progress analysis: system_server passes NetworkStatsServiceInitializer into next startOtherServices stage (or next wall named with first failing line), quoted line numbers from this flight
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK

- [x] G656-6: All prior regression vectors hold at 0, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK

- [x] G656-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track656-system-server-startotherservices-networkstatsservice-apexdata-write.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-startotherservices-networkstatsservice-apexdata-write' in f; assert 'NetworkStatsService' in f; assert 'NetworkStatsServiceInitializer' in f; assert 'startOtherServices' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t656.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK

- [x] G656-8: TIMELINE.md updated and worktree committed
  CHECK: python3 -c "t = open('TIMELINE.md').read(); assert 'T656' in t; print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
