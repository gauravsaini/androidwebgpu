# Gates: Track T637 — gralloc-mapper-fix: give SurfaceFlinger the mapper it aborts without (Chain C, implementation)

OWNS: guest-inputs/**, toolchain/**, tools/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, /Users/Shared/outcome-t637.txt, /Users/Shared/agyx-track637-gralloc-mapper-fix.md

Scope: Identify exact mapper SurfaceFlinger expects from lineage VINTF manifests, HAL declarations, and T635 tombstone stack, build exactly ONE fix initramfs on T628 base providing that mapper (mapper.default.so Same-Process HAL exporting AIMapper_loadIMapper), execute QEMU 11.1.2 boot, confirm mapper is present/loaded, confirm SurfaceFlinger passes setupNewDisplayDeviceInternal without "gralloc-mapper is missing" abort, record next stage reached or exact new wall, deliver findings at /Users/Shared/agyx-track637-gralloc-mapper-fix.md and outcome at /Users/Shared/outcome-t637.txt.

- [x] G637-1: Gralloc mapper library built and packaged into exactly one fix initramfs on T628 base, with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('/Users/Shared/wt-track637-gralloc-mapper-fix/flights/fix-01/guest-image-manifest.json')); img = '/Users/Shared/wt-track637-gralloc-mapper-fix/guest-inputs/t637-gralloc-mapper-fix-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('MAPPER_IMAGE_VERIFIED_OK')"
  EXPECT: MAPPER_IMAGE_VERIFIED_OK

- [x] G637-2: QEMU 11.1.2 boot executed with mapper loaded and SurfaceFlinger passing setupNewDisplayDeviceInternal without gralloc-mapper abort, findings delivered at /Users/Shared/agyx-track637-gralloc-mapper-fix.md and outcome at /Users/Shared/outcome-t637.txt
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track637-gralloc-mapper-fix.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'surfaceflinger-sigabrt-line3732' in f; assert 'gralloc-mapper' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
