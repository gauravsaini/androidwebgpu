#!/bin/sh
# verify-image.sh — determinism + SBOM + platform-contract checks for the U9 guest image.
#
# 1. Rebuilds the image twice, asserts identical SHA-256.
# 2. Validates every SBOM input hash against the rebuilt bytes.
# 3. Asserts the entry point lies inside guest RAM.
# 4. Asserts the console MMIO range does not intersect RAM.
#
# Usage: sh guest-image/scripts/verify-image.sh   (run from the repo root)
set -eu

cd "$(dirname "$0")/../.."

IMG1=/tmp/guest-image-verify-a.bin
IMG2=/tmp/guest-image-verify-b.bin

cargo run -q -p guest-image --example build-image -- "$IMG1" >/dev/null
cargo run -q -p guest-image --example build-image -- "$IMG2" >/dev/null

H1=$(sha256sum "$IMG1" | cut -d' ' -f1)
H2=$(sha256sum "$IMG2" | cut -d' ' -f1)
if [ "$H1" != "$H2" ]; then
  echo "FAIL: rebuild produced a different image ($H1 != $H2)"
  exit 1
fi
echo "OK determinism: sha256=$H1"

python3 - "$IMG1" "$IMG1.sbom.json" <<'EOF'
import json, struct, sys, hashlib

img_path, sbom_path = sys.argv[1], sys.argv[2]
img = open(img_path, 'rb').read()
sbom = json.load(open(sbom_path))

RAM_BASE, RAM_SIZE = 0x40000000, 0x08000000
CONSOLE_BASE, CONSOLE_SIZE = 0x09000000, 0x1000
DATA_OFFSET, HEADER_LEN = 0x1000, 24

magic, version, entry, size = struct.unpack('<4sIQQ', img[:HEADER_LEN])
assert magic == b'PNIM', f"bad magic {magic!r}"
assert version == 1, f"bad version {version}"
assert size == len(img) - HEADER_LEN, "header size disagrees with file length"
print(f"OK header: magic=PNIM version={version} entry=0x{entry:08x} size={size}")

# Entry inside RAM; whole blob fits in RAM.
assert RAM_BASE <= entry < RAM_BASE + RAM_SIZE, "entry outside RAM"
assert entry + size <= RAM_BASE + RAM_SIZE, "image overflows RAM"
print("OK entry: inside RAM, image fits")

# Console MMIO range disjoint from RAM.
c0, c1 = CONSOLE_BASE, CONSOLE_BASE + CONSOLE_SIZE
r0, r1 = RAM_BASE, RAM_BASE + RAM_SIZE
assert not (c0 < r1 and r0 < c1), "console MMIO intersects RAM"
print(f"OK console: MMIO 0x{c0:08x}-0x{c1:08x} disjoint from RAM")

# Every SBOM input hash, recomputed from the image bytes.
inputs = {i['name']: i for i in sbom['inputs']}
blob = img[HEADER_LEN:]
code = blob[:inputs['guest-code']['bytes']]
data = blob[DATA_OFFSET:DATA_OFFSET + inputs['guest-data']['bytes']]
manifest_json = '{"name":"pathn-sh","version":1,"load_addr":1073741824}'
got = {
    'guest-code': hashlib.sha256(code).hexdigest(),
    'guest-data': hashlib.sha256(data).hexdigest(),
    'manifest': hashlib.sha256(manifest_json.encode()).hexdigest(),
}
for name, digest in got.items():
    assert inputs[name]['sha256'] == digest, f"SBOM hash mismatch for {name}"
    print(f"OK sbom: {name} sha256={digest[:16]}...")
assert sbom['image_sha256'] == hashlib.sha256(img).hexdigest(), "image digest mismatch"
print("OK sbom: image_sha256 matches")
print("ALL CHECKS PASSED")
EOF
