# Gates: Track T644 — system-server-apex-namespace: give system_server the linker namespaces its flattened APEXes need (Chain A, diagnosis + implementation)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact linker namespace mechanism causing "Error finding namespace of apex: no namespace called com_android_tethering" in system_server, build exactly ONE fix initramfs on T642 lineage, boot on QEMU 11.1.2, prove 0 occurrences of no namespace called com_android_tethering, system_server proceeding past RuntimeInit.commonInit, prior SF/zygote stages held, and report downstream stage and boot_completed status.

- [x] G644-1: Exact linker namespace mechanism pinned, fix built and packaged into exactly one fix initramfs on T642 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('/Users/Shared/wt-track644-system-server-apex-namespace/flights/flight-01/guest-image-manifest.json')); img = '/Users/Shared/wt-track644-system-server-apex-namespace/guest-image/t644-system-server-apex-namespace-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK

- [x] G644-2: QEMU 11.1.2 boot executed with 0 occurrences of no namespace called com_android_tethering, system_server proceeding past RuntimeInit.commonInit, prior SF/zygote stages held, downstream stage named, findings delivered at /Users/Shared/agyx-track644-system-server-apex-namespace.md and outcome at /Users/Shared/outcome-t644.txt
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track644-system-server-apex-namespace.md').read(); lines = [l.strip() for l in f.splitlines() if l.strip()]; assert lines[1] == 'Verdict: PASS' or lines[1] == 'Verdict: FAIL'; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'system-server-apex-namespace-missing' in f; assert 'no namespace called com_android_tethering' in f; assert 'linkerconfig' in f; assert 'RuntimeInit.commonInit' in f; assert 'system_server' in f; assert open('/Users/Shared/outcome-t644.txt').read().strip() in ['PASS', 'FAIL']; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
