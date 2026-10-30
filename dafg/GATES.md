# Gates: Track T647 — system-server-deviceconfig-listener (Chain A, diagnosis + fix)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact missing-class mechanism causing NoClassDefFoundError: Landroid/provider/DeviceConfig$OnPropertiesChangedListener in system_server FeatureFlagsService, build exactly ONE fix initramfs on T646 lineage providing the class on BOOTCLASSPATH, boot on QEMU 11.1.2 (660s guest target), prove system_server passes FeatureFlagsService startup or name the next wall exactly with its first failing line, report sys.boot_completed, and deliver findings to /Users/Shared/agyx-track647-system-server-deviceconfig-listener.md and outcome to /Users/Shared/outcome-t647.txt.

- [x] G647-1: Missing class mechanism pinned, fix built and packaged into exactly one fix initramfs on T646 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('guest-image/t647-manifest.json')); img = 'guest-image/t647-system-server-deviceconfig-listener-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK
  EVIDENCE: guest-image/t647-system-server-deviceconfig-listener-initramfs.cpio.gz built with SHA-256 9333d37efd516d3b4ed614b6755b5b9c88e9d5b114cb4e4105f338c5ac9ecbef from base T646 ad918c0559913d0b9d81a4488c34f86bbbc45b9ae0e285bcb2d88e4751af4629; CHECK passed FIX_IMAGE_VERIFIED_OK

- [x] G647-2: QEMU 11.1.2 boot executed for 660s guest target, prior stages held (0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts, bootanim alive), guest log lines prove survival past FeatureFlagsService or exact next wall
  CHECK: python3 -c "import json; flight = json.load(open('flights/flight-01/flight-end.json')); assert flight['guest_log_max_seconds'] >= 660.0; print('FLIGHT_VERIFIED_OK')"
  EXPECT: FLIGHT_VERIFIED_OK
  EVIDENCE: flights/flight-01/uart.log (34641 lines, 660.288s guest); FeatureFlagsService passed twice (L14606, L28512); 0 namespace aborts, 0 allowlist aborts, 0 Widevine aborts; next wall at L17493/L31177 PackageManagerService getRequiredPermissionControllerLPr; CHECK passed FLIGHT_VERIFIED_OK

- [ ] G647-3: Findings delivered with line-start Verdict, all MUST-carry strings, and outcome file matching Verdict
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track647-system-server-deviceconfig-listener.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'system-server-deviceconfig-listener-noclassdeffound' in f; assert 'DeviceConfig$OnPropertiesChangedListener' in f; assert 'FeatureFlagsBinder' in f; assert 'FeatureFlagsService' in f; assert 'system_server' in f; assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert open('/Users/Shared/outcome-t647.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: pending
