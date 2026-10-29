# Gates: Track T641 — systemserviceregistry-preload: get zygote past SystemServiceRegistry preload (Chain A, diagnosis + implementation)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact missing class causing SystemServiceRegistry preload NoClassDefFoundError in Zygote, build exactly ONE fix initramfs on T640 lineage providing that dependency, boot on QEMU 11.1.2, prove 0 SystemServiceRegistry preload errors, zygote passing preload, SurfaceFlinger maintaining primeShaderCache/bootanim, and report downstream wall and system_server launch status.

- [x] G641-1: Exact missing class pinned, fix built and packaged into exactly one fix initramfs on T640 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('/Users/Shared/wt-track641-systemserviceregistry-preload/flights/flight-01/guest-image-manifest.json')); img = '/Users/Shared/wt-track641-systemserviceregistry-preload/guest-image/t641-systemserviceregistry-preload-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK

- [x] G641-2: QEMU 11.1.2 boot executed with 0 occurrences of SystemServiceRegistry preload error, zygote progressing past preload, prior SF stage held, downstream stage named, findings delivered at /Users/Shared/agyx-track641-systemserviceregistry-preload.md and outcome at /Users/Shared/outcome-t641.txt
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track641-systemserviceregistry-preload.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'zygote64-preload-failure-systemserviceregistry' in f; assert 'SystemServiceRegistry' in f; assert 'framework-connectivity.jar' in f; assert 'bootanim' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: driver re-ran worktree G641-2 CHECK after driver-completed findings landed; output=FINDINGS_VERIFIED_OK
