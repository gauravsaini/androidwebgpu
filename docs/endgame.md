# §7 Endgame: Real Guest Boot → Real App Pixels

## Problem

Everything visible today is synthetic — JS-drawn placeholder buffers and HTML DOM simulations. No real x86 CPU executes, no real Linux kernel boots, no real app renders pixels through the pipeline.

The real pipeline should be:

```
v86 (x86 CPU) → Linux kernel → virtio-gpu driver → virtqueue rings →
Rust WASM host (VirtioGpuBridge) → SurfaceFlinger compositor → WebGPU canvas
```

## Current State

| Component                    | Status                                                              |
| ---------------------------- | ------------------------------------------------------------------- |
| v86 hypervisor               | Downloaded — `v86.wasm` and `libv86.js` ready                       |
| SeaBIOS / VGA BIOS           | Downloaded — `seabios.bin` and `vgabios.bin` in `bios/`             |
| Linux kernel / ISO           | Downloaded — `linux4.iso` in `guest/build/`                         |
| Dev Server & CSP             | Updated — `serve.py` with `unsafe-eval`, `wasm-unsafe-eval`, COOP/COEP |
| Debug Logs                   | In Progress — structured `[v86]`, `[bridge]`, serial pipe logging   |
| Virtio-GPU wire parser       | ✅ Functional — tested in Rust crates                               |
| SurfaceFlinger compositor    | ✅ Functional — composites layers, presents to WebGPU swapchain     |
| Guest userland               | Pending — compiling i686 binaries                                   |

## Logging & Observability Standard

Every phase must implement structured logging:

- **`[v86]`** — WASM load, memory allocation, BIOS POST, boot milestones.
- **`[bridge]`** — Virtio-GPU commands, buffer swaps, format conversions.
- **`[compositor]`** — Frame composition, layer count, swapchain presentation.
- **`[v86-serial]`** — Dmesg lines and shell output forwarded to logcat panel.

---

## Proposed Changes

### Phase 1: Real Linux Boot on WebGPU Canvas (MVP)

**Goal**: Boot a real 32-bit x86 Linux kernel in v86, stream live serial boot messages (dmesg) to the logcat console panel, and render the Linux framebuffer console on the WebGPU canvas.

```
v86 (WASM + SeaBIOS) → Linux kernel (bzImage/ISO) → /dev/ttyS0 (serial dmesg) → Logcat panel
                                                  → VGA framebuffer → WebGPU Canvas
```

#### Existing Infrastructure Audit

| Component | File:line | Status |
| --- | --- | --- |
| v86 WASM & JS Runtime | `src/v86/` & `bios/` | ✅ `v86.wasm`, `libv86.js`, `seabios.bin`, `vgabios.bin` ready |
| Dev Server & Security Headers | `serve.py:28-35` | ✅ COOP (`same-origin`) & COEP (`require-corp`) enabled for `SharedArrayBuffer` |
| Guest Kernel ISO | `guest/build/linux4.iso` | ⚠️ Available, but embedded kernel config & cmdline unverified |
| Guest Initrd & Script | `guest/initrd/init:1-75` | ✅ Standard boot script emitting milestones to `/dev/ttyS0` |
| Guest Manager State Machine | `src/v86_guest_manager.js:1-645` | ⚠️ Functional serial listeners, but contains synthetic simulation fallback |
| WebGPU Host Canvas & VM Hook | `android.html:120-250` | ⚠️ Canvas initialized; needs direct `V86Starter` screen hookup |

#### Known Gaps

> [!WARNING]
> **G1: `linux4.iso` cmdline configuration.** If the ISO's isolinux bootloader lacks `console=ttyS0`, serial dmesg will not emit to the host serial listener. Fallback: switch boot mode from ISO to `bzImage` + `initrd` where custom kernel arguments (`console=ttyS0 earlyprintk=serial,ttyS0,115200`) can be explicitly passed.

> [!CAUTION]
> **G2: `SharedArrayBuffer` requirement.** v86 multi-threaded/WASM operations require `SharedArrayBuffer`, which browsers block unless served with `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`. Verified in `serve.py`, but must be maintained across all deployment targets.

> [!WARNING]
> **G3: Synthetic boot simulation masking failures.** `simulateBootProgression()` in `v86_guest_manager.js` simulates milestones on timers. It must be disabled when `V86Starter` is active so real kernel boot failures surface immediately.

> [!CAUTION]
> **G4: 32-bit x86 CPU compatibility.** v86 emulates a Pentium Pro/i686 CPU with MMX/SSE2, but lacks SSE3/SSSE3/SSE4.1. Standard Android-x86 kernels crash with `SIGILL`. Phase 1 strictly requires a Buildroot/Alpine minimal i686 kernel compiled with `-march=i686`.

---

#### Sub-phase 1.1: v86 WASM Runtime & BIOS Initialization

**Goal**: Load `v86.wasm`, instantiate SeaBIOS/VGA BIOS, allocate guest RAM buffer, and initialize the virtual machine instance without errors.

> [!IMPORTANT]
> **Kill Signal**: `v86.wasm` compilation fails, `SharedArrayBuffer` is undefined in browser context, or BIOS binary fetch fails (404).

##### [NEW] `guest/download_v86_assets.sh`

Automated script to verify and fetch v86 runtime and BIOS assets:

- Download `v86.wasm` and `libv86.js`
- Download `seabios.bin` and `vgabios.bin`
- Verify SHA256 checksums of BIOS binaries

##### [MODIFY] `android.html`

- Include `<script src="./v86/libv86.js"></script>`
- Wire `V86Starter` config with `bios: { url: "./bios/seabios.bin" }`, `vga_bios: { url: "./bios/vgabios.bin" }`, and `memory_size: 256 * 1024 * 1024`

##### Verification Gates

- **Gate 1.1a**: `v86.wasm` compiles and SeaBIOS POST completes without console errors.
- **Gate 1.1b**: `[v86] [I] VM instantiated with 256MB RAM` logs to debug console.

---

#### Sub-phase 1.2: Guest Kernel Boot & Serial Passthrough

**Goal**: Boot the Linux kernel in v86 and stream all raw serial output (`/dev/ttyS0`) directly to `v86_guest_manager.js`, displaying dmesg lines in the Logcat panel.

> [!IMPORTANT]
> **Kill Signal**: `guestManager.serialLogs.length === 0` after 10 seconds of boot → serial pipe broken or kernel missing `console=ttyS0`. Switch to `bzImage` + `initrd` mode.

##### [MODIFY] `src/v86_guest_manager.js`

- Disable `simulateBootProgression()` when real `emulator` is attached
- Pipe `emulator.add_listener("serial0-output-char", ...)` into line buffer
- Forward parsed dmesg lines to `globalLogcat.append("v86Guest", line)`
- Detect kernel milestones (`Linux version`, `Calibrating delay loop`, `Freeing unused kernel memory`, `init started`)

##### Verification Gates

- **Gate 1.2a**: Real kernel dmesg messages stream into the Logcat panel.
- **Gate 1.2b** (**Serial Gate**): `guestManager.serialLogs.length > 0` in browser console; `uname -a` output captured.

---

#### Sub-phase 1.3: Framebuffer Console Presentation

**Goal**: Present the v86 VGA text/graphic framebuffer console on the host WebGPU canvas.

> [!IMPORTANT]
> **Kill Signal**: Canvas remains black after `init started` milestone or canvas fails to bind to v86 display output adapter.

##### [MODIFY] `android.html` & `src/v86_guest_manager.js`

- Connect v86 screen adapter to render buffer target
- Blit framebuffer updates to main canvas viewport

##### Verification Gates

- **Gate 1.3a**: Linux login prompt or shell cursor visible on the canvas.
- **Gate 1.3b**: Typing characters via guest manager serial/keyboard updates the canvas display.

---

#### Phase 1 Exit Criteria

All Phase 1 gates must pass: **Gate 1.1a, Gate 1.1b, Gate 1.2a, Gate 1.2b, Gate 1.3a, Gate 1.3b.**

> [!IMPORTANT]
> **Highest-risk items**: **G1** (`linux4.iso` missing serial console cmdline) and **G4** (CPU opcode compatibility with v86).

---

### Phase 2: Virtio-GPU Virtqueue Integration

**Goal**: Guest Linux kernel probes a real virtio-gpu PCI device, sends control-queue commands through virtqueue rings, and its framebuffer pixels appear on the WebGPU canvas through SurfaceFlinger.

```
guest kernel (drm/virtio_gpu) → PCI BAR0 I/O → virtqueue rings (guest RAM)
→ v86 I/O hooks → VirtioGpuDevice.js → Rust VirtioGpuBridge (WASM)
→ scanout framebuffer → SurfaceFlinger compositor → WebGPU canvas
```

#### Existing Infrastructure Audit

| Component | File:line | Status |
| --- | --- | --- |
| PCI config space (0x1AF4:0x1050, BAR0 I/O @0xC000, BAR1 MMIO @0xD00000, class 0x03) | `virtio_gpu_device.js:56` | ✅ Written, untested against real guest |
| v86 bus registration (io.register_read/write hooks, cpu.devices.pci.register_device) | `virtio_gpu_device.js:96` | ✅ Written; uses v86 internals — fragile |
| VirtIO Legacy I/O space (features, queue PFN, queue select, notify, status, ISR) | `virtio_gpu_device.js:131-198` | ✅ Written |
| Avail-ring consumer (desc table/avail/used ring addr math, availIdx polling) | `virtio_gpu_device.js:244` | ✅ Written; polling only |
| Descriptor-chain walker (Rust, bytemuck VirtqDesc, MAX_CHAIN_LEN=256, in/out split) | `bridge.rs:598` | ✅ Written + unit tests |
| WASM export `process_virtqueue_descriptor(guest_memory, desc_table, head)` | `wasm.rs:87` | ✅ Exposed |
| JS fallback chain parser (if Rust path unavailable) | `virtio_gpu_device.js:313` | ✅ Written |
| Command decode (resource_create_2d, attach_backing, transfer_to_host_2d, set_scanout, resource_flush, get_display_info) | `bridge.rs` `process_binary_wire_command` | ✅ Exists from Phase 0 work |
| Scanout RGBA readback (BGRX→RGBA swizzle) | `wasm.rs` `get_scanout_framebuffer_rgba` | ✅ Exists |
| Guest memory accessor (3 paths: cpu.memory.buffer, .u8, .raw_memory) | `virtio_gpu_device.js:224-238` | ✅ Handles v86 variants |
| Scanout framebuffer blit (damage rect, OffscreenCanvas worker, cached ImageData) | `virtio_gpu_device.js:403-491` | ✅ Complete |
| Cursor queue | `virtio_gpu_device.js:530-532` | ⚠️ Empty stub |

#### Known Gaps

> [!WARNING]
> **G1: v86 has no native virtio-gpu device.** We hook internal APIs (`cpu.devices.pci.register_device`). If v86 changes internals, this breaks. Fallback: fork `libv86.js` and model device on built-in `virtio_net.js` registration pattern.

> [!WARNING]
> **G2: Legacy virtio interface only.** Device presents revision 0, single I/O BAR, no virtio 1.0 PCI capabilities. Guest kernel **must** enable `CONFIG_VIRTIO_PCI_LEGACY` (or `VIRTIO_PCI` with legacy support) or probe fails silently. Modern kernels default to virtio 1.0 — this is a real risk.

> [!CAUTION]
> **G3: No used-ring write-back and no interrupt injection.** Linux virtio-gpu driver waits on completions; without IRQ the driver stalls after the first command batch. This is the **hardest sub-phase** (2.4).

> [!WARNING]
> **G4: QUEUE_PFN capture unverified.** `ioWrite` offset 0x08 stores PFN but the full queue setup handshake (guest writes queue size, negotiates features, then sets PFN) is not tested with a real driver.

> [!WARNING]
> **G5: Guest kernel config unknown.** `linux4.iso` kernel config is unverified — `CONFIG_DRM_VIRTIO_GPU` may be absent. Must extract `/proc/config.gz` or use a known-good Buildroot kernel.

---

#### Guest Kernel Prerequisite (blocks all of 2.1–2.5)

Before any gate can pass, confirm or build the kernel:

1. **Inspect `linux4.iso`**: `file`, extract config if embedded (`/proc/config.gz`, ikconfig), grep for `DRM_VIRTIO_GPU`, `VIRTIO_PCI`, `VIRTIO_PCI_LEGACY`, `SERIAL_8250_CONSOLE`
2. **If missing any**: build Buildroot i686 bzImage with:
   ```
   CONFIG_VIRTIO=y
   CONFIG_VIRTIO_PCI=y
   CONFIG_VIRTIO_PCI_LEGACY=y
   CONFIG_DRM=y
   CONFIG_DRM_VIRTIO_GPU=y
   CONFIG_FRAMEBUFFER_CONSOLE=y
   CONFIG_SERIAL_8250_CONSOLE=y
   ```
3. **Boot mode switch**: use `bzImage` + `initrd` (cmdline: `console=ttyS0`) instead of ISO so serial is guaranteed — same switch pending from Phase 1 serial probe

---

#### Sub-phase 2.1: Bus Probe (Guest Sees the Device)

**Goal**: Guest kernel's PCI enumeration finds the virtio-gpu device at the registered slot.

##### [MODIFY] `src/virtio_gpu_device.js` → `registerWithV86()`

- Harden across v86 versions (`cpu` / `v86.cpu` paths); assert `pci.register_device` success; log slot
- If public API refuses: fall back to fork of `libv86.js`, model device on built-in `virtio_net.js` registration pattern

##### Verification Gates

- **Gate 2.1a**: serial dmesg shows `virtio-pci 0000:00:xx.0: [1af4:1050] type 0`
- **Gate 2.1b**: `lspci -n` over serial lists `1af4:1050`

> [!IMPORTANT]
> **Kill signal**: Neither line appears after boot → registration path wrong → move to fork approach.

---

#### Sub-phase 2.2: Legacy Common Config Handshake (BAR0 I/O)

**Goal**: Guest driver completes the virtio legacy initialization sequence: feature negotiation, queue setup, DRIVER_OK status.

##### [MODIFY] `src/virtio_gpu_device.js` → `ioRead()` / `ioWrite()`

Implement the full legacy register set:

| Offset | Register | Read/Write | Action |
| --- | --- | --- | --- |
| 0x00 | HOST_FEATURES | R | Return `VIRTIO_GPU_F_VIRGL \| VIRTIO_GPU_F_EDID` |
| 0x04 | GUEST_FEATURES | W | Store guest-accepted features |
| 0x08 | QUEUE_PFN | R/W | **Capture per queue** into `q.pfn` (feeds `:252` ring addr math) |
| 0x0C | QUEUE_NUM | R | Return queue size (256 for control, 16 for cursor) |
| 0x0E | QUEUE_SEL | W | Select active queue |
| 0x10 | QUEUE_NOTIFY | W | Trigger `consumeVirtqueue(val)` |
| 0x12 | DEVICE_STATUS | R/W | Track state machine: `0→ACKNOWLEDGE→DRIVER→FEATURES_OK→DRIVER_OK` |
| 0x13 | ISR_STATUS | R | Return + clear ISR (read-to-clear) |

- Log each status transition: `[bridge] [I] Guest status: ACKNOWLEDGE → DRIVER`
- On status write `0` (reset): zero all queue state

##### Verification Gates

- **Gate 2.2a**: Host log shows guest wrote STATUS transitions `ACKNOWLEDGE` → `DRIVER` → `DRIVER_OK`
- **Gate 2.2b**: Guest wrote `QUEUE_PFN != 0` for queues 0 and 1

> [!IMPORTANT]
> **Kill signal**: Guest writes DRIVER status then resets (writes 0) → feature negotiation failed → check legacy support in kernel (G2).

---

#### Sub-phase 2.3: Control Queue — Guest → Host Commands

**Goal**: Guest virtio-gpu DRM driver sends real GPU commands (GET_DISPLAY_INFO, RESOURCE_CREATE_2D, etc.) through the control virtqueue and the host decodes them.

##### [MODIFY] `src/virtio_gpu_device.js` → `ioWrite()` offset 0x10

- On `QUEUE_NOTIFY` write → call `consumeVirtqueue(q)` immediately (event-driven)
- Keep render-loop poll as backstop for missed notifies

##### [MODIFY] `src/virtio_gpu_device.js` → `processControlQueue()`

Map decoded opcodes onto existing bridge methods:

- `GET_DISPLAY_INFO` → `get_display_info` response buffer (pmodes with resolution)
- `RESOURCE_CREATE_2D` / `ATTACH_BACKING` / `TRANSFER_TO_HOST_2D` / `SET_SCANOUT` / `RESOURCE_FLUSH` → `process_command_packet()` / `handle_binary_packet()` path
- Fill response descriptors: `get_display_info` pmodes, ctrl header `OK_NODATA`

##### Verification Gates

- **Gate 2.3a**: `[bridge]` log shows ≥5 distinct real opcodes from guest (`GET_DISPLAY_INFO` among them) within 10s of DRM probe
- **Gate 2.3b**: `RESOURCE_CREATE_2D` arrives with width/height matching guest fbcon request (e.g. 640×480)

> [!IMPORTANT]
> **Kill signal**: Zero opcodes after `DRIVER_OK` → notify/PFN wiring wrong → back to 2.2.

---

#### Sub-phase 2.4: Completion Path — Host → Guest (Used Ring + IRQ)

**Goal**: Host writes completion entries into the used ring and injects an IRQ so the guest driver knows its commands succeeded. Without this, the driver stalls after the first command batch.

> [!CAUTION]
> This is the **most time-consuming sub-phase**. v86's IRQ injection path is internal and varies across versions. Study how v86's built-in virtio-net device does it.

##### [MODIFY] `crates/virtio_gpu_bridge/src/bridge.rs` → `process_virtqueue_descriptor()`

- Return `out_slices` written bytes (already collected) plus explicit used-ring write helper
- Add `write_used_ring(guest_memory, used_ring_addr, head, len)` utility function

##### [MODIFY] `src/virtio_gpu_device.js` → `consumeVirtqueue()`

After each consumed chain:

1. Write used ring entry: `{id: head_desc_idx, len: written_bytes}` (LE u32 pair)
2. Bump used ring idx (view.setUint16 at usedRingAddr + 2)
3. Set ISR bit 0 (queue interrupt)
4. Inject IRQ via v86: `cpu.device_raise_irq(this.irqLine)`

> [!NOTE]
> Step 1-2 are already implemented in `consumeVirtqueue()` (lines 283-289). Step 3-4 are implemented (lines 300-306). The gap is **verifying this works with a real driver** — timing, memory ordering, and whether v86's `device_raise_irq` actually triggers the guest's interrupt handler.

##### Verification Gates

- **Gate 2.4a**: Guest DRM log advances past `virtio_gpu virtio0: ...` with no `timeout` / `wait_queue` warnings for 30s
- **Gate 2.4b**: `fb0: virtio_gpudrmfb` line appears in serial dmesg

> [!IMPORTANT]
> **Kill signal**: Driver timeouts → IRQ injection broken or used ring layout wrong → instrument v86 interrupt path.

---

#### Sub-phase 2.5: Pixels — Transfer → Scanout → WebGPU Canvas

**Goal**: Guest framebuffer console text appears on the WebGPU canvas, pixel-accurate, at ≥15 FPS.

##### [MODIFY] `crates/virtio_gpu_bridge/src/bridge.rs`

- `TransferToHost2d` payload: honor subrect `x/y/w/h` — copy into resource backing at correct offset (logic partially exists at `ResourceFlush` path)
- On `SET_SCANOUT` + `RESOURCE_FLUSH`: copy resource into `scanout.fb_data`, mark `damage_rect`, run `compose_and_present()` once per flush (not per frame poll)

##### [MODIFY] `src/virtio_gpu_device.js` → `renderScanoutToCanvas()`

- Trigger on `RESOURCE_FLUSH` response (not every consumed descriptor)
- Add dirty tracking: skip blit if scanout backing store unchanged since last present

##### [MODIFY] `src/app_controller.js`

- Remove placeholder `queueAppBufferToSurfaceFlinger()` feed while a real scanout is live
- Keep as fallback behind flag until Gate 2.5c passes

##### [MODIFY] `src/v86_guest_manager.js`

- Add serial milestone detection for virtio-gpu DRM initialization:
  ```
  if (line.includes('virtio_gpudrmfb') || line.includes('fb0: virtio'))
      → recordMilestone(BOOT_MILESTONES.VIRTIO_GPU_READY)
  ```
- Add `VIRTIO_GPU_READY` to milestones enum

##### Verification Gates

- **Gate 2.5a**: Guest fbcon text (kernel messages typed on tty0) readable on WebGPU canvas — screenshot diff vs VGA adapter output matches content
- **Gate 2.5b**: Damage rects logged match guest flush rects (compare `[bridge]` logs against guest-side dirty regions)
- **Gate 2.5c**: ≥15 FPS static console; CPU profile shows <40% main-thread time

> [!IMPORTANT]
> **Kill signal**: Pixels arrive corrupted (swizzle/stride bug) or never (transfer path drops subrects).

---

#### Phase 2 Exit Criteria

All gates must be green: **2.1a, 2.1b, 2.2a, 2.2b, 2.3a, 2.3b, 2.4a, 2.4b, 2.5a, 2.5b, 2.5c.**

Order matters: each kill signal routes back to its own sub-phase, not a rewrite.

> [!IMPORTANT]
> **Highest-risk items**: G2 (legacy virtio — modern Linux drivers need PCI capabilities, our device is revision-0 legacy) and G3 (IRQ — without interrupt injection the guest driver stalls after first command batch). G3 drives sub-phase 2.4 which will consume the most time.

---

### Phase 3: Cross-Compiled Guest Userland

Build real i686 Linux ELF binaries that run inside the v86 guest, register with ServiceManager over real `/dev/binder` ioctl, and respond to binder transactions.

> [!IMPORTANT]
> All crates today compile for the host (macOS/wasm32). Phase 3 requires a second compilation target: `i686-unknown-linux-gnu`. The binder transport layer (`binder_sys`) already has `#[cfg(target_os = "linux")]` guards on real ioctl paths — these are the codepaths that will activate inside the guest.

#### Existing Infrastructure Audit

| Component | Crate | Status | Guest-Portable? |
| --- | --- | --- | --- |
| AIDL trait system | `aidl_compat` | ✅ Complete | ✅ Pure Rust, no platform deps |
| Parcel codec + wire format | `binder_rt` | ✅ Complete | ✅ Pure Rust, no platform deps |
| Kernel binder driver | `binder_sys/driver.rs` | ✅ `LinuxBinderDriver` impl | ✅ Has `#[cfg(target_os = "linux")]` guards for real ioctl |
| IPCThreadState | `binder_sys/ipc_thread_state.rs` | ✅ 17 KB impl | ✅ Uses `BinderDriverBackend` trait — real driver on Linux |
| ProcessState | `binder_sys/process_state.rs` | ✅ `/dev/binder` mmap | ✅ `#[cfg(target_os = "linux")]` for real open+mmap |
| ServiceManager client | `binder_sys/service_manager.rs` | ✅ Full `IServiceManager` | ✅ `get_service`, `add_service`, `list_services` |
| ServiceManager server | `binder_sys/service_manager.rs` | ⚠️ `MockServiceManager` only | ❌ Needs real context manager (BINDER_SET_CONTEXT_MGR ioctl) |
| Kernel transport | `binder_sys/transport.rs` | ✅ `BinderKernelTransport` | ✅ Implements `RemoteTransport` over IPCThreadState |
| PMS (Package Manager) | `pms_rs` | ✅ Full AXML/ARSC parser + IPC | ✅ Registers as `"package"` via `binder_sys::add_service` |
| AMS (Activity Manager) | `ams_rs` | ✅ Full lifecycle + IPC | ✅ Registers as `"activity"` via `binder_sys::add_service` |
| WMS (Window Manager) | `wms_rs` | ✅ SurfaceBridge + sessions | ⚠️ SurfaceBridge depends on `surfaceflinger_gpu_service` (host wgpu) |
| InputFlinger | `inputflinger_rs` | ✅ Evdev + dispatcher | ✅ `EvdevReader` reads from `/dev/input/event*` |
| Virtio-Binder device | `virtio_binder` | ✅ Host-side device | N/A — host only, receives guest binder packets |
| Zygote client | `zygote_client` | ✅ Socket + protocol | ✅ Pure Rust abstract unix socket client |
| Sensors HAL | `sensors_hal_virtual` | ✅ Virtual ISensors | ✅ Pure `aidl_compat` deps |
| Audio HAL | `audio_hal_virtual` | ✅ Virtual IModule | ✅ Pure `aidl_compat` deps |
| Camera HAL | `camera_hal_virtual` | ✅ Virtual ICameraProvider | ✅ Pure `aidl_compat` deps |
| Guest init script | `guest/initrd/init` | ✅ Stub launches | ⚠️ `if [ -x ]` stubs — no real binaries present |
| Initrd build script | `guest/tools/build_initrd.sh` | ✅ cpio+gzip packaging | ⚠️ No cross-compilation step |

#### Known Gaps

> [!WARNING]
> **G1**: Cross-compile toolchain may fail — `binder_sys` uses `libc::ioctl` which needs i686-linux-gnu libc headers. Need `gcc-multilib` or musl cross toolchain.

> [!WARNING]
> **G2**: `binder_sys/driver.rs` has `#[cfg(target_os = "linux")]` but was only tested on x86_64 Linux, not i686. 32-bit ioctl struct layouts may differ (sizeof(long) matches but alignment varies).

> [!CAUTION]
> **G3**: v86 guest has no real `/dev/binder` device. The binder driver is a Linux kernel module (`CONFIG_ANDROID_BINDER_IPC=y`) which the Buildroot kernel likely doesn't include. Must either: (a) build kernel with binder support, or (b) route binder through virtio-binder device to host.

> [!WARNING]
> **G4**: WMS depends on `surfaceflinger_gpu_service` which depends on wgpu — cannot compile for i686-linux-gnu target. Feature gating needed (already documented in 3a).

> [!WARNING]
> **G5**: Static linking vs dynamic — guest initrd is minimal (no libc.so). Must build with `RUSTFLAGS='-C target-feature=+crt-static'` or bundle musl libc.

> [!WARNING]
> **G6**: Guest binaries total size must fit in initrd — if all services + HALs exceed ~50MB, boot time degrades significantly.

---

#### Phase 3a: Cross-Compile Toolchain & Workspace Split

**Goal**: `cargo build --target i686-unknown-linux-gnu` produces real ELF binaries for the guest services. Host-only crates (wgpu, WebGPU) are excluded from the guest build.

> [!IMPORTANT]
> **Kill Signal**: If cross-compile fails on binder_sys → switch to musl target or remove real ioctl, use virtio-binder passthrough only.

**Verification Gates**:
- **Gate 3.1a**: Cross-compile succeeds for all guest crates.
- **Gate 3.1b**: Outputs verified as i686 ELF binaries.
- **Gate 3.1c**: Host tests still pass.

##### [MODIFY] `Cargo.toml` (workspace root)

Add a `guest` feature and conditional workspace members:

```toml
[workspace]
members = [
    # ... existing members ...
    "crates/guest_servicemanager",   # NEW
]

[workspace.metadata.guest]
# Crates that compile for i686-unknown-linux-gnu
target = "i686-unknown-linux-gnu"
members = [
    "crates/aidl_compat",
    "crates/binder_rt",
    "crates/binder_sys",
    "crates/pms_rs",
    "crates/ams_rs",
    "crates/inputflinger_rs",
    "crates/sensors_hal_virtual",
    "crates/audio_hal_virtual",
    "crates/camera_hal_virtual",
    "crates/zygote_client",
    "crates/guest_servicemanager",
]
```

##### [MODIFY] `crates/wms_rs/Cargo.toml`

- Make `surfaceflinger_gpu_service` dependency optional behind a `host` feature
- Guest-mode WMS operates without SurfaceBridge (surfaces are just metadata — guest doesn't composite, host does)

```toml
[features]
default = ["host"]
host = ["surfaceflinger_gpu_service"]

[dependencies]
surfaceflinger_gpu_service = { path = "../surfaceflinger_gpu_service", optional = true }
```

##### [MODIFY] `crates/wms_rs/src/surface_bridge.rs`

- Wrap the entire `SurfaceBridge` impl behind `#[cfg(feature = "host")]`
- Add `#[cfg(not(feature = "host"))]` stub that records surface metadata only (no wgpu)

##### [NEW] `guest/Makefile` or `guest/build_guest_services.sh`

Cross-compilation driver script:

```sh
#!/bin/sh
GUEST_TARGET=i686-unknown-linux-gnu

# Install target (one-time)
rustup target add $GUEST_TARGET

# Build guest services as static-linked binaries
for crate in guest_servicemanager pms_rs ams_rs inputflinger_rs sensors_hal_virtual audio_hal_virtual camera_hal_virtual; do
    cargo build --release --target $GUEST_TARGET -p $crate
done

# Copy binaries into initrd
cp target/$GUEST_TARGET/release/guest_servicemanager guest/initrd/system/bin/servicemanager
cp target/$GUEST_TARGET/release/pms_rs guest/initrd/system/bin/pms_rs
# ... etc
```

##### [MODIFY] `crates/binder_sys/src/driver.rs`

- Verify `LinuxBinderDriver` compiles for `i686-unknown-linux-gnu`
- The existing `#[cfg(target_os = "linux")]` guards should activate — real `open("/dev/binder")`, real `ioctl(BINDER_WRITE_READ)`, real `mmap`
- Add `BINDER_SET_CONTEXT_MGR` ioctl call for servicemanager binary

---

#### Phase 3b: Real ServiceManager Binary (Handle 0)

**Goal**: A real `servicemanager` binary runs as PID 2 inside the guest, opens `/dev/binder`, issues `BINDER_SET_CONTEXT_MGR` ioctl to claim handle 0, and enters a binder looper to serve `getService`/`addService`/`listServices` transactions.

> [!IMPORTANT]
> **Kill Signal**: If binder kernel module absent → all binder goes through virtio-binder to host, skip in-guest servicemanager.

**Verification Gates**:
- **Gate 3.2a**: `servicemanager` starts successfully in the guest.
- **Gate 3.2b**: Handle 0 ping succeeds.
- **Gate 3.2c**: `BINDER_SET_CONTEXT_MGR` ioctl returns OK.

##### [NEW] `crates/guest_servicemanager/`

New crate producing a `[[bin]]` target:

```rust
// src/main.rs
fn main() {
    // 1. Open /dev/binder
    let process = ProcessState::init_with_driver("/dev/binder");
    
    // 2. Claim context manager (handle 0)
    process.become_context_manager();  // BINDER_SET_CONTEXT_MGR ioctl
    
    // 3. Create ServiceManager server
    let sm = Arc::new(ServiceManagerServer::new());
    
    // 4. Register self at handle 0
    process.register_as_binder(sm);
    
    // 5. Enter binder looper
    eprintln!("[servicemanager] context manager ready (handle 0)");
    IPCThreadState::current(|state| state.join_thread_pool(true));
}
```

##### [MODIFY] `crates/binder_sys/src/service_manager.rs`

- Promote `MockServiceManager` to a real `ServiceManagerServer` that:
  - Stores services in `HashMap<String, SpIBinder>`
  - Handles `ADD_SERVICE_TRANSACTION` (code 3) — insert service + return OK
  - Handles `GET_SERVICE_TRANSACTION` (code 1) — lookup + return binder or NAME_NOT_FOUND
  - Handles `LIST_SERVICES_TRANSACTION` (code 4) — return registered names
  - Handles `CHECK_SERVICE_TRANSACTION` (code 2) — non-blocking lookup

##### [MODIFY] `crates/binder_sys/src/process_state.rs`

- Add `become_context_manager()` method:
  ```rust
  pub fn become_context_manager(&self) {
      // ioctl(fd, BINDER_SET_CONTEXT_MGR, 0)
      unsafe { libc::ioctl(self.fd, BINDER_SET_CONTEXT_MGR, 0) };
  }
  ```
- This already has `#[cfg(target_os = "linux")]` — just needs the new method

---

#### Phase 3c: System Service Binaries (PMS, AMS, InputFlinger)

**Goal**: Each system service compiles to a standalone i686 ELF binary, opens `/dev/binder`, calls `addService("package", ...)` / `addService("activity", ...)` on handle 0, and enters a binder looper.

> [!IMPORTANT]
> **Kill Signal**: If any service crashes on startup → check SIGILL (SSE?), check binder fd validity.

**Verification Gates**:
- **Gate 3.3a**: PMS registers as "package".
- **Gate 3.3b**: AMS registers as "activity".
- **Gate 3.3c**: Binder round-trip `getService` works.

##### [NEW] `crates/pms_rs/src/bin/pms_rs.rs`

```rust
fn main() {
    let process = ProcessState::init_with_driver("/dev/binder");
    let pms = Arc::new(PackageManagerService::new());
    
    // Register with ServiceManager (handle 0)
    register_package_service(pms.clone()).expect("Failed to register PMS");
    eprintln!("pms_rs: ready");
    
    IPCThreadState::current(|state| state.join_thread_pool(true));
}
```

##### [NEW] `crates/ams_rs/src/bin/ams_rs.rs`

Same pattern — `register_activity_service()` then join looper.

##### [NEW] `crates/inputflinger_rs/src/bin/inputflinger_rs.rs`

Same pattern — `register_input_service()` then join looper, plus spawn `EvdevReader` thread for `/dev/input/event*`.

##### Service Dependency Order

Services must start in the correct order (init script must enforce):

```mermaid
graph TD
    SM[servicemanager<br/>handle 0] --> PMS[pms_rs<br/>"package"]
    SM --> AMS[ams_rs<br/>"activity"]
    SM --> WMS[wms_rs<br/>"window"]
    SM --> IF[inputflinger_rs<br/>"input"]
    PMS --> AMS
    AMS -.-> ZC[zygote_client]
```

1. `servicemanager` — must be first (claims handle 0)
2. `pms_rs` — no service deps, registers as `"package"`
3. `ams_rs` — depends on PMS for intent resolution, registers as `"activity"`
4. `inputflinger_rs` — no service deps, registers as `"input"`
5. `wms_rs` (guest-mode, no GPU) — registers as `"window"`

##### [MODIFY] `guest/initrd/init`

Replace stub `if [ -x ]` checks with ordered launch + readiness gates:

```sh
# 3. Start ServiceManager
/system/bin/servicemanager &
SM_PID=$!
# Wait for handle 0 to accept transactions
while ! /system/bin/service_check 0 2>/dev/null; do sleep 0.05; done
echo "[init] servicemanager started (handle 0 context manager)" > /dev/ttyS0

# 4. Start system services (ordered)
/system/bin/pms_rs &
sleep 0.1
/system/bin/ams_rs &
sleep 0.1
/system/bin/inputflinger_rs &
sleep 0.1

echo "[init] native Rust services started" > /dev/ttyS0
```

##### [NEW] `crates/guest_servicemanager/src/bin/service_check.rs`

Tiny utility binary — does a binder ping on a given handle, exits 0 if alive, 1 if dead. Used by init for readiness gating:

```rust
fn main() {
    let handle: u32 = std::env::args().nth(1).unwrap().parse().unwrap();
    let process = ProcessState::init_with_driver("/dev/binder");
    let alive = IPCThreadState::current(|s| s.ping(handle)).is_ok();
    std::process::exit(if alive { 0 } else { 1 });
}
```

---

#### Phase 3d: Virtual HAL Daemons

**Goal**: Sensors, Audio, and Camera virtual HALs run as guest binaries, register with ServiceManager, and respond to AIDL transactions from the Android framework.

> [!IMPORTANT]
> **Kill Signal**: HALs are lowest priority — can be deferred to Phase 4.

**Verification Gates**:
- **Gate 3.4a**: At least one HAL registers.
- **Gate 3.4b**: HAL responds to transaction.

##### [NEW] `crates/sensors_hal_virtual/src/bin/sensors_hal_virtual.rs`

```rust
fn main() {
    let process = ProcessState::init_with_driver("/dev/binder");
    let hal = Arc::new(VirtualSensorsHal::new());
    register_sensors_hal(hal).expect("Failed to register Sensors HAL");
    eprintln!("sensors_hal_virtual: ready");
    IPCThreadState::current(|state| state.join_thread_pool(true));
}
```

##### Same pattern for `audio_hal_virtual` and `camera_hal_virtual`

##### [MODIFY] `guest/initrd/init`

Add HAL launches after system services:

```sh
# 5. Start Virtual HAL Daemons
/system/bin/sensors_hal_virtual &
/system/bin/audio_hal_virtual &
/system/bin/camera_hal_virtual &
echo "[init] virtual HALs started" > /dev/ttyS0
```

---

#### Phase 3 Host-Side: Virtio-Binder Routing

While guest services use real `/dev/binder` ioctl internally, the **host** needs to intercept certain binder transactions that cross the guest→host boundary (GPU commands, buffer allocation).

**Verification Gates**:
- **Gate 3.5a**: `virtio-binder` routes GPU handle to host.
- **Gate 3.5b**: Guest-local handles stay in-guest.

##### [MODIFY] `crates/virtio_binder/src/device.rs`

- Add routing rules: if guest sends a binder transaction to handle 10 (`IGraphicBufferProducer`), route to host-side `surfaceflinger_gpu_service`
- If guest sends a transaction to handle 0 (ServiceManager), let it stay in-guest (guest has its own ServiceManager now)
- The `binder_routing` crate already exists — wire it into the virtio-binder device

##### [MODIFY] `crates/binder_routing/`

- Define routing table: which handles are guest-local vs host-routed
- Guest-local: handles 0 (ServiceManager), and all service handles registered via guest `addService`
- Host-routed: handles for GPU buffers, compositor surfaces, and any resource requiring wgpu

---

#### Phase 3 Build Verification

##### Cross-Compile Smoke Test

```sh
# Must produce ELF binaries, not Mach-O or WASM
rustup target add i686-unknown-linux-gnu
cargo build --target i686-unknown-linux-gnu -p guest_servicemanager -p pms_rs -p ams_rs

# Verify binary format
file target/i686-unknown-linux-gnu/debug/guest_servicemanager
# Expected: ELF 32-bit LSB executable, Intel 80386, ...
```

##### Host Test (Mock Driver)

All services must still pass tests with `MockBinderDriver` on the host:

```sh
cargo test -p binder_sys -p pms_rs -p ams_rs -p inputflinger_rs
```

#### Exit Criteria

All of **Gate 3.1a-c**, **Gate 3.2a-c**, and **Gate 3.3a-c** pass. **Gate 3.4** and **Gate 3.5** are optional stretch gates.

> [!IMPORTANT]
> **Highest-risk items**: **G3** (binder kernel module presence) and **G1** (cross-compile toolchain libc alignment).

---

### Phase 4: Android ART + Real App Rendering (Endgame)

> [!WARNING]
> This phase has the highest uncertainty. v86 may not support the CPU features Android ART requires. Sub-phases below are ordered so each one delivers standalone value — if ART proves impossible, Phase 4a alone still proves real guest→host GPU pixel flow.

#### Existing Infrastructure Audit

Before building, inventory what already works:

| Component | Crate / File | Status | Coverage |
| --- | --- | --- | --- |
| GLES2→WebGPU translation | `crates/gles2wgpu/` | ✅ 40+ GL functions | `glDrawArrays`, `glDrawElements`, shaders (GLSL→WGSL), textures, buffers, VAOs, uniforms, viewport, scissor, blend, depth |
| SUBMIT_3D dispatch | `bridge.rs:694-900` | ✅ GLES + Vulkan paths | Opcodes 0x01-0x04 (clear/draw/viewport) + full VK command buffer (begin/end rendering, bind pipeline/vertex/index, draw, draw_indexed, set viewport/scissor) |
| IGraphicBufferProducer | `surfaceflinger_gpu_service/buffer_queue.rs` | ✅ Full slot machine | DEQUEUE/QUEUE/CANCEL with wgpu texture-backed `GraphicBufferSlot` |
| SurfaceBridge (WMS→SF) | `wms_rs/surface_bridge.rs` | ✅ Functional | `allocate_surface()`, `apply_transaction()`, `destroy_surface()` |
| Guest EGL shim | `guest/patches/egl_webgpu.cpp` | ⚠️ Written, not compiled | DRM-based `eglSwapBuffers` → `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH` via `DRM_IOCTL_VIRTGPU_EXECBUFFER` |
| Synthetic app buffer pipe | `app_controller.js:335-416` | 🗑️ To be deleted | `queueAppBufferToSurfaceFlinger()` — JS-side DEQUEUE/QUEUE binder shim |
| Gralloc HAL | — | ❌ Missing | Buffer alloc via virtio-gpu DRM |
| HWComposer HAL | — | ❌ Missing | Frame presentation / vsync |
| libhwui / Skia | — | ❌ Missing | Android's GPU-accelerated 2D renderer |
| ART / Dalvik | — | ❌ Missing | DEX bytecode execution |

#### Known Gaps

> [!WARNING]
> **G1**: `egl_webgpu.cpp` is written but never compiled. Need `i686-linux-gnu` cross-compiler. DRM headers (`drm.h`, `virtgpu_drm.h`) must be available.
>
> **G2**: Guest EGL shim uses hardcoded 1080x1920 resolution. Must match v86 guest framebuffer size (probably 1280x720 or 640x480).
>
> **G3**: No i686 cross-compiler for C/C++ in the project. Need `i686-linux-gnu-gcc` toolchain separate from Rust cross-compile.
>
> **G4**: Skia static lib for `i686-linux-gnu` is not commonly distributed. Building from source requires ~2GB download + long build. Vendor a pre-built binary.

> [!CAUTION]
> **G5**: ART interpreter minimum requirement is SSE2. Some AOSP x86 code has SSE3/SSE4.1 intrinsics scattered in non-JIT paths. SIGILL at runtime is likely.
>
> **G6**: `SUBMIT_3D` opcode encoding in `egl_webgpu.cpp` must match what `bridge.rs execute_submit_3d()` expects (currently only 4 opcodes implemented, Skia needs ~15+).
>
> **G7**: Full pipeline (4d) requires ALL previous phases working. If Phase 2 virtqueue breaks, Phase 4 fails.

---

#### Phase 4a: Guest-Side Graphics HAL Stack (Real Pixels, No ART)

**Goal**: A simple i686 C program inside the v86 guest calls real EGL/GLES2, draws a triangle, and the pixels appear on the WebGPU canvas through the full virtio-gpu pipeline. No Android framework, no ART — just proof that guest GPU commands flow end-to-end.

```
Guest C app → libEGL (egl_webgpu.so) → DRM ioctl → virtio-gpu virtqueue →
Host VirtioGpuBridge → SUBMIT_3D → gles2wgpu → WebGPU canvas
```

> [!IMPORTANT]
> **Kill Signal**: If `egl_webgpu.so` cannot open `/dev/dri/card0` → Phase 2 virtio-gpu failed. Go back.

##### Verification Gates

- **Gate 4.1a**: `egl_webgpu.so` compiles.
- **Gate 4.1b**: `test_triangle` runs in guest.
- **Gate 4.1c**: `SUBMIT_3D` arrives at host.
- **Gate 4.1d**: Blue pixels on canvas.

##### [MODIFY] `guest/patches/egl_webgpu.cpp`

- Add `glClear`, `glClearColor`, `glDrawArrays` stubs that serialize to SUBMIT_3D opcode format (0x01 CLEAR, 0x02 DRAW_ARRAYS)
- Wire `eglSwapBuffers` fence synchronization — current impl fires and forgets, need to wait on `VIRTGPU_EXECBUF_FENCE` fd before returning
- Add `eglChooseConfig` / `eglGetConfigAttrib` stubs (currently missing, some apps query these)

##### [NEW] `guest/patches/gralloc_virtgpu.cpp`

Minimal gralloc HAL that allocates buffers via virtio-gpu DRM:

- `alloc()` → `DRM_IOCTL_VIRTGPU_RESOURCE_CREATE` + `DRM_IOCTL_VIRTGPU_MAP` → mmap'd GEM buffer
- `free()` → `munmap` + `DRM_IOCTL_GEM_CLOSE`
- `lock()` / `unlock()` → mmap pointer management (no cache coherence needed — v86 is single-threaded)
- Expose as `hw_module_t` with `GRALLOC_HARDWARE_MODULE_ID`

##### [NEW] `guest/patches/hwcomposer_virtgpu.cpp`

Minimal HWComposer 2.x HAL:

- `presentDisplay()` → `RESOURCE_FLUSH` on the current scanout resource
- `validateDisplay()` → always accept (single-layer overlay)
- vsync callback — use `timerfd_create` or serial-based tick from host at 60 Hz

##### [NEW] `guest/test_triangle.c`

Proof-of-life test binary (cross-compile `i686-linux-gnu-gcc`):

```c
// EGL init → create surface → glClearColor(0.2, 0.3, 0.8, 1.0) → glClear → eglSwapBuffers
// If blue pixels appear on WebGPU canvas, Phase 4a is complete.
```

##### [MODIFY] `guest/initrd/init`

- Add `test_triangle` launch after service startup (temporary, removed in 4c)
- Set `LD_LIBRARY_PATH=/system/lib` to find `egl_webgpu.so`

##### [MODIFY] `guest/tools/build_initrd.sh`

- Add cross-compilation step for `egl_webgpu.cpp` → `egl_webgpu.so`
- Add cross-compilation step for `gralloc_virtgpu.cpp` → `gralloc.virtgpu.so`
- Add cross-compilation step for `test_triangle.c` → `test_triangle`
- Copy `.so` files into initrd at `/system/lib/`
- Copy `test_triangle` into initrd at `/system/bin/`

---

#### Phase 4b: Skia CPU Fallback Path (Real 2D Rendering, No ART)

**Goal**: Run Skia's CPU rasterizer inside the guest. Skia draws to a memory buffer, guest writes it to virtio-gpu framebuffer via `TRANSFER_TO_HOST_2D`. This avoids the GLES dependency entirely and proves Android's primary 2D renderer works in v86.

```
Skia (CPU backend) → pixel buffer → TRANSFER_TO_HOST_2D → Host scanout → SurfaceFlinger → WebGPU
```

> [!IMPORTANT]
> **Kill Signal**: If Skia can't build for i686 → use simple framebuffer test (`fb_fill`) as fallback.

##### Verification Gates

- **Gate 4.2a**: Skia static lib acquired for i686.
- **Gate 4.2b**: `skia_fb_test` renders to memory.
- **Gate 4.2c**: Pixels appear on canvas via `TRANSFER_TO_HOST_2D`.

> [!NOTE]
> Skia's CPU backend has zero GPU requirements — no SSE3/SSE4 needed. It is the safest path to real rendered pixels inside v86.

##### [NEW] `guest/skia_test/` directory

- Download pre-built Skia static lib for `i686-linux-gnu` (or build from source with `is_official_build=false target_cpu="x86"`)
- Write `skia_fb_test.cpp` that:
  1. Creates an `SkSurface::MakeRasterN32Premul(1280, 720)`
  2. Draws text "Hello from Skia inside v86" with `SkCanvas::drawString()`
  3. Draws a rounded rect, gradient, and shadow (exercises typical Android widget paths)
  4. Reads pixel buffer → writes to `/dev/fb0` or virtio-gpu resource via DRM

##### [MODIFY] `crates/virtio_gpu_bridge/src/bridge.rs`

- Ensure `TransferToHost2d` correctly updates scanout damage for the SurfaceFlinger compositor
- Add frame counter metric: count `TRANSFER_TO_HOST_2D` + `RESOURCE_FLUSH` pairs per second to measure guest FPS

---

#### Phase 4c: Android Runtime (ART or Interpreter Fallback)

**Goal**: Execute real DEX bytecode inside the v86 guest. If full ART JIT is impossible (SSE requirements), fall back to interpreter-only mode.

> [!IMPORTANT]
> **Kill Signal**: If SIGILL on ART boot → fall back to Dalvik 4.4 or skip ART entirely, use Alpine+Firefox alternative.

##### Verification Gates

- **Gate 4.3a**: ART compiles with `-msse2` only.
- **Gate 4.3b**: `dalvikvm` runs `HelloWorld.dex`.
- **Gate 4.3c**: No SIGILL during execution.

> [!CAUTION]
> v86 emulates i686 with SSE2 but **not** SSE3/SSSE3/SSE4.1. ART's JIT compiler (`libart-compiler.so`) uses SSE4.1 intrinsics on x86. The interpreter (`libart.so` with `dalvik.vm.usejit=false`) may work if compiled with `-msse2` only. This must be validated empirically.

##### ART Feasibility Decision Tree

```
1. Build ART with -march=i686 -msse2 (no SSE3+)
   ├── Compiles cleanly? → proceed
   └── SSE3+ intrinsics in source? → patch or #ifdef out
2. Run ART interpreter-only (dalvik.vm.usejit=false)
   ├── Boots? → Phase 4c achieved
   └── SIGILL on boot? → check which instruction, patch
3. If ART is impossible:
   └── Use Dalvik VM from Android 4.4 (pure interpreter, no JIT, no SSE3)
       └── Or: skip Android runtime entirely, use Phase 4b Skia path as endgame
```

##### [NEW] `guest/art/` directory

- Pre-built or cross-compiled ART libraries for i686 (interpreter-only):
  - `libart.so` — core runtime
  - `libdexfile.so` — DEX parser
  - `libnativebridge.so` — native method dispatch
  - `boot.art` / `boot.oat` — pre-compiled boot image
- `dalvikvm` binary — standalone VM launcher (for testing without full Android framework)

##### [MODIFY] `guest/initrd/init`

- Add Zygote launch sequence:
  ```sh
  # Start Zygote (app process forker)
  /system/bin/app_process -Xzygote /system/bin --zygote --start-system-server &
  ```
- Set system properties via `/default.prop`:
  ```
  dalvik.vm.usejit=false
  dalvik.vm.dex2oat-threads=1
  dalvik.vm.heapsize=128m
  ro.zygote=zygote32
  ```

##### [NEW] `guest/test_dex/HelloWorld.dex`

Minimal DEX file that prints to stdout:

```java
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello from ART inside v86");
    }
}
```

Test: `dalvikvm -cp HelloWorld.dex HelloWorld` via serial → output appears in logcat panel.

---

#### Phase 4d: Full Pipeline — App Renders Through Skia→EGL→Host (Endgame)

**Goal**: A real Android app's `View.onDraw()` renders through `libhwui` → Skia → EGL → virtio-gpu → host GLES2→WebGPU → canvas. Delete all synthetic rendering.

```
ART (DEX bytecode) → android.view.View.onDraw()
  → libhwui (ThreadedRenderer / HardwareRenderer)
    → Skia GPU backend (SkiaOpenGLPipeline)
      → GLES2 calls (glDrawArrays, glDrawElements...)
        → egl_webgpu.so (guest EGL shim)
          → DRM ioctl SUBMIT_3D
            → virtio-gpu virtqueue
              → Host VirtioGpuBridge.execute_submit_3d()
                → gles2wgpu GlContext
                  → WebGPU render pass
                    → Canvas pixels
```

> [!IMPORTANT]
> **Kill Signal**: If opcode coverage insufficient for Skia → iteratively add opcodes, this is a long tail.

##### Verification Gates

- **Gate 4.4a**: App `View.onDraw` renders through full pipeline.
- **Gate 4.4b**: `queueAppBufferToSurfaceFlinger` deleted.
- **Gate 4.4c**: Frame rate ≥10 FPS.

##### [MODIFY] `crates/gles2wgpu/src/context.rs`

GLES2 gaps to fill for libhwui/Skia compatibility:

- `glGenFramebuffers` / `glBindFramebuffer` / `glFramebufferTexture2D` — Skia uses FBOs for layer rendering
- `glReadPixels` — Skia reads back for shader effect fallbacks
- `glStencilFunc` / `glStencilOp` / `glStencilMask` — Skia uses stencil buffer for clip paths
- `glPixelStorei` — row alignment for texture uploads
- `glGetIntegerv` / `glGetString` — Skia queries `GL_MAX_TEXTURE_SIZE`, `GL_RENDERER`, `GL_VERSION`
- `glGenerateMipmap` — Skia generates mipmaps for scaled image draws

##### [MODIFY] `crates/virtio_gpu_bridge/src/bridge.rs`

New SUBMIT_3D opcodes needed for full Skia GLES2 path:

| Opcode | GL Call | Priority |
| --- | --- | --- |
| 0x05 | `glEnable` / `glDisable` | High — Skia toggles blend/stencil/scissor constantly |
| 0x06 | `glBlendFunc` / `glBlendFuncSeparate` | High — every composited layer |
| 0x07 | `glStencilFunc` / `glStencilOp` | Medium — clip path rendering |
| 0x08 | `glBindFramebuffer` + `glFramebufferTexture2D` | High — layer FBOs |
| 0x09 | `glTexSubImage2D` | High — glyph atlas updates |
| 0x0A | `glBufferSubData` | Medium — dynamic VBO updates |
| 0x0B | `glCreateShader` + `glShaderSource` + `glCompileShader` + `glLinkProgram` | High — Skia compiles ~20 shader programs |
| 0x0C | `glUniform*` (1f, 2f, 3f, 4f, Matrix4fv) | High — every draw call |
| 0x0D | `glVertexAttribPointer` + `glEnableVertexAttribArray` | High — every draw call |

##### [DELETE] `src/app_controller.js` → `queueAppBufferToSurfaceFlinger()`

- Remove the method entirely (lines 335-416)
- Remove the call site at line 522
- Remove `BinderParcel` / `VirtioBinderFraming` imports if no longer used elsewhere

##### [MODIFY] `src/app_controller.js` → `launchActivity()`

- Remove synthetic framebuffer injection
- Guest-side app rendering now flows through the real pipeline — host only composites what SurfaceFlinger presents

---

#### Phase 4 Alternative: Alpine + X11 + Real Browser

If ART proves impossible in v86 (Phase 4c fails), this path still delivers **real app pixels through the real pipeline**:

| Step | Action |
| --- | --- |
| 1 | Boot Alpine Linux (Buildroot) in v86 — already works from Phase 1 |
| 2 | Install Xorg + `xf86-video-virtio` (uses virtio-gpu DRM) |
| 3 | Install Firefox ESR i686 (Alpine package: `firefox-esr`) |
| 4 | Firefox renders via Skia → GLES2 → virtio-gpu → host WebGPU |
| 5 | Proves: real app, real GPU pipeline, real pixels — just not Android |

> [!TIP]
> This alternative requires zero new code — only a different guest image. All host-side infrastructure (gles2wgpu, SurfaceFlinger, virtio-gpu bridge) is shared with the Android path.

---

#### Exit Criteria

- **Minimum viable (Gates 4.1a-4.1d)**: Guest GPU triangle proves full pipeline.
- **Stretch (Gates 4.2a-4.2c OR 4.3a-4.3c)**: Either Skia CPU or ART works.
- **Endgame (Gates 4.4a-4.4c)**: Full Android app renders.

> [!CAUTION]
> **Highest-risk items**: G5 (ART SSE requirements) and G6 (`SUBMIT_3D` opcode coverage).

---

## Open Questions

> [!IMPORTANT]
> **Q1: v86 CPU feature limitations** — v86 lacks SSE3/SSSE3/SSE4. Can a 32-bit Linux kernel with `CONFIG_X86_32=y` boot without these? Buildroot kernels work, but Android-x86 kernels may not. Should we start with Alpine/Buildroot as proof-of-life?

> [!IMPORTANT]
> **Q2: Virtio-GPU vs VGA framebuffer** — v86 has built-in VGA/SVGA emulation. Should Phase 1 use v86's native screen adapter first (zero integration work) and add virtio-gpu in Phase 2? Or wire virtio-gpu from day 1?

> [!IMPORTANT]
> **Q3: Phase 1 scope** — Should Phase 1 deliver just "Linux console booting on screen" (could be done in 1-2 days), or should it include a minimal GUI (fbterm/framebuffer graphics)?

> [!IMPORTANT]
> **Q4: ART SSE feasibility** — Has anyone built ART interpreter-only with `-msse2` ceiling? If not, should we budget a spike (1-2 days) to attempt the build before committing to Phase 4c, and fall back to Dalvik or the Alpine path early?

> [!IMPORTANT]
> **Q5: Skia build target** — Pre-built Skia static libs for `i686-linux-gnu` are not commonly distributed. Should we maintain a Buildroot package for Skia, or vendor a pre-built binary in `guest/lib/`?

---

## Verification Plan

### Phase 1

- [ ] **Gate 1.1a**: v86 WASM + BIOS load successfully without errors.
- [ ] **Gate 1.2a**: v86 boots real kernel → dmesg appears in serial console → logcat panel shows real boot messages.
- [ ] **Gate 1.3a**: Linux login prompt visible on WebGPU canvas (via v86 screen adapter or virtio-gpu).
- [ ] **Gate 1.3b**: `uname -a` returns real kernel version, not simulated text.
- [ ] **Gate 1.2b** (**Serial gate**): run `guestManager.serialLogs.length` in browser console after boot — must be `>0` to confirm real serial pipe. If `0`, ISO cmdline is missing `console=ttyS0`; switch to `bzimage` + `initrd` boot mode.

**Exit Criteria**: Gates 1.1a, 1.2a, 1.2b, 1.3a, 1.3b all pass.

**Highest-risk items**: 
- G1: Unknown kernel config for `linux4.iso` resulting in silent failure.
- G2: Missing `SharedArrayBuffer` support in production environments.

### Phase 2a — PCI Device Enumeration

- [ ] **Gate 2.1a**: Serial dmesg shows `virtio-pci 0000:00:xx.0: [1af4:1050] type 0`.
- [ ] **Gate 2.1b**: `lspci -n` over serial lists `1af4:1050` at slot 05.
- [ ] `/dev/dri/card0` and `fb0: virtio_gpudrmfb` created in guest.

### Phase 2b — Legacy Common Config & Control Queue

- [ ] **Gate 2.2a**: Host logs show guest STATUS transitions `ACKNOWLEDGE` → `DRIVER` → `DRIVER_OK`.
- [ ] **Gate 2.2b**: Guest writes `QUEUE_PFN != 0` for queues 0 and 1.
- [ ] **Gate 2.3a**: `[bridge]` logs show ≥5 distinct opcodes (`GET_DISPLAY_INFO`, `RESOURCE_CREATE_2D`, etc.).
- [ ] **Gate 2.3b**: `RESOURCE_CREATE_2D` dimensions match requested screen geometry.

### Phase 2c — Completion Path & Continuous Framebuffer Presentation

- [ ] **Gate 2.4a**: Guest DRM driver runs without timeout warnings for >30s.
- [ ] **Gate 2.4b**: `fb0: virtio_gpudrmfb` line appears in serial dmesg.
- [ ] **Gate 2.5a**: Guest fbcon console text readable on WebGPU canvas.
- [ ] **Gate 2.5b**: Logged damage rects match guest dirty region flushes.
- [ ] **Gate 2.5c**: Frame rate ≥15 FPS for static console; CPU usage <40%.

### Phase 3a — Cross-Compile Toolchain

- [ ] **Gate 3.1a**: `cargo build --target i686-unknown-linux-gnu -p guest_servicemanager` succeeds
- [ ] **Gate 3.1b**: `file target/i686-unknown-linux-gnu/debug/guest_servicemanager` → `ELF 32-bit LSB executable, Intel 80386`
- [ ] **Gate 3.1c**: Host tests still pass: `cargo test -p binder_sys -p pms_rs -p ams_rs`

### Phase 3b — ServiceManager (Handle 0)

- [ ] **Gate 3.2a**: `servicemanager` binary starts inside v86 guest (verify: `[init] servicemanager started` in serial)
- [ ] **Gate 3.2b**: `service_check 0` exits 0 — handle 0 responds to binder ping
- [ ] **Gate 3.2c**: `BINDER_SET_CONTEXT_MGR` ioctl succeeds (no `errno` in serial log)

### Phase 3c — System Services

- [ ] **Gate 3.3a**: `pms_rs` registers as `"package"` — `service list` (via serial) shows `package: [android.content.pm.IPackageManager]`
- [ ] **Gate 3.3b**: `ams_rs` registers as `"activity"` — `service list` shows `activity: [android.app.IActivityManager]`
- [ ] **Gate 3.3c**: Binder round-trip works: guest-side `getService("package")` returns a valid handle

### Phase 3d — Virtual HALs

- [ ] **Gate 3.4a**: At least one HAL registers (e.g. `sensors_hal_virtual` registers).
- [ ] **Gate 3.4b**: HAL responds to transaction (e.g. responds to `getSensorsList`).

### Phase 3 Host-Side — Virtio-Binder Routing

- [ ] **Gate 3.5a**: `virtio-binder` routes GPU handle to host.
- [ ] **Gate 3.5b**: Guest-local handles stay in-guest.

### Phase 4a — Guest GPU Triangle

- [ ] **Gate 4.1a**: `egl_webgpu.so` compiles successfully.
- [ ] **Gate 4.1b**: `test_triangle` runs inside v86 guest. `egl_webgpu.so` opens `/dev/dri/card0` successfully.
- [ ] **Gate 4.1c**: `SUBMIT_3D` packets arrive at host `execute_submit_3d()` (verify via `[bridge] [D]` log).
- [ ] **Gate 4.1d**: Blue triangle pixels appear on WebGPU canvas. `queueAppBufferToSurfaceFlinger()` is NOT called.

### Phase 4b — Skia CPU

- [ ] **Gate 4.2a**: Skia static lib acquired for i686.
- [ ] **Gate 4.2b**: `skia_fb_test` rasterizes text to memory buffer. Rounded rect + gradient + shadow render correctly.
- [ ] **Gate 4.2c**: Buffer appears on WebGPU canvas via `TRANSFER_TO_HOST_2D` path.

### Phase 4c — Android Runtime

- [ ] **Gate 4.3a**: ART compiles with `-msse2` only.
- [ ] **Gate 4.3b**: `dalvikvm` runs `HelloWorld.dex`.
- [ ] **Gate 4.3c**: No SIGILL during execution.

### Phase 4d — Full Pipeline

- [ ] **Gate 4.4a**: App `View.onDraw` renders through full pipeline.
- [ ] **Gate 4.4b**: `queueAppBufferToSurfaceFlinger` is deleted.
- [ ] **Gate 4.4c**: Frame rate ≥10 FPS.
- [ ] **Gate 4.4d**: SurfaceFlinger composites ≥2 real guest-rendered layers simultaneously.
- [ ] **Gate 4.4e**: Host `gles2wgpu` processes ≥5 distinct GL call types per frame (clear, bindTexture, bindBuffer, drawArrays, uniform).