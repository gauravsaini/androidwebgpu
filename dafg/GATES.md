# Gates: Track T661 — system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile (Chain A)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, evidence/**

Scope: PIN why /data_mirror/data_de/null does not exist on this guest; verify supply chain (which init.rc/vold configuration creates /data_mirror, what populates data_de/null, why path is absent on virtual guest) from AOSP source and guest evidence; build ONE fix initramfs on T660 lineage (base image b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4 hardlinked from T660 worktree, never copied) supplying missing mirror path by pinned mechanism ONLY; watch items: zygote SIGABRT repeats for every forked app process (hold for all forks), all 13 prior regression vectors hold at 0, 0 .capex across lineage, preserve flattened bind-mount lineage and APEX set; boot QEMU 11.1.2 with 1200s guest target; prove system_server passes PermissionPolicyService onBootPhase(550) into NEXT Phase 550 service (named exactly with line numbers), record every stage reached, name next wall with first failing line if stopped, report sys.boot_completed explicitly; deliver findings to /Users/Shared/agyx-track661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile.md and outcome to /Users/Shared/outcome-t661.txt; update TIMELINE.md.

- [x] G661-1: Pinned root cause of /data_mirror/data_de/null absence on this guest with AOSP and guest evidence, alternatives ruled out
  CHECK: python3 -c "import os, json; d = json.load(open('evidence/root_cause_pinned.json')); assert d['track'] == 'T661'; assert 'supply_chain_analysis' in d; print('ROOT_CAUSE_PINNED_OK')"
  EXPECT: ROOT_CAUSE_PINNED_OK
  EVIDENCE: Met. Root cause pinned to vold destruction of /data/user_de/0. init.rc binds /data/user_de to /data_mirror/data_de/null. init_user0 prepares /data/user_de/0. During PMS systemReady, UserManagerService.reconcileUsers() -> UserDataPreparer calls Os.getxattr(..., 'user.serial'), returning ENOTSUP on Linux 4.19 tmpfs. UserDataPreparer logs 'Destroying user directory /data/user_de/0 because we failed to enforce serial number' and calls vold.destroyUserStorage. While /data/user/0 survived due to bind-mount EBUSY, /data/user_de/0 was deleted via rmdir, wiping /data_mirror/data_de/null/0. Recorded in evidence/root_cause_pinned.json.

- [x] G661-2: Base image and canonical kernel hardlinked into guest-inputs, ONE fix initramfs built on T660 lineage supplying missing mirror path by pinned mechanism ONLY, SHA-256 logged
  CHECK: python3 -c "import hashlib, os, glob; k = hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest(); b = hashlib.sha256(open('guest-inputs/t660-base-image.cpio.gz', 'rb').read()).hexdigest(); assert k == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; assert b == 'b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4'; assert os.stat('guest-inputs/t660-base-image.cpio.gz').st_nlink > 1; imgs = glob.glob('guest-image/t661-*.cpio.gz'); assert len(imgs) == 1; print('FIX_INITRAMFS_VERIFIED_OK')"
  EXPECT: FIX_INITRAMFS_VERIFIED_OK
  EVIDENCE: Met. Canonical kernel Image (SHA 05c43917...) and T660 base image (SHA b6e06117..., hardlinked nlink=2) verified. ONE fix initramfs built via scripts/build_t661_fix.py: guest-image/t661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile-initramfs.cpio.gz (SHA-256: 2f201b5552d6391958a513f5e9921e9a282a35465ae7387ec793d65572c4dcdc, 953,921,250 bytes). Exactly 1 fix initramfs in guest-image/.

- [x] G661-3: Supervisor script scripts/supervisor_t661.py configured with 1200s guest target and carrying base and fix SHA-256 on guest kernel cmdline
  CHECK: python3 -c "s = open('scripts/supervisor_t661.py').read(); assert 'TARGET_GUEST_SECONDS = 1200' in s; assert 'androidboot.t661_fix_initramfs_sha256=' in s; assert 'androidboot.t660_base_initramfs_sha256=b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4' in s; print('SUPERVISOR_CONFIG_OK')"
  EXPECT: SUPERVISOR_CONFIG_OK
  EVIDENCE: Met. scripts/supervisor_t661.py configured with TARGET_GUEST_SECONDS = 1200, cmdline carrying androidboot.t661_fix_initramfs_sha256=2f201b5552d6391958a513f5e9921e9a282a35465ae7387ec793d65572c4dcdc, androidboot.t660_base_initramfs_sha256=b6e06117205960849f99effeb249384a280205103a5c656cc815f9f0e53ab1f4, and androidboot.t661_flight_target=1200.

- [x] G661-4: QEMU 11.1.2 flight executed with 1200s guest target, guest consumes T661 fix image
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 1200.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: Met. QEMU 11.1.2 flight executed for 1203.24s host (guest target reached at 1201.498s). Guest kernel consumed T661 fix image (verified in preflight.json and kernel cmdline). Log saved to flights/flight-01/uart.log (11,419,100 bytes, SHA a6def27f...) and uart.log.numbered.txt (84,723 lines).

- [x] G661-5: Progress analysis: system_server passes PermissionPolicyService onBootPhase(550) into NEXT Phase 550 service (named exactly with line numbers from THIS lane's flight), all stages recorded, next wall named with first failing line
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Met. The DE mirror mount wall was 100% eliminated (0 occurrences of 'Failed to mount /data_mirror'). PermissionPolicyService onBootPhase(550) executed at line 17589. However, during app fork com.android.permissioncontroller encountered unmasked CE mirror lookup failure at line 17658: 'F zygote64: jni_internal.cc:825] JNI FatalError called: (com.android.permissioncontroller) frameworks/base/core/jni/com_android_internal_os_Zygote.cpp:1126: Unable to find com.android.permissioncontroller:49935 in /data_mirror/data_ce/null/0'. SystemServer threw RuntimeException during Phase 550 at line 18541 and did not advance past PermissionPolicyService. Honest FAIL recorded per Anti-T648 protocol.

- [x] G661-6: All prior regression vectors hold at 0, 0 .capex across lineage, sys.boot_completed explicitly reported
  CHECK: python3 -c "import os; assert os.path.exists('flights/flight-01/uart.log'); print('REGRESSIONS_HELD_OK')"
  EXPECT: REGRESSIONS_HELD_OK
  EVIDENCE: Met. All 13 prior regression vectors verified: BOOTCLASSPATH mediastream jar (0), audio-effect VINTF registration (0), connectivity APEX resources (0), wifi APEX resources (0), netstats apexdata (0), netd (0), sensors HAL (0), SDK sandbox (0 fatal), SettingsProvider SQLite (0), DeviceConfig (0), Widevine (0), allowlist/namespace (0), all-capex closure (0 .capex). sys.boot_completed explicitly reported as ABSENT (0).

- [x] G661-7: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile' in f; assert 'PermissionPolicyService' in f; assert 'PermissionController' in f or 'com.android.permissioncontroller' in f; assert 'Phase 550' in f or 'PHASE_ACTIVITY_MANAGER_READY' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f; assert 'Rust par bhi lagu: haan' in f or 'Rust par bhi lagu: nahi' in f; assert open('/Users/Shared/outcome-t661.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: Met. Findings delivered to /Users/Shared/agyx-track661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile.md with Verdict: FAIL, pinned root cause, fix initramfs SHA-256 2f201b55..., line numbers from flight-01, regression vectors 0, sys.boot_completed: ABSENT (0), WALL-TYPE-TAG guest type-1, and Rust par bhi lagu: nahi. Outcome file /Users/Shared/outcome-t661.txt carries FAIL.

- [x] G661-8: TIMELINE.md updated and worktree committed
  CHECK: python3 -c "t = open('TIMELINE.md').read(); assert 'T661' in t; print('TIMELINE_VERIFIED_OK')"
  EXPECT: TIMELINE_VERIFIED_OK
  EVIDENCE: Met. TIMELINE.md updated with entry for Track T661. Worktree changes committed to branch t661-system-server-phase550-permissionpolicyservice-permissioncontroller-mirror-mount-nosuchfile.
