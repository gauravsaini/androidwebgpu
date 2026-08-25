# E2E Test Infra: AndroidWebGPU

## Test Philosophy
- Multi-tier validation: Unit & subsystem tests, Multi-threaded concurrency stress tests, Crash recovery & death recipient tests, Real-world APK binary tests, and In-Browser UI Test Bench tests.
- 100% clean execution across all 30 workspace member crates (`cargo test --workspace`).
- 1:1 validation of all 11 Acceptance Criteria in `ORIGINAL_REQUEST.md`.

## Feature Inventory & Test Mapping
| # | Feature | Requirement | Test Suite / Location | Verification Method |
|---|---------|-------------|-----------------------|---------------------|
| 1 | VINTF Declarations | R1, Criterion 1 | `crates/vintf_validator` | `cargo test -p vintf_validator` |
| 2 | Binder Transport | R1, Criterion 2 | `crates/binder_sys`, `crates/virtio_binder` | `cargo test -p binder_sys -p virtio_binder` |
| 3 | Shared Buffer Pool | R1, Criterion 3 | `crates/camera_host_rs`, `crates/audio_host_rs` | `cargo test -p camera_host_rs -p audio_host_rs` |
| 4 | Sensors HAL E2E | R2, Criterion 4 | `crates/sensors_hal_virtual`, `crates/sensor_host_rs` | `cargo test -p sensors_hal_virtual -p sensor_host_rs` |
| 5 | Audio Playback E2E | R2, Criterion 5 | `crates/audio_hal_virtual`, `crates/audio_host_rs` | `cargo test -p audio_hal_virtual -p audio_host_rs` |
| 6 | Audio Recording E2E | R2, Criterion 6 | `crates/audio_hal_virtual`, `crates/audio_host_rs` | `cargo test -p audio_hal_virtual -p audio_host_rs` |
| 7 | Camera Preview E2E | R2, Criterion 7 | `crates/camera_hal_virtual`, `crates/camera_host_rs` | `cargo test -p camera_hal_virtual -p camera_host_rs` |
| 8 | Media Decode E2E | R2, Criterion 8 | `crates/media_host_rs` | `cargo test -p media_host_rs` |
| 9 | Concurrency/Lifecycle | R3, Criterion 9 | `crates/tests_e2e_system_services`, `crates/ams_rs` | `cargo test -p tests_e2e_system_services` |
| 10 | Browser Backgrounding | R3, Criterion 10 | `src/binder_test_suite.js`, `index.html` | Browser Test Bench UI / JS runner |
| 11 | Real APK Execution | R3, Criterion 11 | `crates/apk_gpu_analyzer`, `crates/tests_e2e_system_services` | `cargo test -p apk_gpu_analyzer -p tests_e2e_system_services` |

## Test Architecture
- **Rust Integration Suites**: `crates/tests_e2e_system_services/tests/`, `crates/tests_e2e_binder/tests/`.
- **Browser JS Test Suite**: `src/binder_test_suite.js`, `src/test_suite.js`.
- **Browser Test Bench UI**: `index.html`.
- **Verification Gates**: `GATES.md`.
