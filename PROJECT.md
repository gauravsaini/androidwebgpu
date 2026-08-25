# Project: AndroidWebGPU E2E Roundtrip Validation

## Architecture
The AndroidWebGPU architecture bridges guest Android applications running in paravirtualized environments to host Web APIs (WebGPU, WebAudio, WebCodecs, WebRTC/getUserMedia, Generic Sensors) through a bidirectional multi-layer transport:
`APK → framework (PMS/AMS/WMS) → daemon (InputFlinger/SurfaceFlinger) → HAL (Sensors/Audio/Camera) → virtio-binder / dev-binder → host runtime → host Web API bridges → virtio-binder → guest callback/buffer`.

### Key Subsystems:
1. **Transport & Registration**: VINTF manifest (`guest/etc/vintf/device_manifest.xml`), `vintf_validator`, userspace `/dev/binder` ioctl loopers (`binder_sys`), virtio-binder paravirtualized queues (`virtio_binder`), and shared memory buffer pools (`camera_host_rs::CameraBufferPool`, `audio_host_rs::AudioRingBuffer`, `binder_sys::BinderMmapRegion`).
2. **Virtual HALs & Web API Bridges**:
   - `ISensors` (`sensors_hal_virtual`) ↔ `sensor_host_rs` (`devicemotion` / synthetic sensor generator).
   - `IModule` / `IConfig` (`audio_hal_virtual`) ↔ `audio_host_rs` (WebAudio stereo 48kHz playback & mic recording).
   - `ICameraProvider` / `ICameraDeviceSession` (`camera_hal_virtual`) ↔ `camera_host_rs` (`getUserMedia` / WebRTC preview frame injection).
   - `IMediaCodecService` ↔ `media_host_rs` (WebCodecs H.264/H.265 Annex B NALU decode to YUV420).
3. **Resiliency & Framework Lifecycle**:
   - Multi-process Activity lifecycle (`ams_rs`, `wms_rs`, `inputflinger_rs`, `surfaceflinger_gpu_service`).
   - Browser backgrounding / `visibilitychange` / `blur` handling (`index.html`, `src/binder_test_suite.js`).
   - Real-world APK ingestion, parsing, forking, and execution (`pms_rs`, `apk_gpu_analyzer`, `tests_e2e_system_services`).
4. **Browser Test Bench**:
   - `index.html` UI bench and `src/binder_test_suite.js` / `src/test_suite.js` automated one-click test runners with visual badges for all 11 E2E validation milestones.

---

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | VINTF HAL Declarations | Validate target-level 7 declarations and `isDeclared()` for `ISensors`, `IModule`, `ICameraProvider` | M1 | ORIGINAL_REQUEST §R1 |
| 2 | Direct & Virtio-Binder Transport | Wire-accurate Parcel serialization, `/dev/binder` ioctl looper, virtqueue roundtrips | M1 | ORIGINAL_REQUEST §R1 |
| 3 | Shared Buffer Transport | Zero-copy shm buffer pools with frame recycling and no leaks | M1 | ORIGINAL_REQUEST §R1 |
| 4 | Sensors HAL E2E | Host-to-guest sensor event stream with verified sample rates and timestamps | M1 | ORIGINAL_REQUEST §R2 |
| 5 | Audio HAL Playback E2E | Stereo 16-bit 48kHz PCM playback through WebAudio bridge with volume/gain scaling | M1 | ORIGINAL_REQUEST §R2 |
| 6 | Audio HAL Recording E2E | Microphone audio capture into guest PCM buffer | M1 | ORIGINAL_REQUEST §R2 |
| 7 | Camera HAL Preview E2E | Live camera frames delivery to `ICameraDeviceCallback` preview buffers | M1 | ORIGINAL_REQUEST §R2 |
| 8 | Media Decode E2E | H.264 video keyframes decode via WebCodecs bridge to YUV420 frame data | M1 | ORIGINAL_REQUEST §R2 |
| 9 | Concurrency & Process Lifecycle | Multi-threaded stress testing, multi-session window allocations, process crash recovery | M1 | ORIGINAL_REQUEST §R3 |
| 10 | Browser Backgrounding | `visibilitychange` & `blur` event listeners, stream pump pause, FPS drop, clean resume | M2 | ORIGINAL_REQUEST §R3 |
| 11 | Real-World APK Execution | Unity (`unity_cube.apk`) and Godot (`godot_gles2.apk`) ingest, resolve, fork, and attach | M1 | ORIGINAL_REQUEST §R3 |
| 12 | Browser Test Bench UI & JS Suite | Interactive UI tabs, badges, and automated one-click test runners in `index.html` & `src/binder_test_suite.js` | M3 | ORIGINAL_REQUEST §R4 |
| 13 | Final E2E Gates Ledger & Workspace Tests | Complete runnable `GATES.md` ledger and 100% clean `cargo test --workspace` verification | M4 | ORIGINAL_REQUEST §Acceptance Criteria |

---

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Transport & Virtual HALs Verification | Verify R1, R2, R3 (Criteria 1-9, 11) in Rust workspace suites | none | DONE |
| M2 | Browser Backgrounding & Resiliency | Implement visibilitychange/blur lifecycle, stream throttling, and audio focus in browser and host bridges | M1 | DONE |
| M3 | Browser Test Bench UI Integration | Add 11 E2E milestone badges, UI runners, and test suite execution in `index.html` & `src/binder_test_suite.js` | M2 | DONE |
| M4 | E2E Gates Sign-off & Verification | Finalize `GATES.md`, produce `TEST_READY.md`, verify 100% pass across all 11 criteria and workspace | M3 | DONE |

---

## Interface Contracts
### Guest HALs ↔ Host Web API Bridges
- `ISensors` (AIDL `android.hardware.sensors.ISensors`):
  - `GET_SENSORS_LIST`: returns Vector of `SensorInfo`.
  - `ACTIVATE(handle, enabled)`: starts/stops host sensor streaming.
  - `BATCH(handle, sampling_period_ns, max_report_latency_ns)`: configures host sampling rate.
  - `INJECT_SENSOR_DATA(event)`: injects accelerometer/gyroscope samples into guest queue.
- `IModule` (AIDL `android.hardware.audio.core.IModule`):
  - `OPEN_OUTPUT_STREAM(config)`: returns `IStreamOut` for 16-bit 48kHz stereo PCM.
  - `OPEN_INPUT_STREAM(config)`: returns `IStreamIn` for 16-bit 48kHz mono/stereo capture.
  - `SET_MASTER_VOLUME(vol)`, `SET_MASTER_MUTE(mute)`: scales host audio output.
- `ICameraProvider` (AIDL `android.hardware.camera.provider.ICameraProvider`):
  - `GET_CAMERA_DEVICE_LIST`: returns list of camera IDs.
  - `GET_CAMERA_CHARACTERISTICS(id)`: returns metadata (facing, resolution, fps).
  - `CREATE_DEVICE_SESSION(id, callback)`: returns `ICameraDeviceSession` with `process_capture_request`.
- `IMediaCodecService` (AIDL `android.media.IMediaCodecService`):
  - `CREATE_BY_CODEC_NAME(name)`: returns `IMediaCodec` instance.
  - `DEQUEUE_INPUT_BUFFER`, `QUEUE_INPUT_BUFFER`, `DEQUEUE_OUTPUT_BUFFER`, `RELEASE_OUTPUT_BUFFER`.
### Host Browser Bridge ↔ JS Test Bench
- `window.AndroidWebGpu`: Global runtime namespace exposing system services and HAL bridges.
- `window.addEventListener('visibilitychange', ...)`: Dispatches pause/resume to audio/camera/sensor stream pumps.

---

## Code Layout
- `crates/vintf_validator/`: VINTF target-level 7 manifest validation.
- `crates/binder_sys/`: Userspace `/dev/binder` ioctl transport, looper threadpool, and `BinderMmapRegion`.
- `crates/virtio_binder/`: VirtIO split virtqueue descriptor chains and guest/host proxy shims.
- `crates/sensors_hal_virtual/` & `crates/sensor_host_rs/`: Virtual Sensors HAL and Host motion bridge.
- `crates/audio_hal_virtual/` & `crates/audio_host_rs/`: Virtual Audio HAL and WebAudio PCM bridge.
- `crates/camera_hal_virtual/` & `crates/camera_host_rs/`: Virtual Camera HAL, zero-copy buffer pool, and preview bridge.
- `crates/media_host_rs/`: MediaCodec service, Annex B bitstream parser, and WebCodecs decoder bridge.
- `crates/ams_rs/`, `crates/wms_rs/`, `crates/pms_rs/`, `crates/apk_gpu_analyzer/`: Activity Manager, Window Manager, Package Manager, and APK analyzer.
- `crates/tests_e2e_system_services/`: Full-stack system services and HAL integration test suites.
- `index.html`: Browser test bench UI.
- `src/binder_test_suite.js` & `src/test_suite.js`: Browser test suites and test runners.
- `GATES.md`: 11 E2E validation checklist gates ledger.
