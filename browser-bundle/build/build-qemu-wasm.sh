#!/usr/bin/env bash
# build-qemu-wasm.sh - Reproduce QEMU 11.1.2 wasm64 TCI build for browser execution
#
# Context: T157 produced qemu-system-aarch64.wasm (55.7 MiB) booting ARM64 Linux guest
# in headless Chromium to "Run /init as init process" (2026-10-09 16:30 AEDT).
#
# Prerequisites:
#   - macOS Darwin ARM64 (Apple Silicon)
#   - Emscripten SDK 6.0.12 (at /Users/Shared/toolchain/emsdk)
#   - Python 3 with Meson 1.11+ and Ninja 1.14+
#   - QEMU 11.1.2 source tree (at /Users/Shared/toolchain/qemu-11.1.2)
#   - Offline dependency sources (zlib 1.3.1, pixman 0.44.2, pcre2 10.44,
#     libffi master, glib 2.84.1, dtc/libfdt 1.8.1)

set -euo pipefail

TOOLCHAIN_ROOT="${TOOLCHAIN_ROOT:-/Users/Shared/toolchain}"
EMSDK_DIR="${EMSDK_DIR:-${TOOLCHAIN_ROOT}/emsdk}"
QEMU_SRC="${QEMU_SRC:-${TOOLCHAIN_ROOT}/qemu-11.1.2}"
DEPS_TARGET="${DEPS_TARGET:-${TOOLCHAIN_ROOT}/t157-v5-deps/target}"
BUILD_DIR="${BUILD_DIR:-${QEMU_SRC}/build-wasm64-v7f}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WRAPPER_PKG_CONFIG="${SCRIPT_DIR}/pkg-config-wasm64"

echo "=== [1/4] Setting up Emscripten 6.0.12 environment ==="
if [ -f "${EMSDK_DIR}/emsdk_env.sh" ]; then
  # shellcheck source=/dev/null
  source "${EMSDK_DIR}/emsdk_env.sh"
else
  echo "ERROR: Emscripten SDK not found at ${EMSDK_DIR}" >&2
  exit 1
fi

emcc --version | head -n 1

echo "=== [2/4] Verifying wasm64 dependencies in target prefix ==="
export PKG_CONFIG="${WRAPPER_PKG_CONFIG}"
export PKG_CONFIG_LIBDIR="${EMSDK_DIR}/upstream/emscripten/cache/sysroot/local/lib/pkgconfig:${EMSDK_DIR}/upstream/emscripten/cache/sysroot/lib/pkgconfig"
export PKG_CONFIG_PATH="${DEPS_TARGET}/lib/pkgconfig"

echo "Checking required pkg-config libraries:"
for pkg in zlib pixman-1 libpcre2-8 libffi glib-2.0 libfdt; do
  if "${PKG_CONFIG}" --exists "${pkg}"; then
    echo "  [OK] ${pkg} $("${PKG_CONFIG}" --modversion "${pkg}")"
  else
    echo "  [FAIL] Missing target dependency: ${pkg}" >&2
    echo "  See BUILD_RECIPE.md for step-by-step dependency compilation." >&2
    exit 1
  fi
done

echo "=== [3/4] Configuring QEMU 11.1.2 for wasm64 TCI ==="
mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Note: CFLAGS="-DEMSCRIPTEN" is required because QEMU 11.1.2 configure checks bare EMSCRIPTEN,
# whereas emsdk 6.0.12 natively defines __EMSCRIPTEN__.
CFLAGS="-DEMSCRIPTEN" emconfigure "${QEMU_SRC}/configure" \
  --target-list=aarch64-softmmu \
  --cross-prefix=em \
  --without-default-features \
  --enable-system \
  --disable-tools \
  --disable-guest-agent \
  --disable-docs \
  --disable-vnc \
  --disable-sdl \
  --disable-gtk \
  --disable-opengl \
  --disable-virglrenderer \
  --disable-xen \
  --disable-kvm \
  --disable-capstone \
  --disable-werror \
  --audio-drv-list= \
  --enable-tcg-interpreter \
  --cpu=wasm64 \
  --wasm64-32bit-address-limit \
  --with-coroutine=wasm \
  --static \
  --disable-download

echo "=== [4/4] Building QEMU 11.1.2 (emmake make) ==="
emmake make -j"$(sysctl -n hw.ncpu || echo 4)"

if [ -f "${BUILD_DIR}/qemu-system-aarch64.wasm" ] && [ -f "${BUILD_DIR}/qemu-system-aarch64.js" ]; then
  echo "=== SUCCESS ==="
  ls -lh "${BUILD_DIR}/qemu-system-aarch64.wasm" "${BUILD_DIR}/qemu-system-aarch64.js"
  echo "To package into browser bundle:"
  echo "  cp ${BUILD_DIR}/qemu-system-aarch64.wasm ${BUILD_DIR}/qemu-system-aarch64.js browser-bundle/assets/"
  echo "  cp ${QEMU_SRC}/pc-bios/*.rom browser-bundle/assets/roms/"
else
  echo "ERROR: Build failed to produce qemu-system-aarch64.wasm / js" >&2
  exit 1
fi
