#!/bin/sh
# Cross-compile native guest Rust system services and HALs for 32-bit x86 Linux/Android
# and copy them into the initrd rootfs structure.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$ROOT_DIR/guest/initrd/system/bin"

export PATH="$HOME/.cargo/bin:$PATH"

echo "=== Building Guest Services for 32-bit x86 Guest Environment ==="

cd "$ROOT_DIR"

# Detect NDK Clang
NDK_CLANG="/opt/homebrew/Caskroom/android-ndk/29/AndroidNDK14206865.app/Contents/NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin/i686-linux-android30-clang"
if [ ! -x "$NDK_CLANG" ]; then
    NDK_CLANG="$(find /opt /usr/local /Users -name "i686-linux-android*-clang" -perm +111 2>/dev/null | head -n 1 || true)"
fi

if [ -n "$NDK_CLANG" ] && [ -x "$NDK_CLANG" ]; then
    TARGET="i686-linux-android"
    rustup target add "$TARGET" 2>/dev/null || true
    echo "Using NDK Clang: $NDK_CLANG for target $TARGET"
    CARGO_TARGET_I686_LINUX_ANDROID_LINKER="$NDK_CLANG" \
    cargo build --release --target "$TARGET" \
        -p guest_servicemanager \
        -p pms_rs \
        -p ams_rs \
        -p inputflinger_rs \
        -p sensors_hal_virtual \
        -p audio_hal_virtual \
        -p camera_hal_virtual
    TARGET_DIR="$ROOT_DIR/target/$TARGET/release"
else
    TARGET="i686-unknown-linux-gnu"
    rustup target add "$TARGET" 2>/dev/null || true
    echo "Building for target $TARGET"
    cargo build --release --target "$TARGET" \
        -p guest_servicemanager \
        -p pms_rs \
        -p ams_rs \
        -p inputflinger_rs \
        -p sensors_hal_virtual \
        -p audio_hal_virtual \
        -p camera_hal_virtual
    TARGET_DIR="$ROOT_DIR/target/$TARGET/release"
fi

mkdir -p "$BIN_DIR"

# Copy binaries into guest initrd system/bin
cp "$TARGET_DIR/servicemanager" "$BIN_DIR/"
cp "$TARGET_DIR/service_check" "$BIN_DIR/"
cp "$TARGET_DIR/pms_rs" "$BIN_DIR/"
cp "$TARGET_DIR/ams_rs" "$BIN_DIR/"
cp "$TARGET_DIR/inputflinger_rs" "$BIN_DIR/"
cp "$TARGET_DIR/sensors_hal_virtual" "$BIN_DIR/"
cp "$TARGET_DIR/audio_hal_virtual" "$BIN_DIR/"
cp "$TARGET_DIR/camera_hal_virtual" "$BIN_DIR/"

chmod +x "$BIN_DIR"/*

echo "=== Successfully built and staged guest services in $BIN_DIR ==="
ls -la "$BIN_DIR"
