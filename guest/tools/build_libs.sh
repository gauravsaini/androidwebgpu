#!/bin/sh
# Cross-compile 32-bit x86 ELF shared objects and system binaries for guest userspace
# Outputs:
#   guest/initrd/system/lib/egl_webgpu.so (+ symlinks libEGL.so, libGLESv2.so)
#   guest/initrd/system/lib/gralloc.virtgpu.so
#   guest/initrd/system/lib/hwcomposer.virtgpu.so
#   guest/initrd/system/bin/surfaceflinger
#   guest/initrd/system/bin/app_process
#   guest/initrd/system/bin/test_triangle
#   guest/initrd/system/bin/skia_fb_test

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUEST_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$GUEST_DIR/.." && pwd)"

LIB_DIR="$GUEST_DIR/initrd/system/lib"
BIN_DIR="$GUEST_DIR/initrd/system/bin"
INC_DIR="$GUEST_DIR/include"
PATCH_DIR="$GUEST_DIR/patches"

mkdir -p "$LIB_DIR" "$BIN_DIR" "$INC_DIR"

echo "=== Building Guest Libraries & Binaries (32-bit x86 ELF) ==="

# Toolchain detection
CXX=""
CC=""
TOOLCHAIN_TYPE=""

# 1. Check GNU i686 cross-compiler
if command -v i686-linux-gnu-g++ >/dev/null 2>&1; then
    CXX="i686-linux-gnu-g++"
    CC="i686-linux-gnu-gcc"
    TOOLCHAIN_TYPE="gnu-i686"
    echo "Found GNU i686 compiler: $CXX"
elif command -v zig >/dev/null 2>&1; then
    CXX="zig c++ -target x86-linux-musl"
    CC="zig cc -target x86-linux-musl"
    TOOLCHAIN_TYPE="zig-musl"
    echo "Found Zig compiler: $CXX"
else
    # Check Android NDK Clang
    NDK_CXX="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "i686-linux-android*-clang++" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    NDK_CC="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "i686-linux-android*-clang" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$NDK_CXX" ] && [ -x "$NDK_CXX" ]; then
        CXX="$NDK_CXX"
        CC="$NDK_CC"
        TOOLCHAIN_TYPE="android-ndk"
        echo "Found Android NDK Clang++: $CXX"
    else
        echo "ERROR: No suitable 32-bit x86 C++ cross-compiler found (i686-linux-gnu-g++, Zig, or NDK required)."
        exit 1
    fi
fi

# Compile 1: egl_webgpu.so
echo "[build_libs] Compiling egl_webgpu.so..."
$CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/egl_webgpu.cpp" -o "$LIB_DIR/egl_webgpu.so"

# Create EGL / GLES symlinks
ln -sf egl_webgpu.so "$LIB_DIR/libEGL.so"
ln -sf egl_webgpu.so "$LIB_DIR/libGLESv2.so"

# Compile 2: gralloc.virtgpu.so
echo "[build_libs] Compiling gralloc.virtgpu.so..."
$CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/gralloc_virtgpu.cpp" -o "$LIB_DIR/gralloc.virtgpu.so"

# Compile 3: hwcomposer.virtgpu.so
echo "[build_libs] Compiling hwcomposer.virtgpu.so..."
$CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/hwcomposer_virtgpu.cpp" -o "$LIB_DIR/hwcomposer.virtgpu.so"

# Compile 4: surfaceflinger binary
echo "[build_libs] Compiling surfaceflinger..."
$CC -static -O2 -I"$INC_DIR" "$GUEST_DIR/surfaceflinger.c" -o "$BIN_DIR/surfaceflinger"
chmod +x "$BIN_DIR/surfaceflinger"

# Compile 5: app_process binary
echo "[build_libs] Compiling app_process..."
$CC -static -O2 -I"$INC_DIR" "$GUEST_DIR/app_process.c" -o "$BIN_DIR/app_process"
chmod +x "$BIN_DIR/app_process"
ln -sf app_process "$BIN_DIR/zygote" 2>/dev/null || true

# Compile 6: test_triangle binary
echo "[build_libs] Compiling test_triangle..."
$CC -O2 -I"$INC_DIR" "$GUEST_DIR/test_triangle.c" -L"$LIB_DIR" -lEGL -Wl,-rpath=/system/lib -o "$BIN_DIR/test_triangle"
chmod +x "$BIN_DIR/test_triangle"

# Compile 7: skia_fb_test binary
echo "[build_libs] Compiling skia_fb_test..."
$CXX -static -O2 -I"$INC_DIR" "$GUEST_DIR/skia_test/skia_fb_test.cpp" -o "$BIN_DIR/skia_fb_test" -lm
chmod +x "$BIN_DIR/skia_fb_test"

echo "=== Build Complete ==="
echo "Libraries in $LIB_DIR:"
ls -la "$LIB_DIR"
file "$LIB_DIR"/*.so

echo "Binaries in $BIN_DIR:"
file "$BIN_DIR/surfaceflinger" "$BIN_DIR/app_process" "$BIN_DIR/test_triangle" "$BIN_DIR/skia_fb_test"
