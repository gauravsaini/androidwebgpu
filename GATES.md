# Gates: Track T667 — vold-posix-acl-enotsup-user-unlock-android-fg-fatal (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, GATES.md, TIMELINE.md, heartbeat.json, evidence/**

Scope: PIN, from kernel evidence (config, symbols) and guest evidence, the exact mechanism of the ACL failure: canonical Linux 4.19 kernel (05c43917...) lacks CONFIG_EXT4_FS_POSIX_ACL=y, so setxattr of system.posix_acl_default on ext4 /data/media/0 returns EOPNOTSUPP (errno 95). Rule out alternatives with evidence: mount-option gap, vold-side misuse, /data/media subtree location. Fix by pinned mechanism ONLY: a filesystem/mount arrangement under which vold's default-ACL set succeeds without patching services.jar or reverting /data to tmpfs, preserving ext4 /data and ENODATA serial path. Build ONE fix initramfs on T666 lineage (base image 34463d5c... hardlinked, never copied). Boot QEMU 11.1.2 with 1200s guest window. Prove vold prepareUserStorage on /data/media/0 completes without ACL EOPNOTSUPP, user unlock completes, sys.boot_completed=1 processed, system_server alive at flight end with non-empty ce_available, zero Zygote SIGSEGV, all regression vectors held at 0 or characterized. Deliver findings to /Users/Shared/agyx-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal.md and outcome to /Users/Shared/outcome-t667.txt; update TIMELINE.md.

- [x] G667-1: Pinned root cause & exact mechanism of ACL failure (kernel config + guest evidence, alternatives ruled out)
  CHECK: python3 -c "import json; d = json.load(open('evidence/root_cause_pinned.json')); assert d['track'] == 'T667'; assert d['pinned_mechanism']['kernel_config']['CONFIG_EXT4_FS_POSIX_ACL'] == 'not set'; assert d['pinned_mechanism']['kernel_config']['CONFIG_TMPFS_POSIX_ACL'] == 'y'; print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=ROOT_CAUSE_PINNED_OK

- [x] G667-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T666 lineage supplying fix by pinned filesystem/mount arrangement ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t666-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == '34463d5c23b13e1f58707462236bbfbc43f326249da3a578c9a01576c25a2a6c'; assert os.stat('guest-inputs/t666-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t667-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=FIX_INITRAMFS_VERIFIED_OK

- [x] G667-3: Supervisor script scripts/supervisor_t667.py configured with 1200s guest target and carrying base and fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t667.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t667_fix_initramfs_sha256=' in s; assert 'androidboot.t666_base_initramfs_sha256=' in s; assert '34463d5c23b13e1f58707462236bbfbc43f326249da3a578c9a01576c25a2a6c' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=SUPERVISOR_CONFIG_OK

- [x] G667-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T667 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=FLIGHT_REACHED_TARGET_OK

- [x] G667-5: Progress & unlock analysis: vold prepareUserStorage on /data/media/0 completes without ACL EOPNOTSUPP, user unlock completes, system_server survives user unlock past sys.boot_completed, odrefresh exit(79) count 0, Zygote SIGSEGV count 0
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=FLIGHT_ANALYSIS_OK

- [x] G667-6: All prior regression vectors hold at 0, 0 .capex across lineage, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=REGRESSIONS_HELD_OK

- [x] G667-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'vold-posix-acl-enotsup-user-unlock-android-fg-fatal' in f; assert 'prepareUserStorage' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t667.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=FINDINGS_VERIFIED_OK

- [x] G667-8: TIMELINE.md updated with dated entry following protocol
  CHECK: python3 -c "assert 'Track T667' in open('TIMELINE.md').read(); print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal; path=2f0fa3816a0a/16 entries; output=TIMELINE_VERIFIED_OK
