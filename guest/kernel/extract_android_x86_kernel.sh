#!/bin/sh
# extract_android_x86_kernel.sh — Source 2: Android-x86 ISO → bzImage
# Usage: ./extract_android_x86_kernel.sh [iso_path_or_url]
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$(cd "$SCRIPT_DIR/../build" && pwd)"
OUT="$BUILD_DIR/bzImage"

ISO_URL_DEFAULT="https://downloads.sourceforge.net/project/android-x86/Release%209.0/android-x86-9.0-r2.iso"
ISO_PATH="${1:-}"

if [ -z "$ISO_PATH" ]; then
  # check existing linux4.iso
  if [ -f "$BUILD_DIR/linux4.iso" ]; then
    echo "[extract] Found existing $BUILD_DIR/linux4.iso, trying to extract kernel..."
    ISO_PATH="$BUILD_DIR/linux4.iso"
  else
    echo "[extract] No ISO provided, downloading Android-x86 9.0 ( ~700 MB, may take a while)..."
    ISO_PATH="/tmp/android-x86.iso"
    if command -v curl >/dev/null 2>&1; then
      curl -L --fail -o "$ISO_PATH" "$ISO_URL_DEFAULT"
    else
      wget -O "$ISO_PATH" "$ISO_URL_DEFAULT"
    fi
  fi
fi

echo "[extract] ISO: $ISO_PATH"
ls -lh "$ISO_PATH"

if command -v 7z >/dev/null 2>&1; then
  echo "[extract] Using 7z to extract kernel..."
  7z e "$ISO_PATH" kernel -o"$BUILD_DIR/" -y 2>&1 | tail -n 20
  if [ -f "$BUILD_DIR/kernel" ]; then
    mv "$BUILD_DIR/kernel" "$OUT"
    echo "[extract] SUCCESS: $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"
    file "$OUT"
    node -e "
import fs from 'fs';
const b=fs.readFileSync(process.argv[1]);
console.log('verify', b.readUInt16LE(0x1FE).toString(16), b.slice(0x202,0x206).toString(), b.length);
" "$OUT"
    exit 0
  fi
fi

if command -v 7zz >/dev/null 2>&1; then
  7zz e "$ISO_PATH" kernel -o"$BUILD_DIR/" -y
  [ -f "$BUILD_DIR/kernel" ] && mv "$BUILD_DIR/kernel" "$OUT" && echo "done" && exit 0
fi

# fallback: try bsdtar
if command -v bsdtar >/dev/null 2>&1; then
  bsdtar -xf "$ISO_PATH" -C "$BUILD_DIR" kernel 2>&1 | tail -n 20
  [ -f "$BUILD_DIR/kernel" ] && mv "$BUILD_DIR/kernel" "$OUT" && echo "done" && exit 0
fi

echo "[extract] ERROR: Could not extract kernel. Install p7zip (brew install p7zip) or use Source 1/3 direct download."
echo "  7z e android-x86-*.iso kernel -oguest/build/"
echo "  mv guest/build/kernel guest/build/bzImage"
exit 1
