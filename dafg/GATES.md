# Gates: Track T648 — system-server-packagemanager-no-permission-controller (Chain A, diagnosis + fix)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact permissions-manager mechanism causing RuntimeException: There must be exactly one permissions manager; found [] in system_server PackageManagerService.getRequiredPermissionControllerLPr, build exactly ONE fix initramfs on T647 lineage providing the permission controller to PackageManagerService, boot on QEMU 11.1.2 (660s guest target), prove system_server passes PackageManagerService init or name the next wall exactly with its first failing line, report sys.boot_completed honestly, and deliver findings to /Users/Shared/agyx-track648-system-server-packagemanager-no-permission-controller.md and outcome to /Users/Shared/outcome-t648.txt.

- [x] G648-1: Missing permission controller mechanism pinned, fix built and packaged into exactly one fix initramfs on T647 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('guest-image/t648-manifest.json')); img = 'guest-image/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK
  EVIDENCE: guest-image/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz built with SHA-256 446f45ccd386f85ab0b27ebb7c7452e7deb7ac7bc0dc720292d7566e18ac73f8 (984403301 bytes) from T647 base 9333d37efd516d3b4ed614b6755b5b9c88e9d5b114cb4e4105f338c5ac9ecbef; CHECK passed FIX_IMAGE_VERIFIED_OK

- [x] G648-2: QEMU 11.1.2 boot executed for 660s guest target, prior stages held (0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts, 0 DeviceConfig errors, bootanim alive), guest log lines prove survival past PackageManagerService init or exact next wall
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 660.0; print('FLIGHT_VERIFIED_OK')"
  EXPECT: FLIGHT_VERIFIED_OK
  EVIDENCE: flights/flight-01/uart.log (18512 lines, 660.288s guest); 0 PMS permission manager exceptions (eliminated from 2 in T647); 0 zygote exit suicides (eliminated from 2 in T647); 0 DeviceConfig errors; 0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts; bootanim alive (57 hits); system_server alive through target window; CHECK passed FLIGHT_VERIFIED_OK

- [x] G648-3: Findings delivered with line-start Verdict, all MUST-carry strings, and outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track648-system-server-packagemanager-no-permission-controller.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-packagemanager-no-permission-controller' in f; assert 'PackageManagerService' in f; assert 'getRequiredPermissionControllerLPr' in f; assert 'permissions manager' in f; assert 'system_server' in f; assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert open('/Users/Shared/outcome-t648.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: /Users/Shared/agyx-track648-system-server-packagemanager-no-permission-controller.md written with line-start Verdict: PASS and all MUST-carry strings; /Users/Shared/outcome-t648.txt written with PASS; CHECK passed FINDINGS_VERIFIED_OK
