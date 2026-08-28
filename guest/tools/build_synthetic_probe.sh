#!/bin/sh
# Build synthetic_virtio_probe.c → i686 static ELF for v86 guest initrd
# Produces guest/initrd/system/bin/synthetic_virtio_probe
set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GUEST_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC="$GUEST_DIR/synthetic_virtio_probe.c"
OUT_DIR="$GUEST_DIR/initrd/system/bin"
OUT="$OUT_DIR/synthetic_virtio_probe"

mkdir -p "$OUT_DIR"

# Detect cross C++ compiler
CXX=""
if command -v i686-linux-gnu-g++ >/dev/null 2>&1; then
    CXX="i686-linux-gnu-g++"
elif command -v zig >/dev/null 2>&1; then
    CXX="zig c++ -target x86-linux-musl"
else
    NDK_CXX="$(find /opt/homebrew/Caskroom/android-ndk /opt/android-ndk /usr/local/Caskroom/android-ndk "$HOME/Library/Android/sdk/ndk" "$ANDROID_NDK_HOME" "$NDK_HOME" -name "i686-linux-android*-clang++" -perm +111 2>/dev/null | sort -V | tail -n 1 || true)"
    if [ -n "$NDK_CXX" ] && [ -x "$NDK_CXX" ]; then
        CXX="$NDK_CXX"
    fi
fi

if [ -z "$CXX" ]; then
    echo "[build_synthetic_probe] ERROR: no suitable i686 g++ compiler found"
    exit 1
fi

echo "[build_synthetic_probe] Using CXX=$CXX"
$CXX -static -O2 -D_GNU_SOURCE -o "$OUT" "$SRC"

chmod +x "$OUT"
ls -lh "$OUT"
file "$OUT"
echo "[build_synthetic_probe] SUCCESS: $OUT"
