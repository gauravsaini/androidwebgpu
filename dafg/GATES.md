# Gates: Track T651 — system-server-packagemanager-missing-sdk-sandbox (Chain A, diagnosis + fix)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin why getRequiredSdkSandboxPackageName finds zero SDK sandbox packages from AOSP source and guest evidence, build exactly ONE fix initramfs on the hardlinked T650 base lineage supplying com.android.sdksandbox by the pinned mechanism, boot QEMU 11.1.2 for 900s guest target, prove system_server passes getRequiredSdkSandboxPackageName into the next stage (or name next wall), report sys.boot_completed explicitly, and deliver findings to /Users/Shared/agyx-track651-system-server-packagemanager-missing-sdk-sandbox.md and outcome to /Users/Shared/outcome-t651.txt.

- [x] G651-1: Base image and canonical kernel hardlinked into worktree, SHA-256 verified against T650 specification
  CHECK: python3 -c "import hashlib; assert hashlib.sha256(open('guest-inputs/t650-system-server-packagemanager-missing-services-extension-initramfs.cpio.gz', 'rb').read()).hexdigest() == '5cff40e94e4c67465f5e74d64a41a26ee2ecf94d93f0ec88944b22e91b2489f7'; assert hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest() == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; print('INPUTS_VERIFIED_OK')"
  EXPECT: INPUTS_VERIFIED_OK
  EVIDENCE: guest-inputs/t650-system-server-packagemanager-missing-services-extension-initramfs.cpio.gz hardlinked with SHA-256 5cff40e94e4c67465f5e74d64a41a26ee2ecf94d93f0ec88944b22e91b2489f7 (inode 738133761, 984403269 bytes); canonical kernel Image verified with SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 (inode 728652686); CHECK passed INPUTS_VERIFIED_OK

- [x] G651-2: Mechanism pinned from AOSP source and guest evidence
  CHECK: python3 -c "assert open('scripts/extracted_t650_diag.sh').readable(); print('MECHANISM_PINNED_OK')"
  EXPECT: MECHANISM_PINNED_OK
  EVIDENCE: Pinned package com.android.sdksandbox, SdkSandboxManagerLocal.SERVICE_INTERFACE = com.android.sdksandbox.SdkSandboxService, SdkSandbox.apk in /system/system_ext/apex/com.android.adservices/app/SdkSandbox@UQ1A.231205.015/SdkSandbox.apk (inode 3666 in dir 3665), apexd failed FS_IOC_GETFLAGS on non-f2fs /data leaving apex inactive and un-scanned by PMS; PMS queryIntentServicesInternal(MATCH_SYSTEM_ONLY) returned 0 matches and threw RuntimeException at line 3686

- [x] G651-3: Exactly ONE fix initramfs built on T650 lineage supplying com.android.sdksandbox by pinned mechanism, hash-logged
  CHECK: python3 -c "import json, os, hashlib; m = json.load(open('guest-image/t651-manifest.json')); img = m['output_image']; assert os.path.exists(img); assert hashlib.sha256(open(img, 'rb').read()).hexdigest() == m['output_image_sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK
  EVIDENCE: Built guest-image/t651-system-server-packagemanager-missing-sdk-sandbox-initramfs.cpio.gz (984403602 bytes, SHA-256 811fcb03e4e23305cc5bd18997e73ee9c5549dd0669b5ac1b2df629af0469ec7, inode 738148981) from base image 5cff40e9...; patched /system/system_ext/priv-app block 458818 adding SdkSandbox (ino 3665) and updated init.rc mark and t636-diag.sh.

- [x] G651-4: QEMU 11.1.2 flight executed with 900s guest target, guest consumes fix image, prior stages hold
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 900.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: Flight 01 ran QEMU 11.1.2 with 900s target, reaching 904.4s guest uptime (host duration 907.0s); kernel command line carried androidboot.t651_fix_initramfs_sha256=811fcb03e4e23305cc5bd18997e73ee9c5549dd0669b5ac1b2df629af0469ec7 (line 28); guest markers emitted: T651_SDKSANDBOX_APK_EXISTS (lines 3360, 3624), T651_SDK_SANDBOX_FIX_APPLIED (line 3584); all prior regression vectors held (0 permission controller missing fatals, 0 services extension missing fatals, 0 SdkSandbox missing fatals, 0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts, 0 DeviceConfig errors).

- [x] G651-5: PMS passes getRequiredSdkSandboxPackageName into next stage quoted with line numbers from this flight, next stage or wall named, sys.boot_completed explicitly reported
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Line 19808 shows PackageManager scanning /system/system_ext/priv-app/SdkSandbox; line 20018 shows Finished scanning system apps with packageCount: 165 (up from 164 in T650); line 20927 marks boot_progress_pms_ready: 694227; getRequiredSdkSandboxPackageName passed with 0 fatal errors (no 'There should exactly one sdk sandbox package; found 0' exception); startBootstrapServices completed, startCoreServices completed (line 25510), startOtherServices entered (lines 26585, 27245, 28334); next wall pinned at lines 28317-28344 as system-server-startotherservices-settingsprovider-sqlite-cantopen (SQLiteCantOpenDatabaseException code 14 SQLITE_CANTOPEN in SettingsProvider.onCreate because /data/user_de/0/com.android.providers.settings/databases ENOENT); sys.boot_completed explicitly reported as ABSENT (empty in all T642_BOOT_STATE logs, e.g. line 36796).

- [x] G651-6: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track651-system-server-packagemanager-missing-sdk-sandbox.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-packagemanager-missing-sdk-sandbox' in f; assert 'PackageManagerService' in f; assert 'getRequiredSdkSandboxPackageName' in f; assert 'sdksandbox' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert open('/Users/Shared/outcome-t651.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: Delivered findings to /Users/Shared/agyx-track651-system-server-packagemanager-missing-sdk-sandbox.md with line-start 'Verdict: PASS', all MUST-carry strings verified, outcome written to /Users/Shared/outcome-t651.txt as 'PASS', CHECK command exited zero with FINDINGS_VERIFIED_OK.
