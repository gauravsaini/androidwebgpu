#!/bin/sh
# fetch_bzimage.sh — Option B direct download for 32-bit x86 bzImage
# Implements user-suggested sources with validation via verifyBzImage
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$(cd "$SCRIPT_DIR/../build" && pwd)"
OUT="$BUILD_DIR/bzImage"
TMP="$BUILD_DIR/bzImage.tmp"
ALPINE_URL="https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86/netboot/vmlinuz-lts"
TINYCORE_URL="http://tinycorelinux.net/15.x/x86/release/distribution_files/vmlinuz"
# Android-x86 OSDN/SourceForge — large ISO, only fetch if explicitly requested
ANDROID_X86_ISO_URL="https://downloads.sourceforge.net/project/android-x86/Release%209.0/android-x86_64-9.0-r2.iso"

echo "[fetch_bzimage] Target: $OUT"

fetch_and_verify() {
  url="$1"
  label="$2"
  echo "[fetch_bzimage] Trying $label: $url"
  if command -v curl >/dev/null 2>&1; then
    curl -L --fail --connect-timeout 15 --max-time 120 -o "$TMP" "$url" 2>&1 | tail -n 20
  elif command -v wget >/dev/null 2>&1; then
    wget -O "$TMP" "$url" 2>&1 | tail -n 20
  else
    echo "[fetch_bzimage] ERROR: curl/wget not found"
    return 1
  fi
  if [ ! -f "$TMP" ]; then echo "[fetch_bzimage] download failed"; return 1; fi
  ls -lh "$TMP"
  file "$TMP" | head -n 5
  # verify HdrS + 0xAA55 via node
  node -e "
import fs from 'fs';
const p=process.argv[1];
const b=fs.readFileSync(p);
const ok = b.length>0x300 && b.readUInt16LE(0x1FE)===0xAA55 && b.slice(0x202,0x206).toString()==='HdrS';
console.log('[verify] size',b.length,'HdrS',ok,'0xAA55',b.readUInt16LE(0x1FE).toString(16));
if(!ok) process.exit(1);
" "$TMP" || return 1
  mv "$TMP" "$OUT"
  echo "[fetch_bzimage] SUCCESS: $label -> $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"
  file "$OUT"
  return 0
}

# Try Alpine first (fastest, virtio present)
if fetch_and_verify "$ALPINE_URL" "Alpine v3.19 x86 LTS (Source 1)"; then
  echo "[fetch_bzimage] Using Alpine kernel — virtio + serial console ready"
  exit 0
fi
echo "[fetch_bzimage] Alpine failed, trying TinyCore..."

if fetch_and_verify "$TINYCORE_URL" "TinyCore 15.x x86 (Source 3)"; then
  echo "[fetch_bzimage] Using TinyCore kernel — minimal 5 MB"
  exit 0
fi
echo "[fetch_bzimage] TinyCore failed"

# Fallback: keep existing 6.8M kernel if present
if [ -f "$OUT" ] && [ -s "$OUT" ]; then
  echo "[fetch_bzimage] Keeping existing $OUT"
  ls -lh "$OUT"
  exit 0
fi

echo "[fetch_bzimage] ERROR: All direct downloads failed. Options:"
echo "  1. Check network / try again"
echo "  2. Manual Android-x86 ISO: curl -LO https://osdn.net/projects/android-x86/releases/... && 7z e android-x86*.iso kernel -oguest/build/ && mv guest/build/kernel guest/build/bzImage"
echo "  3. Keep synthetic 64KB kernel (guest/tools/generate_bzimage.mjs) for JS-only proof"
exit 1
