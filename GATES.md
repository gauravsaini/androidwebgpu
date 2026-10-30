# Gates: Track T646 — system-server-zygote-sigkill (Chain A, diagnosis + fix)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact mechanism causing "init: Service 'zygote' (pid N) received signal 9" following apexd: getActivePackages in system_server, build exactly ONE fix initramfs on T644 lineage, boot on QEMU 11.1.2 (660s guest target), prove survival past old death point or name the next wall exactly with its first failing line, report sys.boot_completed, and deliver findings to /Users/Shared/agyx-track646-system-server-zygote-sigkill.md and outcome to /Users/Shared/outcome-t646.txt.

- [x] G646-1: Exact mechanism pinned, fix built and packaged into exactly one fix initramfs on T644 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('guest-image/t646-manifest.json')); img = 'guest-image/t646-system-server-zygote-sigkill-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK
  EVIDENCE: FIX_IMAGE_VERIFIED_OK (sha256=ad918c0559913d0b9d81a4488c34f86bbbc45b9ae0e285bcb2d88e4751af4629, 984403240 bytes)

- [x] G646-2: QEMU 11.1.2 boot executed for 660s guest target, prior stages held (0 namespace aborts, 0 allowlist aborts, 0 NoClassDefFoundError, bootanim alive), guest log lines prove survival or exact wall, findings delivered with line-start Verdict and all MUST-carry strings
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track646-system-server-zygote-sigkill.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert any(l in ['Verdict: PASS', 'Verdict: FAIL'] for l in lines[:5]); assert 'WALL-TYPE-TAG guest type-1' in f or 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'system-server-zygote-sigkill-after-bootstrap' in f; assert 'received signal 9' in f; assert 'Widevine Factory HAL' in f; assert 'status=-3' in f; assert 'IDrmFactory/widevine' in f; assert 'VINTF' in f; assert 'system_server' in f; assert open('/Users/Shared/outcome-t646.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: FINDINGS_VERIFIED_OK (Verdict: FAIL, guest uptime 665.4s, 0 Widevine aborts, Zygote SIGKILL pinned to system_server self-kill via NoClassDefFoundError in FeatureFlagsService, next wall system-server-deviceconfig-listener-noclassdeffound)
