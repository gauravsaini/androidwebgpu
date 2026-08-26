#!/bin/bash
# Download v86 runtime assets, BIOS files, and a pre-built Linux guest image
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

V86_DIR="$PROJECT_ROOT/v86"
BIOS_DIR="$PROJECT_ROOT/bios"
GUEST_DIR="$PROJECT_ROOT/guest/build"

mkdir -p "$V86_DIR" "$BIOS_DIR" "$GUEST_DIR"

echo "=== Downloading v86 runtime ==="

if [ ! -f "$V86_DIR/libv86.js" ]; then
    curl -fSL "https://nicedoctor.github.io/libv86.js" -o "$V86_DIR/libv86.js"
    echo "✓ libv86.js"
else
    echo "✓ libv86.js exists"
fi

if [ ! -f "$V86_DIR/v86.wasm" ]; then
    curl -fSL "https://nicedoctor.github.io/v86.wasm" -o "$V86_DIR/v86.wasm"
    echo "✓ v86.wasm"
else
    echo "✓ v86.wasm exists"
fi

echo "=== Downloading BIOS ==="

if [ ! -f "$BIOS_DIR/seabios.bin" ]; then
    curl -fSL "https://raw.githubusercontent.com/nicedoctor/nicedoctor.github.io/main/seabios.bin" -o "$BIOS_DIR/seabios.bin"
    echo "✓ seabios.bin"
else
    echo "✓ seabios.bin exists"
fi

if [ ! -f "$BIOS_DIR/vgabios.bin" ]; then
    curl -fSL "https://raw.githubusercontent.com/nicedoctor/nicedoctor.github.io/main/vgabios.bin" -o "$BIOS_DIR/vgabios.bin"
    echo "✓ vgabios.bin"
else
    echo "✓ vgabios.bin exists"
fi

echo "=== Downloading Buildroot Linux ==="

if [ ! -f "$GUEST_DIR/buildroot-bzimage.bin" ]; then
    curl -fSL "https://nicedoctor.github.io/buildroot-bzimage.bin" -o "$GUEST_DIR/buildroot-bzimage.bin"
    echo "✓ buildroot-bzimage.bin"
else
    echo "✓ buildroot-bzimage.bin exists"
fi

echo ""
ls -lh "$V86_DIR"/ "$BIOS_DIR"/ "$GUEST_DIR"/buildroot-bzimage.bin 2>/dev/null || true
echo "Done."
