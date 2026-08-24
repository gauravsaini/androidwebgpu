//! Reusable test harness and fixtures for E2E binder integration tests.

use aidl_compat::{
    DeathRecipient, IBinder, Parcel, Remotable, Result as AidlResult,
    Status, TransactionCode, TransactionFlags, STATUS_BAD_VALUE,
};
use binder_handle_bridge::HandleBridge;
use binder_routing::RoutingPolicy;
use binder_rt::wire::PING_TRANSACTION;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use surfaceflinger_gpu_service::SurfaceComposerService;
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::GuestVirtioTransport;

/// Initialize a headless WGPU device and queue for testing.
pub async fn create_test_wgpu_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("E2E Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .ok()?;

    Some((Arc::new(device), Arc::new(queue)))
}

/// Simple echo service for testing IPC transactions.
pub struct EchoService {
    pub call_count: AtomicU32,
    pub last_code: AtomicU32,
}

impl Default for EchoService {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoService {
    pub const DESCRIPTOR: &'static str = "android.os.IEchoService";
    pub const TRANSACTION_ECHO: u32 = 1;
    pub const TRANSACTION_ADD: u32 = 2;
    pub const TRANSACTION_FAIL: u32 = 3;

    pub fn new() -> Self {
        Self {
            call_count: AtomicU32::new(0),
            last_code: AtomicU32::new(0),
        }
    }
}

impl Remotable for EchoService {
    fn get_class_descriptor() -> &'static str {
        Self::DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.last_code.store(code, Ordering::SeqCst);

        match code {
            Self::TRANSACTION_ECHO => {
                let mut offset = 0;
                let text = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_utf8(text.as_deref())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            Self::TRANSACTION_ADD => {
                let mut offset = 0;
                let a = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(a + b)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            Self::TRANSACTION_FAIL => Err(Status::new_service_specific_error(-99, Some("Intentional failure"))),
            PING_TRANSACTION => Ok(()),
            _ => Err(Status::from_status(aidl_compat::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for EchoService {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        self.on_transact(code, data, reply)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(Self::DESCRIPTOR)
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }
}

/// Combined end-to-end offloading test fixture.
pub struct FullStackFixture {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub handle_bridge: Arc<Mutex<HandleBridge>>,
    pub virtio_device: Arc<VirtioBinderDevice>,
    pub sf_service: Arc<SurfaceComposerService>,
    pub routing_policy: RoutingPolicy,
    pub transport: GuestVirtioTransport,
}

impl FullStackFixture {
    pub async fn create() -> Option<Self> {
        let (device, queue) = create_test_wgpu_device().await?;
        let handle_bridge = Arc::new(Mutex::new(HandleBridge::new()));
        let virtio_device = Arc::new(VirtioBinderDevice::new());

        let sf_service = Arc::new(SurfaceComposerService::with_handle_bridge(
            Arc::clone(&device),
            Arc::clone(&queue),
            640,
            480,
            Arc::clone(&handle_bridge),
        ));

        // Register SurfaceComposer at handle 1 in virtio device and handle bridge
        virtio_device.register_service(1, Arc::clone(&sf_service) as Arc<dyn IBinder>);
        handle_bridge
            .lock()
            .unwrap()
            .register_service_with_handle(
                100, // client id 100
                1,
                SurfaceComposerService::DESCRIPTOR,
                Arc::clone(&sf_service) as Arc<dyn IBinder>,
            )
            .ok();

        let mut routing_policy = RoutingPolicy::new_default_local();
        routing_policy.allow_host_offload(SurfaceComposerService::DESCRIPTOR);
        routing_policy.allow_host_offload(EchoService::DESCRIPTOR);

        let transport = GuestVirtioTransport::new_with_device(Arc::clone(&virtio_device));

        Some(Self {
            device,
            queue,
            handle_bridge,
            virtio_device,
            sf_service,
            routing_policy,
            transport,
        })
    }
}
