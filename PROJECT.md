# Project: AndroidWebGPU

## Architecture
- Paravirtualized Android 13+ guest running on 32-bit x86 Linux kernel with BinderFS in browser WASM/v86 hypervisor.
- Guest-native Rust system services (`binder_sys`, `pms_rs`, `ams_rs`, `zygote_client`, `wms_rs`, `inputflinger_rs`, `input_channel`).
- Virtual AIDL HAL subsystem (`sensors_hal_virtual`, `audio_hal_virtual`, `camera_hal_virtual`, `media_host_rs`, `vintf_validator`).
- Host WebGPU compositor & swapchain (`surfaceflinger_gpu_service`, `webgpu_compositor`, `webgpu_swapchain`, `virtio_gpu_bridge`).
- Zero-copy shared memory buffer transport (`camera_host_rs`, `audio_host_rs`, `sensor_host_rs`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real v86 Guest Boot Baseline | 9-state hypervisor FSM, kernel decomp, BinderFS mount, ServiceManager ping | M0 | docs/updated_plan.md §0 |
| 2 | VINTF Device Manifest (Target-Level 7) | Target-level 7 manifest declaring ISensors, IModule, ICameraProvider | M1 | docs/updated_plan.md §1 |
| 3 | Direct & Virtio-Binder Transport | Direct ioctl to /dev/binder and VirtIO descriptor queue processing | M2 | docs/updated_plan.md §2 |
| 4 | Shared Buffer Transport & Zero-Copy | SharedArrayBuffer ring buffers and pre-allocated buffer pools | M3 | docs/updated_plan.md §3 |
| 5 | Sensors HAL E2E Streaming | ISensors AIDL HAL + devicemotion host bridge streaming | M4 | docs/updated_plan.md §4 |
| 6 | Audio HAL Playback & Recording | IModule AIDL HAL + 16-bit 48kHz stereo WebAudio ring buffer & mic source | M5 | docs/updated_plan.md §5 |
| 7 | binder-sys Direct ioctl & Threadpool | Raw ioctls (BINDER_WRITE_READ, BINDER_SET_MAX_THREADS) + spawn-before-block looper | M6 | docs/updated_plan.md §6 |
| 8 | pms-rs Binary AXML/ARSC & APK Ingestion | Full chunk parser for AXML/ARSC, resolving activities, permissions, providers | M7 | docs/updated_plan.md §7 |
| 9 | ams-rs & zygote-client Process Lifecycle | Zygote abstract socket fork client, 7-state activity lifecycle, bindApplication IPC | M8 | docs/updated_plan.md §8 |
| 10 | wms-rs Window Sessions & SurfaceControl | Fullscreen window sessions and layer composition handoff to WebGPU | M9 | docs/updated_plan.md §9 |
| 11 | inputflinger-rs & input-channel Subsystem | InputChannel socketpairs, evdev decoding, and synchronous finish ack | M10 | docs/updated_plan.md §10 |
| 12 | ISensors Virtual AIDL HAL | Frozen stable-AIDL ISensors interface with VINTF registration | M11 | docs/updated_plan.md §11 |
| 13 | IModule Virtual Audio AIDL HAL | Stable-AIDL IModule/IStreamOut/IStreamIn with VINTF declaration | M12 | docs/updated_plan.md §12 |
| 14 | ICameraProvider Virtual AIDL HAL | Stable-AIDL ICameraProvider/ICameraDevice with VINTF declaration & YUV420 pool | M13 | docs/updated_plan.md §13 |
| 15 | IMediaCodecService Framework Bridge | Framework-level bridging with WebCodecs Annex-B H.264/H.265 NALU parser | M14 | docs/updated_plan.md §14 |
| 16 | Comprehensive E2E Verification & Gates | All 16 gates in GATES.md passing 100% clean across 30 member crates | M15 | GATES.md |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M0 | v86 Boot Baseline | Real v86 guest boot, kernel, BinderFS, ServiceManager | none | DONE |
| M1 | VINTF Manifest | Target-Level 7 device manifest & validation | M0 | DONE |
| M2 | Virtio-Binder Transport | Raw ioctl & virtqueue descriptor processing | M0, M1 | DONE |
| M3 | Shared Memory Buffers | Audio ring buffers & camera buffer pools | M2 | DONE |
| M4 | Virtual Sensors HAL | ISensors stable-AIDL & devicemotion bridge | M1, M2 | DONE |
| M5 | Virtual Audio HAL | IModule AIDL HAL & WebAudio bridge | M1, M3 | DONE |
| M6 | binder-sys Kernel Transport | BINDER_WRITE_READ & spawn-before-block threadpool | M2 | DONE |
| M7 | pms-rs Package Manager | Binary AXML/ARSC & F-Droid APK ingestion | M6 | DONE |
| M8 | ams-rs & zygote-client | Zygote socket fork & Activity lifecycle | M6, M7 | DONE |
| M9 | wms-rs Window Manager | SurfaceControl session & WebGPU composition | M6, M8 | DONE |
| M10 | inputflinger-rs Input Subsystem | InputChannel socketpairs & evdev dispatching | M6, M9 | DONE |
| M11 | ISensors Production HAL | Frozen AIDL descriptor & VINTF registration | M1, M4 | DONE |
| M12 | IModule Production HAL | Stable-AIDL routing & 48kHz PCM playback/mic | M1, M5 | DONE |
| M13 | ICameraProvider Production HAL | Stable-AIDL YUV420 frame streaming & buffer pool | M1, M3 | DONE |
| M14 | IMediaCodecService Bridge | WebCodecs Annex-B NALU parser & AV sync | M6, M9 | DONE |
| M15 | Full E2E & Gate Certification | 30 crates cargo test + adversarial bench verifier | M0–M14 | DONE |

## Code Layout
- `crates/binder_sys`: Raw ioctl bindings to `/dev/binder` and looper threadpool.
- `crates/pms_rs`: Native package manager with AXML and ARSC parsing.
- `crates/ams_rs`: Native activity manager and process lifecycle state machine.
- `crates/zygote_client`: Zygote abstract Unix socket client for app process forks.
- `crates/wms_rs`: Native window manager and SurfaceControl window sessions.
- `crates/inputflinger_rs`: Input event reader and focus-targeted dispatcher.
- `crates/input_channel`: Low-latency Unix socketpair input communication channel.
- `crates/sensors_hal_virtual`: Virtual ISensors AIDL HAL implementation.
- `crates/audio_hal_virtual`: Virtual IModule / IStreamOut / IStreamIn AIDL HAL implementation.
- `crates/camera_hal_virtual`: Virtual ICameraProvider / ICameraDevice AIDL HAL implementation.
- `crates/media_host_rs`: Framework IMediaCodecService bridge and WebCodecs Annex-B parser.
- `crates/vintf_validator`: VINTF device manifest parser and validator.
- `crates/virtio_binder`: VirtIO queue transport device and descriptor processor.
- `crates/surfaceflinger_gpu_service`: SurfaceFlinger ISurfaceComposer and buffer queue.
- `crates/webgpu_compositor` & `crates/webgpu_swapchain`: WebGPU presentation pipelines.
- `src/v86_guest_manager.js`: v86 browser VM manager and boot lifecycle monitor.
- `guest/initrd/init`: Guest userspace root init script.
- `guest/kernel/android_x86_defconfig`: Linux kernel config for x86 guest.
