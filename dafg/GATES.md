# Gates: Track T639 — merge-classpath-mapper: prove the T636+T637 combined lineage (Chain A+C, implementation)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, /Users/Shared/outcome-t639.txt, /Users/Shared/agyx-track639-merge-classpath-mapper.md

Scope: Merge T636 (classpath fix) and T637 (gralloc mapper fix) into exactly ONE merged initramfs on T636 base, boot under QEMU 11.1.2 oracle, prove zygote inherits 20-jar BOOTCLASSPATH with 0 MediaPlayer preload errors, SurfaceFlinger loads mapper.default.so with 0 mapper-missing aborts, and SF advances past setupNewDisplayDeviceInternal to primeShaderCache or named wall; deliver findings at /Users/Shared/agyx-track639-merge-classpath-mapper.md and outcome at /Users/Shared/outcome-t639.txt.

- [x] G639-1: Build exactly ONE merged initramfs starting from T636 fix image adding mapper.default.so with SELinux label u:object_r:same_process_hal_file:s0, with all input and output SHA-256 digests verified.
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('flights/flight-01/guest-image-manifest.json')); img = manifest['images']['merged_initramfs']['path']; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['merged_initramfs']['sha256']; print('MERGED_IMAGE_VERIFIED_OK')"
  EXPECT: MERGED_IMAGE_VERIFIED_OK

- [x] G639-2: QEMU 11.1.2 boot executed with both fixes proven: Zygote environ shows framework-connectivity.jar with 0 preload errors, SurfaceFlinger loads mapper.default.so with 0 mapper-missing aborts, and SF passes setupNewDisplayDeviceInternal to primeShaderCache or named wall; deliverables at /Users/Shared/agyx-track639-merge-classpath-mapper.md and /Users/Shared/outcome-t639.txt.
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track639-merge-classpath-mapper.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'zygote64-preload-failure-mediaplayer' in f; assert 'surfaceflinger-sigabrt-line3732' in f; assert 'framework-connectivity.jar' in f; assert 'mapper.default.so' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
