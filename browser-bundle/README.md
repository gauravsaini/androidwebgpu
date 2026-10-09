# QEMU 11.1.2 wasm64 Browser Execution Bundle

This bundle contains the browser harness, runtime configuration, build recipes, and reproduction instructions for running **upstream QEMU 11.1.2** compiled to WebAssembly (`wasm64` TCI mode) inside modern browsers.

On **2026-10-09 16:30 AEDT**, Track 157 achieved the core milestone for the Bada goal (Android in browser) by booting an unpatched upstream QEMU 11.1.2 wasm64 emulator inside headless Chromium, reaching:
```text
[   57.459359] Run /init as init process
[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)
[PATHN-TRACK-B] STATUS: PASS - USERSPACE INITIALIZATION COMPLETE
```

---

## Architecture & Root Cause Fix

Earlier attempts halted before userspace initialization because QEMU's PCI/virtio device emulation required pc-bios ROM images that were not packaged in standard WebAssembly bundles.

The critical fixes applied:
1. **15 QEMU ROMs packaged**: Packaged all standard bios ROMs (`efi-*.rom`, `pxe-*.rom`, `qboot.rom`).
2. **Emscripten FS Staging**: In `launch.js`, preRun hooks load the ROM array into Emscripten virtual filesystem at both `/` and `/qemu-data/`.
3. **Firmware Path Directive**: Passed `-L /qemu-data` in QEMU's command line arguments.
4. **TCI wasm64 lowered to 32-bit address space**: Built with `--cpu=wasm64 --wasm64-32bit-address-limit --enable-tcg-interpreter --with-coroutine=wasm`, lowering via Emscripten `-sMEMORY64=2` to ensure 4 GiB browser WebAssembly compatibility.

---

## Reproduction Instructions

### Step 1: Stage Binary Assets

Per repository policy, large binaries (.wasm, .rom, kernel images) are excluded from git. On this Mac, copy the pre-built read-only assets into the local bundle directory:

```bash
mkdir -p browser-bundle/assets
cp -R /Users/Shared/wt-track157-qemuwasm/browser-bundle/assets/* browser-bundle/assets/
```

Verify the checksums against `MANIFEST.md`:
```bash
shasum -a 256 browser-bundle/assets/qemu-system-aarch64.wasm
# Expected: 1f958af8543bd6320c3c29b5be2b62780cc3be1d26dc02794cbc9505e98ba676
```

### Step 2: Start Local HTTP Server

Start the included Python server, which provides the requisite `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` headers (required for WebAssembly pthread/SharedArrayBuffer support):

```bash
cd browser-bundle
python3 serve.py 8124
# Alternatively with uv:
# uv run --script serve.py 8124
```

### Step 3: Run the Browser Harness

1. Open Chromium or Chrome to `http://127.0.0.1:8124/`.
2. Click **Start guest** (or trigger `document.querySelector('#start').click()` in automation).
3. The harness fetches `Image`, `initramfs.cpio`, `minimal-virt-fixed.dtb`, and the 15 ROMs into memory, writes them to the Emscripten filesystem, and initializes QEMU.
4. Serial console output streams directly into the `<pre id="serial">` element.

### Step 4: Validate the Checkpoint

- At approximately ~57 seconds into boot, the serial stream reaches:
  `[   57.459359] Run /init as init process`
- The userspace status banner appears:
  `[PATHN-TRACK-B] STATUS: PASS - USERSPACE INITIALIZATION COMPLETE`
- The status element transitions to `data-state="init-seen"` and `window.__INIT_SEEN === true`.

---

## Rebuilding QEMU wasm64 from Source

To compile the WebAssembly module and all dependencies from source:

1. Review the full compilation prerequisites and step-by-step instructions in `build/BUILD_RECIPE.md`.
2. Execute the automated build script:
   ```bash
   ./build/build-qemu-wasm.sh
   ```
3. Compiled artifacts will be placed in `${BUILD_DIR}/qemu-system-aarch64.wasm` and `${BUILD_DIR}/qemu-system-aarch64.js`.

---

## Deliverables & Directory Layout

- `index.html`: Web interface hosting the serial terminal and status controls.
- `launch.js`: Harness logic orchestrating Emscripten FS mounting, ROM staging, and QEMU execution.
- `serve.py`: Multi-threaded HTTP server with COOP/COEP support.
- `build/BUILD_RECIPE.md`: Comprehensive cross-compilation documentation for QEMU 11.1.2 and all dependencies.
- `build/build-qemu-wasm.sh`: Shell script driving the QEMU wasm64 build.
- `build/pkg-config-wasm64`: Pkg-config wrapper prioritizing wasm64 target cross-compilation.
- `evidence/browser-qemuwasm-init-excerpt.log`: 36-line log excerpt showing boot milestone.
- `MANIFEST.md`: Complete SHA-256 and byte-size registry for all assets.
- `.gitignore`: Ensures binary assets (`assets/`) are never committed to git.
