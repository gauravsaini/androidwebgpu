//! Test environment harness wiring together all Android system services and virtual HALs.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use ams_rs::ActivityManagerService;
use audio_hal_virtual::AudioModuleService;
use audio_host_rs::AudioHostBridge;
use camera_hal_virtual::{CameraDeviceService, CameraProviderService};
use inputflinger_rs::InputManagerService;
use media_host_rs::MediaCodecService;
use pms_rs::service::{IPackageManager, PackageManagerClient};
use pms_rs::PackageManagerService;
use sensor_host_rs::SensorHostBridge;
use sensors_hal_virtual::SensorsHalService;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use wms_rs::{SurfaceBridge, WindowManagerService};
use zygote_client::socket::ZygoteClient;
use zygote_client::MockZygoteHandler;

/// Fully-integrated AndroidWebGPU System Services and Virtual HAL Test Environment.
pub struct SystemServicesHarness {
    pub pms: Arc<PackageManagerService>,
    pub pms_client: Arc<dyn IPackageManager>,
    pub zygote_client: Arc<ZygoteClient>,
    pub zygote_handler: Arc<MockZygoteHandler>,
    pub ams: Arc<ActivityManagerService>,
    pub surface_bridge: Arc<SurfaceBridge>,
    pub wms: Arc<WindowManagerService>,
    pub input_service: Arc<InputManagerService>,
    pub sensors_service: Arc<SensorsHalService>,
    pub sensor_bridge: Arc<SensorHostBridge>,
    pub audio_service: Arc<AudioModuleService>,
    pub audio_bridge: Arc<AudioHostBridge>,
    pub camera_provider: Arc<CameraProviderService>,
    pub camera_device: Arc<CameraDeviceService>,
    pub media_service: Arc<MediaCodecService>,
}

impl Default for SystemServicesHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemServicesHarness {
    /// Initialize a new comprehensive system services test environment.
    pub fn new() -> Self {
        let pms = Arc::new(PackageManagerService::new());
        let pms_client: Arc<dyn IPackageManager> = Arc::new(PackageManagerClient::new(SpIBinder::from_arc(
            Arc::clone(&pms) as Arc<dyn IBinder>,
        )));

        let (zygote_client_inst, zygote_handler) = ZygoteClient::new_mock_default();
        let zygote_client = Arc::new(zygote_client_inst);

        let ams = Arc::new(ActivityManagerService::new(
            Arc::clone(&pms_client),
            Arc::clone(&zygote_client),
        ));

        let surface_bridge = Arc::new(SurfaceBridge::new());
        let wms = Arc::new(WindowManagerService::with_surface_bridge(Arc::clone(
            &surface_bridge,
        )));

        let input_service = Arc::new(InputManagerService::new());

        let sensors_service = Arc::new(SensorsHalService::new());
        let sensor_bridge = Arc::new(SensorHostBridge::new(Arc::clone(&sensors_service)));

        let audio_service = Arc::new(AudioModuleService::new());
        let audio_bridge = Arc::new(AudioHostBridge::new(Arc::clone(&audio_service)));

        let camera_provider = Arc::new(CameraProviderService::new());
        let camera_device = Arc::new(CameraDeviceService::new("device@1.0/virtual/0"));

        let media_service = Arc::new(MediaCodecService::new());

        Self {
            pms,
            pms_client,
            zygote_client,
            zygote_handler,
            ams,
            surface_bridge,
            wms,
            input_service,
            sensors_service,
            sensor_bridge,
            audio_service,
            audio_bridge,
            camera_provider,
            camera_device,
            media_service,
        }
    }

    /// Helper to read fixture APK bytes from repository fixture locations.
    pub fn read_fixture_apk(apk_name: &str) -> Vec<u8> {
        let candidate_paths = [
            format!("fixtures/{}", apk_name),
            format!("../../fixtures/{}", apk_name),
            format!("../fixtures/{}", apk_name),
            format!("/Users/ektasaini/Desktop/androidwebgpu/fixtures/{}", apk_name),
        ];

        for p in &candidate_paths {
            if Path::new(p).exists() {
                if let Ok(bytes) = fs::read(p) {
                    return bytes;
                }
            }
        }

        panic!(
            "Fixture APK {} not found in search paths: {:?}",
            apk_name, candidate_paths
        );
    }
}
