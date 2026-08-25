//! Comprehensive End-to-End System Services Integration and Deterministic Gates Test Suite.
//!
//! Integrates:
//! - Package Manager Service (`pms_rs`)
//! - Activity Manager Service (`ams_rs`)
//! - Zygote Process Spawning (`zygote_client`)
//! - Window Manager Service (`wms_rs`)
//! - InputFlinger & InputChannel Transport (`inputflinger_rs`, `input_channel`)
//! - Virtual Sensors HAL & Sensor Host Bridge (`sensors_hal_virtual`, `sensor_host_rs`)
//! - Virtual Audio HAL & Audio Host Bridge (`audio_hal_virtual`, `audio_host_rs`)
//! - Virtual Camera HAL & Camera Host Bridge (`camera_hal_virtual`, `camera_host_rs`)
//! - Media Codec Service & WebCodecs Bridge (`media_host_rs`)
//! - VINTF Device Manifest Validator (`vintf_validator`)
//! - Offloaded SurfaceFlinger GPU Compositor (`surfaceflinger_gpu_service`, `webgpu_compositor`)

pub mod harness;

pub use harness::*;
