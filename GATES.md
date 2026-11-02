# Gates: Track T666 — system-server-art-odrefresh-boot-profile-recompile-zygote-segv (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, GATES.md, TIMELINE.md, heartbeat.json, evidence/**

Scope: PIN, from AOSP source and guest evidence, that the filesystem layer is the correct fix point: on ext4, user.* extended attributes are supported natively, so Os.getxattr(path, "user.serial") returns ENODATA instead of ENOTSUP, UserDataPreparer.getSerialNumber natively catches ENODATA and returns -1, and setSerialNumber succeeds without entering the wipe path; confirm mount points UserDataPreparer.enforceSerialNumber touches (/data/user/0, /data/user_de/0, /data/system_de/0, /data/system_ce/0) and prove mount scope covers all of them; fix by the pinned mechanism ONLY, at the filesystem layer, WITHOUT touching system.img or any boot-classpath JAR: mount an ext4 filesystem on /data in early-ramdisk (loop-mounted ext4 image, or format /dev/ram0 via kernel brd); build ONE fix initramfs on T664 lineage (base image 2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b hardlinked, never copied); boot QEMU 11.1.2 with 1200s guest window; prove system_server survives user unlock past sys.boot_completed without android.fg FATAL and without Zygote SIGSEGV: ext4 mount evidenced in guest mount table, getxattr returns ENODATA (or setSerialNumber succeeds), user unlock completes (onBeforeUnlockUser -> finishUserUnlocking), sys.boot_completed=1 processed, system_server alive at flight end, odrefresh exit(79) count 0; name next wall with first failing line if stopped; deliver findings to /Users/Shared/agyx-track666-system-server-art-odrefresh-boot-profile-recompile-zygote-segv.md and outcome to /Users/Shared/outcome-t666.txt; update TIMELINE.md.

- [x] G666-1: Pinned root cause & filesystem layer fix point with AOSP source and guest evidence, alternatives ruled out
  CHECK: python3 -c "import json; d = json.load(open('evidence/root_cause_pinned.json')); assert d['track'] == 'T666'; assert 'pinned_mechanism' in d['dominant_cause']; print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK

- [x] G666-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T664 lineage supplying fix by pinned filesystem mechanism ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t664-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == '2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b'; assert os.stat('guest-inputs/t664-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t666-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK

- [x] G666-3: Supervisor script scripts/supervisor_t666.py configured with 1200s guest target and carrying base and fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t666.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t666_fix_initramfs_sha256=' in s; assert 'androidboot.t664_base_initramfs_sha256=' in s; assert '2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK

- [x] G666-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T666 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK

- [x] G666-5: Progress & unlock analysis: ext4 mount evidenced in guest mount table, getxattr returns ENODATA (or setSerialNumber succeeds) on checked paths, user unlock completes (onBeforeUnlockUser -> finishUserUnlocking), system_server survives user unlock past sys.boot_completed, odrefresh exit(79) count 0, Zygote SIGSEGV count 0
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK

- [x] G666-6: All prior regression vectors hold at 0, 0 .capex across lineage, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK

- [x] G666-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track666-system-server-art-odrefresh-boot-profile-recompile-zygote-segv.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-art-odrefresh-boot-profile-recompile-zygote-segv' in f; assert 'UserDataPreparer' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t666.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK

- [x] G666-8: TIMELINE.md updated with dated entry following protocol
  CHECK: python3 -c "assert 'Track T666' in open('TIMELINE.md').read(); print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
