#!/bin/sh
# Packaging script for AndroidWebGPU v86 initrd image
# Creates standard cpio archive compressed with gzip

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUEST_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
INITRD_SRC="$GUEST_DIR/initrd"
OUTPUT_DIR="$GUEST_DIR/build"
OUTPUT_IMG="$OUTPUT_DIR/initrd.img"

echo "[build_initrd] Preparing initrd root directory: $INITRD_SRC"

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Ensure init script is executable
chmod +x "$INITRD_SRC/init"

# Create necessary mount directory structures in initrd
mkdir -p "$INITRD_SRC/dev" "$INITRD_SRC/proc" "$INITRD_SRC/sys" "$INITRD_SRC/tmp" "$INITRD_SRC/data"
mkdir -p "$INITRD_SRC/system/bin" "$INITRD_SRC/system/lib" "$INITRD_SRC/system/framework" "$INITRD_SRC/system/etc/vintf"
mkdir -p "$INITRD_SRC/vendor/etc/vintf"

# Copy VINTF device manifest to /system/etc/vintf and /vendor/etc/vintf
if [ -f "$GUEST_DIR/etc/vintf/device_manifest.xml" ]; then
    cp "$GUEST_DIR/etc/vintf/device_manifest.xml" "$INITRD_SRC/system/etc/vintf/"
    cp "$GUEST_DIR/etc/vintf/device_manifest.xml" "$INITRD_SRC/vendor/etc/vintf/"
fi

# Package cpio.gz archive
echo "[build_initrd] Packaging initrd archive to: $OUTPUT_IMG"
(cd "$INITRD_SRC" && find . | cpio -o -H newc | gzip -9 > "$OUTPUT_IMG")

echo "[build_initrd] SUCCESS: Created $OUTPUT_IMG ($(wc -c < "$OUTPUT_IMG" | tr -d ' ') bytes)"
