#!/bin/sh
# Automated Build Script for AndroidWebGPU v86 Guest Assets (bzImage and initrd.img)
# Option B: Direct download (Alpine/TinyCore) preferred, fallback to synthetic
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# --- 1. bzImage: try real kernel via direct download, fallback to synthetic ---
if [ -x "$SCRIPT_DIR/kernel/fetch_bzimage.sh" ]; then
  echo "[guest/build.sh] 1. Fetching real x86 bzImage via direct download (Alpine/TinyCore)..."
  if "$SCRIPT_DIR/kernel/fetch_bzimage.sh"; then
    echo "[guest/build.sh] Real bzImage ready: $(ls -lh "$SCRIPT_DIR/build/bzImage" | awk '{print $9, $5}')"
  else
    echo "[guest/build.sh] Direct fetch failed, falling back to synthetic 64KB bzImage..."
    node "$SCRIPT_DIR/tools/generate_bzimage.mjs"
  fi
else
  echo "[guest/build.sh] 1. Generating synthetic x86 bzImage (fallback)..."
  node "$SCRIPT_DIR/tools/generate_bzimage.mjs"
fi

echo "[guest/build.sh] 2. Generating ART boot.art & framework.jar assets..."
node "$SCRIPT_DIR/tools/generate_art_assets.mjs"

echo "[guest/build.sh] 3. Building initrd.img archive..."
"$SCRIPT_DIR/tools/build_initrd.sh"

echo "[guest/build.sh] SUCCESS: All guest boot assets built successfully."
echo "  bzImage:  $(ls -lh "$SCRIPT_DIR/build/bzImage" 2>/dev/null | awk '{print $5, $9}')  ($(file "$SCRIPT_DIR/build/bzImage" 2>/dev/null | cut -d: -f2-))"
echo "  initrd:   $(ls -lh "$SCRIPT_DIR/build/initrd.img" 2>/dev/null | awk '{print $5, $9}')"
