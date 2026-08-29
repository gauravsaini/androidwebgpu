#!/bin/sh
# Packaging script for AndroidWebGPU v86 initrd image
# Creates standard cpio archive compressed with gzip
# Outputs to guest/build/initrd.img and dist/initrd.img

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUEST_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$GUEST_DIR/.." && pwd)"
INITRD_SRC="$GUEST_DIR/initrd"
OUTPUT_DIR="$GUEST_DIR/build"
OUTPUT_IMG="$OUTPUT_DIR/initrd.img"
DIST_DIR="$ROOT_DIR/dist"

echo "[build_initrd] Preparing initrd root directory: $INITRD_SRC"

# Create output directory
mkdir -p "$OUTPUT_DIR" "$DIST_DIR"

# Ensure init script is executable
chmod +x "$INITRD_SRC/init"

# Create necessary mount directory structures in initrd
mkdir -p "$INITRD_SRC/dev" "$INITRD_SRC/dev/dri" "$INITRD_SRC/proc" "$INITRD_SRC/sys" "$INITRD_SRC/tmp" "$INITRD_SRC/data"
mkdir -p "$INITRD_SRC/system/bin" "$INITRD_SRC/system/lib" "$INITRD_SRC/system/framework" "$INITRD_SRC/system/etc/vintf"
mkdir -p "$INITRD_SRC/vendor/etc/vintf" "$INITRD_SRC/bin"

# Detect C compiler for static shell
CC=""
if command -v i686-linux-gnu-gcc >/dev/null 2>&1; then
    CC="i686-linux-gnu-gcc"
elif command -v zig >/dev/null 2>&1; then
    CC="zig cc -target x86-linux-musl"
fi

if [ -n "$CC" ] && [ -f "$SCRIPT_DIR/sh.c" ]; then
    echo "[build_initrd] Compiling static /bin/sh..."
    $CC -static -O2 "$SCRIPT_DIR/sh.c" -o "$INITRD_SRC/bin/sh"
    cp "$INITRD_SRC/bin/sh" "$INITRD_SRC/system/bin/sh"
    chmod +x "$INITRD_SRC/bin/sh" "$INITRD_SRC/system/bin/sh"
fi

# Compile synthetic virtio probe
if [ -x "$SCRIPT_DIR/build_synthetic_probe.sh" ]; then
    echo "[build_initrd] Compiling synthetic_virtio_probe..."
    "$SCRIPT_DIR/build_synthetic_probe.sh"
fi

# Compile 32-bit x86 shared objects and binaries
if [ -x "$SCRIPT_DIR/build_libs.sh" ]; then
    echo "[build_initrd] Compiling guest libraries and binaries..."
    "$SCRIPT_DIR/build_libs.sh"
fi

# Copy VINTF device manifest to /system/etc/vintf and /vendor/etc/vintf
if [ -f "$GUEST_DIR/etc/vintf/device_manifest.xml" ]; then
    cp "$GUEST_DIR/etc/vintf/device_manifest.xml" "$INITRD_SRC/system/etc/vintf/"
    cp "$GUEST_DIR/etc/vintf/device_manifest.xml" "$INITRD_SRC/vendor/etc/vintf/"
fi

# Stage authentic ART and Framework assets (boot.art, core-libart.jar, ext.jar, framework.jar, services.jar)
echo "[build_initrd] Staging authentic Android ART runtime and framework bytecode assets..."
node "$SCRIPT_DIR/stage_authentic_framework.mjs"

# Stage F-Droid APK into /system/app/org.fdroid.fdroid/base.apk
mkdir -p "$INITRD_SRC/system/app/org.fdroid.fdroid"
if [ -f "$ROOT_DIR/F-Droid.apk" ]; then
    echo "[build_initrd] Staging F-Droid.apk into system/app/org.fdroid.fdroid/base.apk..."
    cp "$ROOT_DIR/F-Droid.apk" "$INITRD_SRC/system/app/org.fdroid.fdroid/base.apk"
fi

# Ensure zygote symlink points to app_process
ln -sf app_process "$INITRD_SRC/system/bin/zygote" 2>/dev/null || true
chmod +x "$INITRD_SRC/system/bin"/* 2>/dev/null || true

# Package cpio.gz archive
echo "[build_initrd] Packaging initrd archive to: $OUTPUT_IMG"
(cd "$INITRD_SRC" && find . | cpio -o -H newc | gzip -9 > "$OUTPUT_IMG")

# Copy to dist/
cp "$OUTPUT_IMG" "$DIST_DIR/initrd.img"

echo "[build_initrd] SUCCESS: Created $OUTPUT_IMG ($(wc -c < "$OUTPUT_IMG" | tr -d ' ') bytes)"
echo "[build_initrd] Staged copy to $DIST_DIR/initrd.img"
