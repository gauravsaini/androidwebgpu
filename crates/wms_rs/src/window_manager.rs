//! AIDL `IWindowManager` Interface, Remotable Server Stub, and Client Proxy.

use crate::surface_bridge::SurfaceBridge;
use crate::window_session::{
    get_active_session, register_active_session, WindowSession,
};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};

pub const IWINDOW_MANAGER_DESCRIPTOR: &str = "android.view.IWindowManager";

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod iwindow_manager_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const OPEN_SESSION: u32 = FIRST_CALL_TRANSACTION; // 1
}

// -----------------------------------------------------------------------------
// IWindowManager Trait Definition
// -----------------------------------------------------------------------------

pub trait IWindowManager: Interface + Send + Sync {
    fn open_session(&self, callback: Option<SpIBinder>) -> AidlResult<SpIBinder>;
}

static GLOBAL_NEXT_SESSION_ID: AtomicU32 = AtomicU32::new(1);

// -----------------------------------------------------------------------------
// WindowManagerService Implementation
// -----------------------------------------------------------------------------

pub struct WindowManagerService {
    surface_bridge: Arc<SurfaceBridge>,
    sessions: Arc<RwLock<HashMap<u32, Arc<WindowSession>>>>,
    next_session_id: AtomicU32,
    display_width: i32,
    display_height: i32,
}

impl WindowManagerService {
    /// Create a new WindowManagerService without host compositor.
    pub fn new() -> Self {
        Self {
            surface_bridge: Arc::new(SurfaceBridge::new()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_session_id: AtomicU32::new(1),
            display_width: 1280,
            display_height: 720,
        }
    }

    /// Create a new WindowManagerService with specified `SurfaceBridge`.
    pub fn with_surface_bridge(surface_bridge: Arc<SurfaceBridge>) -> Self {
        Self {
            surface_bridge,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            next_session_id: AtomicU32::new(1),
            display_width: 1280,
            display_height: 720,
        }
    }

    /// Set display metrics.
    pub fn set_display_metrics(&mut self, width: i32, height: i32) {
        self.display_width = width;
        self.display_height = height;
    }

    /// Access surface bridge.
    pub fn surface_bridge(&self) -> &Arc<SurfaceBridge> {
        &self.surface_bridge
    }

    /// Access active session by id.
    pub fn get_session(&self, session_id: u32) -> Option<Arc<WindowSession>> {
        self.sessions.read().unwrap().get(&session_id).cloned()
    }

    /// Internal session opener returning session_id and session Arc.
    pub fn open_session_internal(&self, _callback: Option<SpIBinder>) -> AidlResult<(u32, Arc<WindowSession>)> {
        let session_id = GLOBAL_NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
        let mut session = WindowSession::new(session_id, Arc::clone(&self.surface_bridge));
        session.set_display_metrics(self.display_width, self.display_height);

        let session_arc = Arc::new(session);
        register_active_session(Arc::clone(&session_arc));

        self.sessions
            .write()
            .unwrap()
            .insert(session_id, Arc::clone(&session_arc));

        Ok((session_id, session_arc))
    }

    fn clone_internal(&self) -> Self {
        Self {
            surface_bridge: Arc::clone(&self.surface_bridge),
            sessions: Arc::clone(&self.sessions),
            next_session_id: AtomicU32::new(self.next_session_id.load(Ordering::SeqCst)),
            display_width: self.display_width,
            display_height: self.display_height,
        }
    }
}

impl Default for WindowManagerService {
    fn default() -> Self {
        Self::new()
    }
}

impl Interface for WindowManagerService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IWindowManager for WindowManagerService {
    fn open_session(&self, callback: Option<SpIBinder>) -> AidlResult<SpIBinder> {
        let (_id, session_arc) = self.open_session_internal(callback)?;
        let binder: Arc<dyn IBinder> = session_arc;
        Ok(SpIBinder::from_arc(binder))
    }
}

// -----------------------------------------------------------------------------
// Remotable and IBinder Implementations for WindowManagerService
// -----------------------------------------------------------------------------

impl Remotable for WindowManagerService {
    fn get_class_descriptor() -> &'static str {
        IWINDOW_MANAGER_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf8(Some(IWINDOW_MANAGER_DESCRIPTOR)).unwrap();
                Ok(())
            }
            iwindow_manager_codes::OPEN_SESSION => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IWINDOW_MANAGER_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let has_cb = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let callback = if has_cb {
                    let handle = data
                        .read_u32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    Some(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
                } else {
                    None
                };

                let res = self.open_session_internal(callback);
                match res {
                    Ok((session_id, session_arc)) => {
                        reply.write_status(&Status::ok()).unwrap();
                        let handle = session_arc.as_binder().handle().unwrap_or(session_id);
                        reply
                            .write_u32(handle)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            _ => {
                reply
                    .write_status(&Status::from_status(STATUS_UNKNOWN_TRANSACTION))
                    .unwrap();
                Ok(())
            }
        }
    }
}

impl IBinder for WindowManagerService {
    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn ping_binder(&self) -> AidlResult<()> {
        Ok(())
    }

    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf16(Some(IWINDOW_MANAGER_DESCRIPTOR)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(IWINDOW_MANAGER_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// WindowManagerProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct WindowManagerProxy {
    binder: SpIBinder,
}

impl WindowManagerProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for WindowManagerProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for WindowManagerProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IWindowManager for WindowManagerProxy {
    fn open_session(&self, callback: Option<SpIBinder>) -> AidlResult<SpIBinder> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IWINDOW_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref cb) = callback {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let handle = cb.handle().unwrap_or(1);
            data.write_u32(handle)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder
            .transact(iwindow_manager_codes::OPEN_SESSION, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let handle = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(session_arc) = get_active_session(handle) {
            Ok(SpIBinder::from_arc(session_arc as Arc<dyn IBinder>))
        } else {
            Ok(SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0)))
        }
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `WindowManagerService` with handle 0 ServiceManager as `"window"`.
pub fn register_window_service(service: Arc<WindowManagerService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service("window", SpIBinder::from_arc(binder))
}
