//! `IServiceManager` client targeting handle 0 and in-memory ServiceManager server stub.

use crate::transport::BinderKernelTransport;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::stub::{Binder, RemoteBinder};
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::wire::FIRST_CALL_TRANSACTION;
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const SERVICE_MANAGER_DESCRIPTOR: &str = "android.os.IServiceManager";

// Standard AOSP IServiceManager transaction codes
pub const GET_SERVICE_TRANSACTION: u32 = FIRST_CALL_TRANSACTION; // 1
pub const CHECK_SERVICE_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 1; // 2
pub const ADD_SERVICE_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 2; // 3
pub const LIST_SERVICES_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 3; // 4
pub const REGISTER_FOR_NOTIFICATIONS_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 4; // 5
pub const UNREGISTER_FOR_NOTIFICATIONS_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 5; // 6
pub const IS_DECLARED_TRANSACTION: u32 = FIRST_CALL_TRANSACTION + 6; // 7

pub const DUMP_FLAG_PRIORITY_CRITICAL: u32 = 1 << 0;
pub const DUMP_FLAG_PRIORITY_HIGH: u32 = 1 << 1;
pub const DUMP_FLAG_PRIORITY_NORMAL: u32 = 1 << 2;
pub const DUMP_FLAG_PRIORITY_DEFAULT: u32 = 1 << 3;
pub const DUMP_FLAG_PRIORITY_ALL: u32 = DUMP_FLAG_PRIORITY_CRITICAL
    | DUMP_FLAG_PRIORITY_HIGH
    | DUMP_FLAG_PRIORITY_NORMAL
    | DUMP_FLAG_PRIORITY_DEFAULT;

/// Interface definition for Android ServiceManager.
pub trait IServiceManager: Interface + Send + Sync {
    /// Retrieve a registered service by name, blocking until available.
    fn get_service(&self, name: &str) -> Result<Option<SpIBinder>>;

    /// Check if a service is registered without blocking.
    fn check_service(&self, name: &str) -> Result<Option<SpIBinder>>;

    /// Register a service under the specified name.
    fn add_service(
        &self,
        name: &str,
        service: SpIBinder,
        allow_isolated: bool,
        dump_priority: u32,
    ) -> Result<()>;

    /// List all registered service names.
    fn list_services(&self, dump_priority: u32) -> Result<Vec<String>>;

    /// Check if an AIDL interface is declared in VINTF manifest.
    fn is_declared(&self, name: &str) -> Result<bool> {
        let _ = name;
        Ok(true)
    }
}

/// Client proxy for `IServiceManager` communicating over Binder handle 0.
pub struct ServiceManagerClient {
    binder: SpIBinder,
}

impl ServiceManagerClient {
    /// Construct client attached to handle 0 using `BinderKernelTransport`.
    pub fn new() -> Self {
        let transport = Arc::new(BinderKernelTransport::new());
        let binder = RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            transport,
        );
        Self { binder }
    }

    /// Construct client with custom `SpIBinder`.
    pub fn with_binder(binder: SpIBinder) -> Self {
        Self { binder }
    }

    /// Access underlying `SpIBinder`.
    pub fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl Default for ServiceManagerClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Interface for ServiceManagerClient {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl IServiceManager for ServiceManagerClient {
    fn get_service(&self, name: &str) -> Result<Option<SpIBinder>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(SERVICE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(GET_SERVICE_TRANSACTION, 0, &data, &mut reply)?;

        let mut offset = 0;
        if let Ok(flat) = reply.read_binder(&mut offset) {
            let handle = flat.handle();
            let transport = Arc::new(BinderKernelTransport::new());
            Ok(Some(RemoteBinder::new_with_transport(handle, 0, None, transport)))
        } else {
            Ok(None)
        }
    }

    fn check_service(&self, name: &str) -> Result<Option<SpIBinder>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(SERVICE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(CHECK_SERVICE_TRANSACTION, 0, &data, &mut reply)?;

        let mut offset = 0;
        if let Ok(flat) = reply.read_binder(&mut offset) {
            let handle = flat.handle();
            let transport = Arc::new(BinderKernelTransport::new());
            Ok(Some(RemoteBinder::new_with_transport(handle, 0, None, transport)))
        } else {
            Ok(None)
        }
    }

    fn add_service(
        &self,
        name: &str,
        service: SpIBinder,
        allow_isolated: bool,
        dump_priority: u32,
    ) -> Result<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(SERVICE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let handle = service.handle().unwrap_or(0);
        data.write_binder(handle, 0)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(allow_isolated)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(dump_priority)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(ADD_SERVICE_TRANSACTION, 0, &data, &mut reply)?;

        Ok(())
    }

    fn list_services(&self, dump_priority: u32) -> Result<Vec<String>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(SERVICE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(dump_priority)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(LIST_SERVICES_TRANSACTION, 0, &data, &mut reply)?;

        let mut offset = 0;
        let count = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let mut list = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            if let Ok(Some(svc_name)) = reply.read_utf8(&mut offset) {
                list.push(svc_name);
            }
        }

        Ok(list)
    }

    fn is_declared(&self, name: &str) -> Result<bool> {
        let mut data = Parcel::new();
        data.write_utf16(Some(SERVICE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(IS_DECLARED_TRANSACTION, 0, &data, &mut reply)?;

        let mut offset = 0;
        reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }
}

/// Retrieve default `ServiceManager` client proxy.
pub fn default_service_manager() -> ServiceManagerClient {
    ServiceManagerClient::new()
}

/// Convenience helper to register a service with ServiceManager.
pub fn add_service(name: &str, service: SpIBinder) -> Result<()> {
    default_service_manager().add_service(name, service, false, DUMP_FLAG_PRIORITY_DEFAULT)
}

/// Convenience helper to lookup a service with ServiceManager.
pub fn get_service(name: &str) -> Result<Option<SpIBinder>> {
    default_service_manager().get_service(name)
}

/// Convenience helper to check if a service is registered with ServiceManager.
pub fn check_service(name: &str) -> Result<Option<SpIBinder>> {
    default_service_manager().check_service(name)
}

// -----------------------------------------------------------------------------
// In-Memory ServiceManager Server Stub (for simulated mock driver execution)
// -----------------------------------------------------------------------------

/// In-memory ServiceManager server implementing `Remotable`.
pub struct MockServiceManager {
    services: RwLock<HashMap<String, SpIBinder>>,
}

/// ServiceManager Server implementation alias.
pub type ServiceManagerServer = MockServiceManager;

impl MockServiceManager {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
        }
    }

    pub fn new_binder() -> SpIBinder {
        Binder::new(Self::new())
    }
}

impl Default for MockServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Interface for MockServiceManager {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            services: RwLock::new(self.services.read().unwrap().clone()),
        })
    }
}

impl Remotable for MockServiceManager {
    fn get_class_descriptor() -> &'static str {
        SERVICE_MANAGER_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            GET_SERVICE_TRANSACTION | CHECK_SERVICE_TRANSACTION => {
                let name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;

                let services = self.services.read().unwrap();
                if let Some(svc) = services.get(&name) {
                    let handle = svc.handle().unwrap_or(0);
                    reply
                        .write_binder(handle, 0)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                } else {
                    reply
                        .write_i32(0)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            ADD_SERVICE_TRANSACTION => {
                let name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;

                let binder_obj = if let Ok(flat) = data.read_binder(&mut offset) {
                    let transport = Arc::new(BinderKernelTransport::new());
                    RemoteBinder::new_with_transport(flat.handle(), flat.cookie, None, transport)
                } else {
                    return Err(Status::from_status(STATUS_BAD_VALUE));
                };

                let _allow_isolated = data.read_bool(&mut offset).unwrap_or(false);
                let _dump_priority = data.read_u32(&mut offset).unwrap_or(0);

                let mut services = self.services.write().unwrap();
                services.insert(name, binder_obj);
                Ok(())
            }
            LIST_SERVICES_TRANSACTION => {
                let _dump_priority = data.read_u32(&mut offset).unwrap_or(0);
                let services = self.services.read().unwrap();
                let keys: Vec<String> = services.keys().cloned().collect();

                reply
                    .write_i32(keys.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for k in keys {
                    reply
                        .write_utf8(Some(&k))
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            IS_DECLARED_TRANSACTION => {
                reply
                    .write_bool(true)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}
