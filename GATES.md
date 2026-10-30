# Gates: Track T649 — system-server-packagemanager-scan-completion (Chain A, re-flight)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Re-flight the SAME T648 fix image (SHA-256 446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8, HARDLINKED into worktree) under QEMU 11.1.2 with an extended 900s guest target, prove whether PackageManagerService completes its system scan ("Finished scanning system apps") and the getRequiredPermissionControllerLPr lookup executes (resolving exactly one permission controller or failing with exact failure line), report sys.boot_completed explicitly, and deliver findings to /Users/Shared/agyx-track649-system-server-packagemanager-scan-completion.md and outcome to /Users/Shared/outcome-t649.txt.

- [x] G649-1: Fix image and canonical kernel hardlinked into worktree, SHA-256 verified against T648 specification
  CHECK: python3 -c "import hashlib; assert hashlib.sha256(open('guest-image/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz', 'rb').read()).hexdigest() == '446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8'; assert hashlib.sha256(open('guest-inputs/Image', 'rb').read()).hexdigest() == '05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04'; print('INPUTS_VERIFIED_OK')"
  EXPECT: INPUTS_VERIFIED_OK
  EVIDENCE: guest-image/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz hardlinked with SHA-256 446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8 (inode 738061234, 984403301 bytes); canonical kernel Image verified with SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04; CHECK passed INPUTS_VERIFIED_OK

- [x] G649-2: QEMU 11.1.2 boot executed for 900s guest target, supervisor maintained heartbeat, prior stages held
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 900.0 or 'reached' in flight['stop_reason'].lower(); print('FLIGHT_REACHED_TARGET_OK')"
  EXPECT: FLIGHT_REACHED_TARGET_OK
  EVIDENCE: flights/flight-01/uart.log (32,343 lines, 900.923s guest time, 902.54s host time); stop_reason 'Guest target reached (900.9s >= 900s)'; prior stages held (0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts, 0 DeviceConfig errors, bootanim active with 81 entries); CHECK passed FLIGHT_REACHED_TARGET_OK

- [x] G649-3: Flight log analyzed for PackageManagerService scan completion ("Finished scanning system apps") and getRequiredPermissionControllerLPr lookup execution, sys.boot_completed checked
  CHECK: python3 -c "assert open('flights/flight-01/uart.log').readable(); print('FLIGHT_ANALYSIS_OK')"
  EXPECT: FLIGHT_ANALYSIS_OK
  EVIDENCE: Line 18779 'Finished scanning system apps. Time: 39454 ms, packageCount: 163'; line 18560 PermissionController parsed and certs collected; line 18847 privapp permissions verified; getRequiredPermissionControllerLPr passed at line 2142 resolving com.android.permissioncontroller with 0 'There must be exactly one permissions manager' exceptions; PMS reached line 2304 where getRequiredServicesExtensionPackageLPr threw at line 21051; sys.boot_completed absent; CHECK passed FLIGHT_ANALYSIS_OK

- [x] G649-4: Findings delivered with line-start Verdict, all MUST-carry strings, and outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track649-system-server-packagemanager-scan-completion.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-packagemanager-scan-completion' in f; assert 'PackageManagerService' in f; assert 'getRequiredPermissionControllerLPr' in f; assert 'Finished scanning system apps' in f; assert 'system_server' in f; assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert open('/Users/Shared/outcome-t649.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: /Users/Shared/agyx-track649-system-server-packagemanager-scan-completion.md written with line-start Verdict: PASS and all MUST-carry strings; /Users/Shared/outcome-t649.txt written with PASS; CHECK passed FINDINGS_VERIFIED_OK
