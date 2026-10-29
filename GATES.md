# Gates: Track T640 — three-fix-merge: prove the T636+T637+T638 combined lineage (Chain A+C, implementation)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, outcome-t640.txt, agyx-track640-three-fix-merge.md

Scope: Merge T636 (classpath fix), T637 (gralloc-mapper fix), and T638 (SwiftShader AutoBackendTexture patch) into exactly ONE merged initramfs on T639 merged base, boot under QEMU 11.1.2 native oracle harness with detached supervisor, prove zygote inherits 20-jar BOOTCLASSPATH with 0 MediaPlayer preload errors, SurfaceFlinger loads mapper.default.so with 0 mapper-missing aborts, SF passes primeShaderCache with 0 texture aborts, and record next stage/wall reached (including system_server launch status); deliver findings at /Users/Shared/agyx-track640-three-fix-merge.md and outcome at /Users/Shared/outcome-t640.txt.

- [x] G640-1: Exactly ONE merged initramfs built from T639 merged base with T638 SwiftShader patch added, SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('/Users/Shared/wt-track640-three-fix-merge/flights/flight-01/guest-image-manifest.json')); img = manifest['images']['merged_initramfs']['path']; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['merged_initramfs']['sha256']; print('MERGED_IMAGE_VERIFIED_OK')"
  EXPECT: MERGED_IMAGE_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track640-three-fix-merge/dafg; path=205225b31f9f/17 entries; output=MERGED_IMAGE_VERIFIED_OK

- [x] G640-2: QEMU 11.1.2 boot executed with all three fixes proven simultaneously (zygote BOOTCLASSPATH + 0 preload errors, mapper loaded + 0 mapper aborts, primeShaderCache passed + 0 texture aborts) and next stage/wall reported; deliverables at /Users/Shared/agyx-track640-three-fix-merge.md and /Users/Shared/outcome-t640.txt
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track640-three-fix-merge.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'zygote64-preload-failure-mediaplayer' in f; assert 'surfaceflinger-sigabrt-line3732' in f; assert 'surfaceflinger-skia-autobackendtexture-abort' in f; assert 'framework-connectivity.jar' in f; assert 'mapper.default.so' in f; assert 'primeShaderCache' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
  EVIDENCE: exit=0; shell=/bin/sh; cwd=/Users/Shared/wt-track640-three-fix-merge/dafg; path=205225b31f9f/17 entries; output=FINDINGS_VERIFIED_OK
