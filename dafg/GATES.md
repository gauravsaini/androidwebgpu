# Gates: Track T642 — system-server-fd-allowlist: get system_server past the fd allowlist check (Chain A, diagnosis + implementation)

OWNS: guest-inputs/**, guest-image/**, toolchain/**, tools/**, scripts/**, flights/**, dafg/GATES.md, GATES.md, TIMELINE.md, heartbeat.json

Scope: Pin the exact fd allowlist mechanism causing "(system_server) Not allowlisted (27): /system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar" death in forkSystemServer, build exactly ONE fix initramfs on T641 lineage, boot on QEMU 11.1.2, prove 0 occurrences of Not allowlisted, system_server proceeding past fork into its own init, prior SF/zygote stages held, and report downstream stage and system_server running status.

- [x] G642-1: Exact fd allowlist mechanism pinned, fix built and packaged into exactly one fix initramfs on T641 base with SHA-256 and lineage verified
  CHECK: python3 -c "import hashlib, json; manifest = json.load(open('/Users/Shared/wt-track642-system-server-fd-allowlist/flights/flight-01/guest-image-manifest.json')); img = '/Users/Shared/wt-track642-system-server-fd-allowlist/guest-image/t642-system-server-fd-allowlist-initramfs.cpio.gz'; h = hashlib.sha256(open(img, 'rb').read()).hexdigest(); assert h == manifest['images']['initramfs']['sha256']; print('FIX_IMAGE_VERIFIED_OK')"
  EXPECT: FIX_IMAGE_VERIFIED_OK

- [x] G642-2: QEMU 11.1.2 boot executed with 0 occurrences of (system_server) Not allowlisted, system_server proceeding past fork into its own init, prior SF/zygote stages held, downstream stage named, findings delivered at /Users/Shared/agyx-track642-system-server-fd-allowlist.md and outcome at /Users/Shared/outcome-t642.txt
  CHECK: python3 -c "f = open('/Users/Shared/agyx-track642-system-server-fd-allowlist.md').read(); assert 'Verdict: PASS' in f or 'Verdict: FAIL' in f; assert 'WALL-TYPE-TAG: guest (type-1)' in f; assert 'Rust par bhi lagu: haan' in f; assert 'zygote64-system-server-fd-allowlist-not-allowlisted' in f; assert 'Not allowlisted' in f; assert 'framework-connectivity.jar' in f; assert 'forkSystemServer' in f; assert 'system_server' in f; print('FINDINGS_VERIFIED_OK')"
  EXPECT: FINDINGS_VERIFIED_OK
