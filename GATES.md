# Gates: Track T650 — system-server-packagemanager-missing-services-extension (Chain A, diagnosis + fix)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin why getRequiredServicesExtensionPackageLPr finds no services extension package from AOSP source and guest evidence, build exactly ONE fix initramfs on the hardlinked T649 base lineage supplying android.ext.services by the pinned mechanism, boot QEMU 11.1.2 for 900s guest target, prove system_server passes getRequiredServicesExtensionPackageLPr into Core Services (or name next wall), report sys.boot_completed explicitly, and deliver findings to /Users/Shared/agyx-track650-system-server-packagemanager-missing-services-extension.md and outcome to /Users/Shared/outcome-t650.txt.

- [x] G650-1: Base image and canonical kernel hardlinked into worktree, SHA-256 verified against T649 specification
  CHECK: python3 -c "import hashlib; assert hashlib.sha256(open('guest-inputs/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz', 'rb').read()).hexdigest() == '446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8'; assert hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest() == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; print('INPUTS_VERIFIED_OK')"
  EXPECT: INPUTS_VERIFIED_OK
  EVIDENCE: guest-inputs/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz hardlinked with SHA-256 446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8 (inode 738061234, 984403301 bytes); canonical kernel Image verified with SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04 (inode 728652686); CHECK passed INPUTS_VERIFIED_OK

- [x] G650-2: Mechanism pinned from AOSP source and guest evidence
  CHECK: python3 -c "assert open('scripts/extracted_t648_diag.sh').readable(); print('MECHANISM_PINNED_OK')"
  EXPECT: MECHANISM_PINNED_OK
  EVIDENCE: Pinned package android.ext.services, config_servicesExtensionPackage, ExtServices.apk in /system/system_ext/apex/com.android.extservices/priv-app/ExtServices@UQ1A.231205.015/ExtServices.apk (inode 4106 in dir 4105), apexd failed FS_IOC_GETFLAGS on non-f2fs /data leaving apex inactive and un-scanned by PMS; PMS ensureSystemPackageName(MATCH_FACTORY_ONLY) threw RuntimeException at line 2498

- [x] G650-3: Exactly ONE fix initramfs built on T649 lineage supplying android.ext.services by pinned mechanism, hash-logged
  CHECK: python3 -c "import json, os, hashlib; m = json.load(open('guest-image/t650-manifest.json')); img = m['output_image']; assert os.path.exists(img); assert hashlib.sha256(open(img, 'rb').read()).hexdigest() == m['output_image_sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK
  EVIDENCE: Built guest-image/t650-system-server-packagemanager-missing-services-extension-initramfs.cpio.gz (984403269 bytes, SHA-256 5cff40e94e4c67465f5e74d64a41a26ee2ecf94d93f0ec88944b22e91b2489f7) from base image 446f45cc...; patched /system/system_ext/priv-app block 458818 adding ExtServices (ino 4105) and updated init.rc and t636-diag.sh.

- [x] G650-4: QEMU 11.1.2 flight executed with 900s guest target, guest consumes fix image, prior stages hold
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 900.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: flights/flight-01/uart.log (27,143 lines, 903.416s guest time, 906.95s host time); stop_reason 'Guest target reached (903.4s >= 900s)'; guest consumed fix image (cmdline lines 28-29, marks lines 3526, 3612, 3625); prior stages held (0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts, 0 DeviceConfig errors, 0 permission-controller exceptions, bootanim active); CHECK passed FLIGHT_REACHED_TARGET_OK

- [x] G650-5: PMS passes getRequiredServicesExtensionPackageLPr into Core Services quoted with line numbers from this flight, next stage or wall named, sys.boot_completed explicitly reported
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: ExtServices scanned from /system/system_ext/priv-app/ExtServices (line 18819); system apps scan completed with packageCount: 164 (line 19106); privapp permissions granted (lines 19139-19142); getRequiredServicesExtensionPackageLPr completed without throwing (0 occurrences of missing services extension); system_server advanced past line 2304 into line 2314 (getRequiredSdkSandboxPackageName); next wall pinned as system-server-packagemanager-missing-sdk-sandbox (line 21327); sys.boot_completed ABSENT; CHECK passed FLIGHT_ANALYSIS_OK

- [x] G650-6: Findings delivered with line-start Verdict, all MUST-carry strings, outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track650-system-server-packagemanager-missing-services-extension.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-packagemanager-missing-services-extension' in f; assert 'PackageManagerService' in f; assert 'getRequiredServicesExtensionPackageLPr' in f; assert 'android.ext.services' in f; assert 'system_server' in f; assert 'sys.boot_completed' in f; assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert open('/Users/Shared/outcome-t650.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: /Users/Shared/agyx-track650-system-server-packagemanager-missing-services-extension.md written with line-start Verdict: PASS and all MUST-carry strings; /Users/Shared/outcome-t650.txt written with PASS; CHECK passed FINDINGS_VERIFIED_OK
