//! Integration tests: Mock ADB sideload flow into guest-image rootfs.
//!
//! Tests the complete Track D pipeline:
//! 1. APK analysis through frozen contracts ([`pathn_contracts::machine::ApkMeta`]).
//! 2. In-memory ADB connection handshake and sync stream negotiation over [`LoopbackSocket`].
//! 3. Streaming file transfer chunking and reception by [`MockAdbServer`].
//! 4. Installation of the APK into [`guest_image::Rootfs`] at `/data/app/<package>/base.apk`.
//! 5. Verification of rootfs file contents, directory hierarchy, and CPIO initramfs serialization.

use guest_image::rootfs::Rootfs;
use pathn_contracts::machine::{ApkMeta, EngineKind};
use std::cell::RefCell;
use std::rc::Rc;
use u10_analyzer::mock_adb::{AdbTargetFs, MockAdbServer};
use u10_analyzer::sideload::{sideload_apk, AdbChannel, InstallResult, SideloadError};
use u10_analyzer::analyze;
use u13_adapters::LoopbackSocket;

/// Adapter connecting [`guest_image::Rootfs`] to [`AdbTargetFs`].
struct RootfsTarget<'a> {
    rootfs: &'a mut Rootfs,
}

impl<'a> RootfsTarget<'a> {
    fn new(rootfs: &'a mut Rootfs) -> Self {
        Self { rootfs }
    }
}

impl<'a> AdbTargetFs for RootfsTarget<'a> {
    fn write_file(&mut self, path: &str, data: Vec<u8>, mode: u32) -> Result<(), String> {
        self.rootfs
            .write_file(path, data, mode)
            .map_err(|e| e.to_string())
    }

    fn file_exists(&self, path: &str) -> bool {
        self.rootfs.has_file(path)
    }

    fn read_file(&self, path: &str) -> Option<&[u8]> {
        self.rootfs.get_file(path)
    }
}

/// Helper that runs the full sideload flow between client and mock ADB server.
fn run_mock_adb_sideload(
    meta: &ApkMeta,
    apk_bytes: &[u8],
    rootfs: &mut Rootfs,
) -> Result<InstallResult, SideloadError> {
    let (mut client_sock, mut server_sock) = LoopbackSocket::pair();

    // Use Rc<RefCell<...>> for the server so the client's peer stepper can borrow-pump it
    let target = RootfsTarget::new(rootfs);
    let server = Rc::new(RefCell::new(MockAdbServer::new(target)));

    let mut channel = AdbChannel::new(&mut client_sock);
    let server_clone = server.clone();
    channel.set_peer_stepper(move || {
        let _ = server_clone.borrow_mut().step(&mut server_sock);
    });

    sideload_apk(meta, apk_bytes, &mut channel)
}

#[test]
fn test_mock_adb_sideload_unity_cube_fixture() {
    let apk_bytes = include_bytes!("../../../fixtures/unity_cube.apk");

    // 1. Contract conformance analysis
    let meta = analyze(apk_bytes).expect("unity_cube.apk must analyze successfully");
    assert_eq!(meta.engine, EngineKind::Unity);
    assert_eq!(meta.package, "com.unknown.androidgpu");

    // 2. Setup guest-image minimal rootfs
    let mut rootfs = Rootfs::new_minimal();
    assert!(rootfs.has_dir("/data/app"));
    assert_eq!(rootfs.file_count(), 0);

    // 3. Sideload APK via mock ADB flow
    let result = run_mock_adb_sideload(&meta, apk_bytes, &mut rootfs)
        .expect("mock-adb sideload of unity_cube must succeed");

    // 4. Assert install result
    assert_eq!(result.package, "com.unknown.androidgpu");
    assert_eq!(result.apk_path, "/data/app/com.unknown.androidgpu/base.apk");
    assert_eq!(result.bytes_installed, apk_bytes.len());

    // 5. Assert rootfs contents and integrity
    assert!(rootfs.has_file("/data/app/com.unknown.androidgpu/base.apk"));
    let installed_bytes = rootfs
        .get_file("/data/app/com.unknown.androidgpu/base.apk")
        .expect("installed APK must be readable");
    assert_eq!(installed_bytes, apk_bytes);

    // 6. Assert initramfs CPIO packing includes the sideloaded APK
    let cpio = rootfs.to_cpio();
    assert!(!cpio.is_empty());
    let restored = Rootfs::from_cpio(&cpio).expect("CPIO unpack must succeed");
    assert!(restored.has_file("/data/app/com.unknown.androidgpu/base.apk"));
    assert_eq!(
        restored.get_file("/data/app/com.unknown.androidgpu/base.apk"),
        Some(apk_bytes.as_ref())
    );
}

#[test]
fn test_mock_adb_sideload_godot_fixture() {
    let apk_bytes = include_bytes!("../../../fixtures/godot_gles2.apk");

    let meta = analyze(apk_bytes).expect("godot_gles2.apk must analyze successfully");
    assert_eq!(meta.engine, EngineKind::Godot);

    let mut rootfs = Rootfs::new_minimal();
    let result = run_mock_adb_sideload(&meta, apk_bytes, &mut rootfs)
        .expect("mock-adb sideload of godot_gles2 must succeed");

    assert_eq!(result.package, meta.package);
    assert_eq!(
        result.apk_path,
        format!("/data/app/{}/base.apk", meta.package)
    );
    assert_eq!(result.bytes_installed, apk_bytes.len());
    assert_eq!(
        rootfs.get_file(&result.apk_path),
        Some(apk_bytes.as_ref())
    );
}

#[test]
fn test_mock_adb_sideload_custom_package_name() {
    let apk_bytes = include_bytes!("../../../fixtures/unity_cube.apk");
    let meta = ApkMeta {
        package: "com.example.pathn.customgame".to_string(),
        engine: EngineKind::Unity,
        gles_version: (3, 0),
    };

    let mut rootfs = Rootfs::new_minimal();
    let result = run_mock_adb_sideload(&meta, apk_bytes, &mut rootfs)
        .expect("sideload of custom package must succeed");

    assert_eq!(
        result.apk_path,
        "/data/app/com.example.pathn.customgame/base.apk"
    );
    assert!(rootfs.has_file("/data/app/com.example.pathn.customgame/base.apk"));
    assert_eq!(
        rootfs.get_file("/data/app/com.example.pathn.customgame/base.apk"),
        Some(apk_bytes.as_ref())
    );

    let app_dirs = rootfs.list_dir("/data/app").expect("list /data/app");
    assert!(app_dirs.contains(&"com.example.pathn.customgame".to_string()));
}

#[test]
fn test_mock_adb_sideload_empty_bytes_rejected() {
    let meta = ApkMeta {
        package: "com.example.empty".to_string(),
        engine: EngineKind::Other,
        gles_version: (2, 0),
    };
    let mut rootfs = Rootfs::new_minimal();
    let err = run_mock_adb_sideload(&meta, b"", &mut rootfs).unwrap_err();
    assert!(matches!(err, SideloadError::InstallFailed(_)));
}

#[test]
fn test_mock_adb_sideload_determinism() {
    let apk_bytes = include_bytes!("../../../fixtures/unity_cube.apk");
    let meta = ApkMeta {
        package: "com.determinism.test".to_string(),
        engine: EngineKind::Unity,
        gles_version: (3, 0),
    };

    let run_once = || {
        let mut fs = Rootfs::new_minimal();
        let _ = run_mock_adb_sideload(&meta, apk_bytes, &mut fs).unwrap();
        fs.to_cpio()
    };

    let cpio_a = run_once();
    let cpio_b = run_once();
    assert_eq!(cpio_a, cpio_b, "sideload into rootfs must be bit-identical across runs");
}
