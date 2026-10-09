# QEMU 11.1.2 wasm64 (TCI) Build Recipe

This document records the exact, reproducible build recipe used by T157 to compile **QEMU 11.1.2** into WebAssembly (`qemu-system-aarch64.wasm`, 55.7 MiB) and successfully boot an ARM64 Linux guest inside headless Chromium to `Run /init as init process` (2026-10-09 16:30 AEDT).

---

## 1. Architectural Principles & Flag Rationale

QEMU's upstream WebAssembly support relies on the **TCG Interpreter (TCI)** and Emscripten's **wasm64 / MEMORY64** lowering mode:

1. **`--cpu=wasm64`**: QEMU 11.1.2 system emulation requires a 64-bit host architecture for 64-bit target guests (`aarch64-softmmu`). Configuring with `wasm32` fails at Meson setup with:
   `QEMU emulator requires a 64-bit CPU host architecture. Only tools may be built for 32-bit.`
2. **`--wasm64-32bit-address-limit`**: Translates to Emscripten flag `-sMEMORY64=2`. This retains 64-bit C pointer representations while lowering the output WebAssembly module to 32-bit linear memory with a 4 GiB maximum address space. This ensures compatibility with standard browser WebAssembly engines.
3. **`--enable-tcg-interpreter` (TCI)**: Upstream QEMU does not have a native WebAssembly JIT compiler backend. TCI executes portable TCG bytecode. On native Apple Silicon M-series hardware, native TCG reached `/init` in 1.06 s; in browser WebAssembly via TCI, the boot completes in ~57.4 s (~54× slowdown, well within expected limits).
4. **`--with-coroutine=wasm`**: QEMU 11.1.2 supports `ucontext`, `sigaltstack`, `windows`, and `wasm`. The historical `fiber` backend used in older forks is rejected by Meson.
5. **`-DEMSCRIPTEN` in CFLAGS**: QEMU 11.1.2's `configure` checks for the bare macro `EMSCRIPTEN`, whereas Emscripten 6.0.12 natively defines `__EMSCRIPTEN__`. Supplying `-DEMSCRIPTEN` ensures host OS detection passes cleanly.
6. **No QEMU source patches**: The build uses clean, unpatched upstream QEMU 11.1.2 source.

---

## 2. Prerequisites & Environment

- **Host OS**: macOS Darwin ARM64 (Apple Silicon)
- **Compiler**: Emscripten SDK 6.0.12 (`/Users/Shared/toolchain/emsdk`)
- **Python Toolchain**: Python 3.14 with Meson 1.11+ and Ninja 1.14+
- **QEMU Source**: QEMU 11.1.2 release tree (`/Users/Shared/toolchain/qemu-11.1.2`)
- **Target Prefix**: `/Users/Shared/toolchain/t157-v5-deps/target`

```sh
source /Users/Shared/toolchain/emsdk/emsdk_env.sh
export PATH="/Users/Shared/toolchain/ninja:${PATH}"
```

---

## 3. Dependency Cross-Compilation for wasm64

All dependencies must be compiled statically for wasm64 using `-m64 -sMEMORY64=2 -DEMSCRIPTEN`. Native Homebrew libraries are incompatible.

### 3.1 zlib 1.3.1
- **Configure**: `emconfigure ./configure --prefix=${DEPS_TARGET} --static`
- **Archive Fix**: macOS native `libtool` rejects wasm object files; run archive with:
  `emmake make AR=emar ARFLAGS=rcs RANLIB=emranlib install`
- **Artifacts**: `libz.a`, `zlib.pc`

### 3.2 Pixman 0.44.2
- **Cross Setup**: Built with Meson using `cross.meson` declaring `emcc`, `emar`, and `wasm64`.
- **Options**: `-Ddefault_library=static -Dtests=disabled -Ddemos=disabled`
- **Artifacts**: `libpixman-1.a`, `pixman-1.pc`

### 3.3 PCRE2 10.44
- **Configure**: `emconfigure ./configure --prefix=${DEPS_TARGET} --disable-shared --enable-static --enable-unicode --disable-pcre2-16 --disable-pcre2-32 --disable-jit CFLAGS="-m64 -sMEMORY64=2 -DEMSCRIPTEN"`
- **Artifacts**: `libpcre2-8.a`, `libpcre2-8.pc`

### 3.4 libffi (Upstream Master, commit `bc553867`)
- **Note**: Stock libffi 3.4.8 only has a wasm32 backend and asserts on struct layout under 64-bit pointers. Upstream master added `src/wasm/ffi.c` and `FFI_WASM64_EMSCRIPTEN`.
- **Build**: Built static `libffi.a` with Emscripten wasm64.
- **Artifacts**: `libffi.a`, `libffi.pc` (version 3.8.0)

### 3.5 GLib 2.84.1
- **Cross Build**: Meson cross-build targeting wasm64 with target prefix pkg-config.
- **Resolver**: Static stub provided for `res_query`.
- **config.h Adjustment**: Meson's probe falsely detects `HAVE_POSIX_SPAWN` and `HAVE_PTHREAD_GETNAME_NP`; remove both definitions from generated `config.h` (matching QEMU's upstream `emsdk-wasm64-cross.docker` recipe).
- **Artifacts**: `libglib-2.0.a`, `libgobject-2.0.a`, `libgio-2.0.a`, `glib-2.0.pc`

### 3.6 libfdt 1.8.1 (from DTC 1.8.1)
- **Compile**: AArch64 system emulation strictly requires FDT support. Compile the 10 source files (`fdt.c`, `fdt_ro.c`, `fdt_wip.c`, `fdt_sw.c`, `fdt_rw.c`, `fdt_strerror.c`, `fdt_empty_tree.c`, `fdt_addresses.c`, `fdt_overlay.c`, `fdt_check.c`):
  ```sh
  emcc -c -m64 -sMEMORY64=2 -DEMSCRIPTEN -I. libfdt/*.c
  emar rcs libfdt.a *.o
  ```
- **Install**: Copied `libfdt.a`, `fdt.h`, `libfdt.h`, `libfdt_env.h`, and `libfdt.pc` to `${DEPS_TARGET}` and linked into the Emscripten sysroot so Meson's `find_library('fdt')` passes.

---

## 4. QEMU 11.1.2 Configure & Build

### 4.1 Cross pkg-config Wrapper
Use `pkg-config-wasm64` (included in this directory) to prioritize the wasm64 target prefix:
```sh
#!/bin/sh
PKG_CONFIG_PATH=/Users/Shared/toolchain/t157-v5-deps/target/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}
export PKG_CONFIG_PATH
exec /opt/homebrew/bin/pkg-config "$@"
```

### 4.2 Configure Command Line
```sh
cd /Users/Shared/toolchain/qemu-11.1.2/build-wasm64-v7f

PKG_CONFIG=/Users/Shared/wt-track176-gitshape/browser-bundle/build/pkg-config-wasm64 \
PKG_CONFIG_LIBDIR=/Users/Shared/toolchain/emsdk/upstream/emscripten/cache/sysroot/local/lib/pkgconfig:/Users/Shared/toolchain/emsdk/upstream/emscripten/cache/sysroot/lib/pkgconfig \
CFLAGS="-DEMSCRIPTEN" \
emconfigure ../configure \
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
```

### 4.3 Build
```sh
emmake make -j4
```
Ninja completes 2,030 steps and links `qemu-system-aarch64.js` and `qemu-system-aarch64.wasm`.

---

## 5. Produced Artifacts on This Mac

- **WASM**: `/Users/Shared/toolchain/qemu-11.1.2/build-wasm64-v7f/qemu-system-aarch64.wasm` (58,398,971 bytes / 55.7 MiB)
- **JS Glue**: `/Users/Shared/toolchain/qemu-11.1.2/build-wasm64-v7f/qemu-system-aarch64.js` (405,940 bytes / 396 KiB)
- **Read-Only Bundle**: `/Users/Shared/wt-track157-qemuwasm/browser-bundle/`
