# Gates: Track T638 — skia-texture: get SurfaceFlinger past Skia primeShaderCache (Chain C, implementation)

OWNS: guest-inputs/**, toolchain/**, tools/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json, /Users/Shared/outcome-t638.txt, /Users/Shared/agyx-track638-skia-texture.md

Scope: Identify exact cause of AutoBackendTexture abort in SkiaRenderEngine::primeCache (128x128 test texture creation/mapping failure), build exactly ONE fix initramfs on T637 base addressing the root cause (mapper capability, buffer import, GL/EGL backend or RenderEngine configuration), boot QEMU 11.1.2 under supervisor control, verify SF reaches primeShaderCache and passes primeShaderCache without "Failed to create a valid texture" abort (or names exact new wall), record outcomes and deliverables.

- [x] G638-1: Skia texture fix built and packaged into exactly one fix initramfs on T637 base, with SHA-256 and lineage verified
  CHECK: uv run python -c "import hashlib, json; manifest = json.load(open('flights/fix-01/guest-image-manifest.json')); img = manifest['images']['initramfs']['path']; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK

- [x] G638-2: QEMU 11.1.2 boot executed with SF passing primeShaderCache without AutoBackendTexture abort (or exact new wall named), findings delivered at /Users/Shared/agyx-track638-skia-texture.md and outcome at /Users/Shared/outcome-t638.txt
  CHECK: uv run python -c "f = open('/Users/Shared/agyx-track638-skia-texture.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'surfaceflinger-skia-autobackendtexture-abort' in f; assert 'primeShaderCache' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
