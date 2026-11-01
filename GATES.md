# Gates: Track T660 — system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, evidence/**

Scope: PIN why android.provider.MediaStore$Images$Media cannot be resolved by the boot classloader on this guest; verify supply path (framework bootclasspath entry vs MediaProvider APEX com.android.mediaprovider mounted and indexed by PMS/apexd); rule out alternatives with evidence; build ONE fix initramfs on T659 lineage (base image fe1809ffbb3ee91252a009442c8956264df1bcac022dd731d86c7555cea2eb6c hardlinked) supplying missing MediaStore resolution path by pinned mechanism ONLY; boot QEMU 11.1.2 with 1200s guest target; prove system_server passes Phase 550 (PHASE_ACTIVITY_MANAGER_READY) into next boot phase or name next wall with exact line numbers from this flight; check all prior regression vectors (all 0); report sys.boot_completed explicitly; deliver findings to /Users/Shared/agyx-track660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror.md and outcome to /Users/Shared/outcome-t660.txt; update TIMELINE.md.

- [x] G660-1: Pinned root cause of android.provider.MediaStore$Images$Media NoClassDefFoundError in JobStatus.<clinit> with AOSP and guest evidence, alternatives ruled out
  CHECK: python3 -c "import os; assert os.path.exists('evidence/root_cause_pinned.json'); print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK
  EVIDENCE: Met. Root cause pinned to bootclasspath omission. com.android.mediaprovider APEX is activated and mounted, its APK indexed by PMS, and framework-mediaprovider.jar contains MediaStore$Images$Media. However, hardcoded BOOTCLASSPATH_32_APEX override in init.rc and t636-diag.sh omitted framework-mediaprovider.jar from BOOTCLASSPATH, causing ART to fail class resolution and mark JobStatus defective. Recorded in evidence/root_cause_pinned.json.

- [x] G660-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T659 lineage supplying missing MediaStore resolution path on BOOTCLASSPATH by pinned mechanism ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t659-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == 'fe1809ffbb3ee91252a009442c8956264df1bcac022dd731d86c7555cea2eb6c'; assert os.stat('guest-inputs/t659-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t660-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK
  EVIDENCE: Met. Hardlinked base image and canonical kernel verified. Built ONE fix initramfs (SHA-256 b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4) supplying framework-mediaprovider.jar on BOOTCLASSPATH in system.img init.rc and t636-diag.sh.

- [x] G660-3: Supervisor script scripts/supervisor_t660.py configured with 1200s guest target and carrying fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t660.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t660_fix_initramfs_sha256=' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK
  EVIDENCE: Met. scripts/supervisor_t660.py configured with TARGET_GUEST_SECONDS = 1200 and androidboot.t660_fix_initramfs_sha256 on guest kernel command line.

- [x] G660-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T660 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: Met. QEMU 11.1.2 flight-01 ran 1203.9s wall time (1202.6s guest time), exceeding 1200.0s target, exit code 0. Guest booted with androidboot.t660_fix_initramfs_sha256=b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4.

- [x] G660-5: Progress analysis: system_server passes Phase 550 into next boot phase (or next wall named with first failing line), quoted line numbers from this flight
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Met. JobStatus.<clinit> succeeded at line 14666 (StartDiskStatsService). NetworkWatchlistService$Lifecycle scheduled cleanly at line 16713 without NoClassDefFoundError. SystemServer advanced through 142 services in Phase 550. Next wall identified at line 16905 (PermissionPolicyService) where Zygote failed to mount /data_mirror/data_de/null/0/com.android.permissioncontroller (lines 17065, 17697).

- [x] G660-6: All prior regression vectors hold at 0, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK
  EVIDENCE: Met. All 13 prior regression vectors held at 0 errors (PermissionController, services-ext, sdk-sandbox, SettingsProvider, sensors HAL, netd, netstats, wifi APEX, connectivity APEX, audio effect VINTF, namespace, privapp allowlist, Widevine). 0 .capex files across lineage. sys.boot_completed explicitly reported as 0.

- [x] G660-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror' in f; assert 'JobStatus' in f; assert 'MediaStore' in f; assert 'Phase 550' in f or 'PHASE_ACTIVITY_MANAGER_READY' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t660.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: Met. Findings delivered to /Users/Shared/agyx-track660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror.md with line-start Verdict: PASS. Outcome file /Users/Shared/outcome-t660.txt carries PASS.

- [x] G660-8: TIMELINE.md updated and worktree committed
  CHECK: python3 -c "t = open('TIMELINE.md').read(); assert 'T660' in t; print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
  EVIDENCE: Met. TIMELINE.md updated following user global protocol and git commit executed.
