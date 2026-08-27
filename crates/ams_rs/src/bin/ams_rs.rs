//! Standalone ActivityManagerService Daemon for AndroidWebGPU Guest Environment.

use ams_rs::{register_activity_service, ActivityManagerService};
use binder_sys::{IPCThreadState, ProcessState};
use pms_rs::{IPackageManager, PackageManagerClient, PackageManagerService};
use std::sync::Arc;
use zygote_client::ZygoteClient;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");

    // Connect to PMS via ServiceManager or fallback to registered local service
    let pms_client = match PackageManagerClient::from_service_manager() {
        Ok(c) => c,
        Err(_) => {
            let pms_service = Arc::new(PackageManagerService::new());
            let _ = pms_rs::register_package_service(pms_service.clone());
            PackageManagerClient::new(aidl_compat::pointer::SpIBinder::from_arc(
                pms_service as Arc<dyn aidl_compat::traits::IBinder>,
            ))
        }
    };
    let pms: Arc<dyn IPackageManager> = Arc::new(pms_client);

    let zygote = Arc::new(ZygoteClient::new_path(zygote_client::DEFAULT_ZYGOTE_SOCKET_PATH));
    let ams = Arc::new(ActivityManagerService::new(pms, zygote));

    // Register with ServiceManager (handle 0)
    register_activity_service(ams).expect("Failed to register AMS");
    eprintln!("ams_rs: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
