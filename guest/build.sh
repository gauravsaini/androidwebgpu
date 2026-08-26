#!/bin/sh
# Automated Build Script for AndroidWebGPU v86 Guest Assets (bzImage and initrd.img)
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "[guest/build.sh] 1. Generating Linux x86 bzImage..."
node "$SCRIPT_DIR/tools/generate_bzimage.mjs"

echo "[guest/build.sh] 2. Generating ART boot.art & framework.jar assets..."
node "$SCRIPT_DIR/tools/generate_art_assets.mjs"

echo "[guest/build.sh] 3. Building initrd.img archive..."
"$SCRIPT_DIR/tools/build_initrd.sh"

echo "[guest/build.sh] SUCCESS: All guest boot assets built successfully."
