#!/bin/sh
# Cross-compile 32-bit (i686) and 64-bit (x86_64) shared objects and system binaries for guest userspace
# Outputs:
#   guest/initrd/system/lib/ (32-bit ELF)
#   guest/initrd/system/lib64/ (64-bit ELF)
#   guest/initrd/system/bin/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUEST_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT_DIR="$(cd "$GUEST_DIR/.." && pwd)"

LIB32_DIR="$GUEST_DIR/initrd/system/lib"
LIB64_DIR="$GUEST_DIR/initrd/system/lib64"
BIN_DIR="$GUEST_DIR/initrd/system/bin"
INC_DIR="$GUEST_DIR/include"
PATCH_DIR="$GUEST_DIR/patches"

mkdir -p "$LIB32_DIR" "$LIB64_DIR" "$BIN_DIR" "$INC_DIR"

echo "=== Building Multiarch Guest Libraries & Binaries (i686 + x86_64 ELF) ==="

# -------------------------------------------------------------
# 1. Detect 32-bit (i686) Toolchain
# -------------------------------------------------------------
CXX32=""
CC32=""
if command -v i686-linux-gnu-g++ >/dev/null 2>&1; then
    CXX32="i686-linux-gnu-g++"
    CC32="i686-linux-gnu-gcc"
    echo "Found GNU i686 compiler: $CXX32"
elif command -v zig >/dev/null 2>&1; then
    CXX32="zig c++ -target x86-linux-musl"
    CC32="zig cc -target x86-linux-musl"
    echo "Found Zig 32-bit compiler: $CXX32"
else
    NDK_CXX="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "i686-linux-android*-clang++" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    NDK_CC="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "i686-linux-android*-clang" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$NDK_CXX" ] && [ -x "$NDK_CXX" ]; then
        CXX32="$NDK_CXX"
        CC32="$NDK_CC"
    fi
fi

# -------------------------------------------------------------
# 2. Detect 64-bit (x86_64) Toolchain
# -------------------------------------------------------------
CXX64=""
CC64=""
if command -v x86_64-linux-gnu-g++ >/dev/null 2>&1; then
    CXX64="x86_64-linux-gnu-g++"
    CC64="x86_64-linux-gnu-gcc"
    echo "Found GNU x86_64 compiler: $CXX64"
elif command -v zig >/dev/null 2>&1; then
    CXX64="zig c++ -target x86_64-linux-musl"
    CC64="zig cc -target x86_64-linux-musl"
    echo "Found Zig 64-bit compiler: $CXX64"
else
    NDK_CXX="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "x86_64-linux-android*-clang++" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    NDK_CC="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "x86_64-linux-android*-clang" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$NDK_CXX" ] && [ -x "$NDK_CXX" ]; then
        CXX64="$NDK_CXX"
        CC64="$NDK_CC"
    fi
fi

# -------------------------------------------------------------
# 3. Build Function for Target Arch
# -------------------------------------------------------------
build_arch_libs() {
    T_CXX="$1"
    T_CC="$2"
    T_LIB_DIR="$3"
    ARCH_NAME="$4"

    echo ">>> Compiling shared libraries for $ARCH_NAME into $T_LIB_DIR..."

    # egl_webgpu.so
    $T_CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/egl_webgpu.cpp" -o "$T_LIB_DIR/egl_webgpu.so"
    ln -sf egl_webgpu.so "$T_LIB_DIR/libEGL.so"
    ln -sf egl_webgpu.so "$T_LIB_DIR/libGLESv2.so"
    ln -sf egl_webgpu.so "$T_LIB_DIR/libGLESv3.so" 2>/dev/null || true
    ln -sf egl_webgpu.so "$T_LIB_DIR/libvulkan.so" 2>/dev/null || true

    # gralloc.virtgpu.so
    $T_CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/gralloc_virtgpu.cpp" -o "$T_LIB_DIR/gralloc.virtgpu.so"

    # hwcomposer.virtgpu.so
    $T_CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/hwcomposer_virtgpu.cpp" -o "$T_LIB_DIR/hwcomposer.virtgpu.so"

    # libandroid.so
    $T_CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/libandroid.cpp" -o "$T_LIB_DIR/libandroid.so"

    # libart.so
    $T_CXX -shared -fPIC -O2 -I"$INC_DIR" "$PATCH_DIR/libart.cpp" -o "$T_LIB_DIR/libart.so"
}

# Build 32-bit libraries
if [ -n "$CXX32" ]; then
    build_arch_libs "$CXX32" "$CC32" "$LIB32_DIR" "i686 (32-bit)"
fi

# Build 64-bit libraries
if [ -n "$CXX64" ]; then
    build_arch_libs "$CXX64" "$CC64" "$LIB64_DIR" "x86_64 (64-bit)"
fi

# -------------------------------------------------------------
# 4. Build System Binaries
# -------------------------------------------------------------
echo ">>> Compiling guest system binaries..."

BIN_CXX="${CXX32:-$CXX64}"
BIN_CC="${CC32:-$CC64}"
BIN_LIB="${LIB32_DIR}"

# surfaceflinger binary linked with EGL and DRM VirtIO-GPU driver
$BIN_CXX -static -O2 -I"$INC_DIR" "$GUEST_DIR/surfaceflinger.c" "$PATCH_DIR/egl_webgpu.cpp" -o "$BIN_DIR/surfaceflinger" -lpthread -lm 2>/dev/null || \
$BIN_CC -static -O2 -I"$INC_DIR" "$GUEST_DIR/surfaceflinger.c" "$PATCH_DIR/egl_webgpu.cpp" -o "$BIN_DIR/surfaceflinger" -lpthread -lm
chmod +x "$BIN_DIR/surfaceflinger"

# app_process binary
$BIN_CC -static -O2 -I"$INC_DIR" "$GUEST_DIR/app_process.c" -o "$BIN_DIR/app_process"
chmod +x "$BIN_DIR/app_process"
ln -sf app_process "$BIN_DIR/zygote" 2>/dev/null || true

# test_triangle binary
$BIN_CC -O2 -I"$INC_DIR" "$GUEST_DIR/test_triangle.c" -L"$BIN_LIB" -lEGL -Wl,-rpath=/system/lib -o "$BIN_DIR/test_triangle"
chmod +x "$BIN_DIR/test_triangle"

# skia_fb_test binary
$BIN_CXX -static -O2 -I"$INC_DIR" "$GUEST_DIR/skia_test/skia_fb_test.cpp" -o "$BIN_DIR/skia_fb_test" -lm
chmod +x "$BIN_DIR/skia_fb_test"

echo "=== Build Complete ==="
echo "32-bit Libraries in $LIB32_DIR:"
ls -la "$LIB32_DIR"
echo "64-bit Libraries in $LIB64_DIR:"
ls -la "$LIB64_DIR"
