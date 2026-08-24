//! Server-side stub dispatch engines and local `Binder` wrapper.

use crate::death::{DeathRecipient, DeathRecipientRegistry};
use crate::pointer::SpIBinder;
use crate::status::{
    Result, Status, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_UNKNOWN_TRANSACTION,
};
use crate::traits::{IBinder, Remotable};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{DUMP_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Features and capabilities configurable for a local Binder stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BinderFeatures {
    /// Whether this binder requires caller security context / SID.
    pub set_requesting_sid: bool,
}

/// Generic local Binder object wrapping a `Remotable` AIDL interface implementation.
pub struct Binder<T: Remotable> {
    remotable: Arc<T>,
    registry: DeathRecipientRegistry,
    alive: AtomicBool,
}

impl<T: Remotable> Binder<T> {
    /// Construct a new raw `Binder<T>`.
    pub fn new_raw(remotable: T) -> Self {
        Self {
            remotable: Arc::new(remotable),
            registry: DeathRecipientRegistry::new(),
            alive: AtomicBool::new(true),
        }
    }

    /// Wrap a remotable service implementation in a local `SpIBinder`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(remotable: T) -> SpIBinder {
        SpIBinder::new(Self::new_raw(remotable))
    }

    /// Wrap a pre-existing `Arc<T>` service implementation in a local `SpIBinder`.
    pub fn new_with_arc(remotable: Arc<T>) -> SpIBinder {
        SpIBinder::new(Self {
            remotable,
            registry: DeathRecipientRegistry::new(),
            alive: AtomicBool::new(true),
        })
    }

    /// Create with specific features.
    pub fn new_with_features(remotable: T, _features: BinderFeatures) -> SpIBinder {
        Self::new(remotable)
    }

    /// Return reference to the underlying remotable instance.
    pub fn remotable(&self) -> &Arc<T> {
        &self.remotable
    }

    /// Manually trigger death on this local stub, executing all linked death recipients.
    pub fn trigger_death(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.registry.notify_death();
    }
}

impl<T: Remotable> IBinder for Binder<T> {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        if !self.is_binder_alive() {
            return Err(Status::from_status(STATUS_DEAD_OBJECT));
        }

        match code {
            PING_TRANSACTION => Ok(()),
            INTERFACE_TRANSACTION => {
                reply
                    .write_utf16(Some(T::get_class_descriptor()))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            DUMP_TRANSACTION => Ok(()),
            _ => self.remotable.on_transact(code, data, reply),
        }
    }

    fn is_binder_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn link_to_death(&self, recipient: Arc<dyn DeathRecipient>) -> Result<()> {
        self.registry.link(recipient, self.is_binder_alive())
    }

    fn unlink_to_death(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()> {
        self.registry.unlink(recipient)
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self.remotable.as_ref())
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(T::get_class_descriptor())
    }
}

/// Abstract transport hook for dispatching remote Binder transactions.
pub trait RemoteTransport: Send + Sync + 'static {
    /// Transact across remote IPC or Virtio boundary.
    fn transact(
        &self,
        handle: u32,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()>;
}

/// Remote Binder handle proxy representing an object residing across a process/VM boundary.
pub struct RemoteBinder {
    handle: u32,
    cookie: u64,
    descriptor: Option<&'static str>,
    registry: DeathRecipientRegistry,
    alive: AtomicBool,
    transport: Option<Arc<dyn RemoteTransport>>,
}

impl RemoteBinder {
    /// Construct a new raw `RemoteBinder`.
    pub fn new_raw(handle: u32, cookie: u64) -> Self {
        Self {
            handle,
            cookie,
            descriptor: None,
            registry: DeathRecipientRegistry::new(),
            alive: AtomicBool::new(true),
            transport: None,
        }
    }

    /// Construct a new remote binder handle with default transport.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(handle: u32, cookie: u64) -> SpIBinder {
        SpIBinder::new(Self::new_raw(handle, cookie))
    }

    /// Construct a raw remote binder with custom descriptor and transport hook.
    pub fn new_raw_with_transport(
        handle: u32,
        cookie: u64,
        descriptor: Option<&'static str>,
        transport: Arc<dyn RemoteTransport>,
    ) -> Self {
        Self {
            handle,
            cookie,
            descriptor,
            registry: DeathRecipientRegistry::new(),
            alive: AtomicBool::new(true),
            transport: Some(transport),
        }
    }

    /// Construct a remote binder handle with custom class descriptor and transport hook.
    pub fn new_with_transport(
        handle: u32,
        cookie: u64,
        descriptor: Option<&'static str>,
        transport: Arc<dyn RemoteTransport>,
    ) -> SpIBinder {
        SpIBinder::new(Self::new_raw_with_transport(
            handle,
            cookie,
            descriptor,
            transport,
        ))
    }

    /// Return cookie value.
    pub fn cookie(&self) -> u64 {
        self.cookie
    }

    /// Manually trigger death on this remote binder proxy.
    pub fn trigger_death(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.registry.notify_death();
    }
}

impl IBinder for RemoteBinder {
    fn transact(
        &self,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        if !self.is_binder_alive() {
            return Err(Status::from_status(STATUS_DEAD_OBJECT));
        }

        if let Some(transport) = &self.transport {
            transport.transact(self.handle, code, flags, data, reply)
        } else {
            match code {
                PING_TRANSACTION => Ok(()),
                INTERFACE_TRANSACTION => {
                    if let Some(desc) = self.descriptor {
                        reply
                            .write_utf16(Some(desc))
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    } else {
                        Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION))
                    }
                }
                _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
            }
        }
    }

    fn is_binder_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn link_to_death(&self, recipient: Arc<dyn DeathRecipient>) -> Result<()> {
        self.registry.link(recipient, self.is_binder_alive())
    }

    fn unlink_to_death(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()> {
        self.registry.unlink(recipient)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        self.descriptor
    }

    fn handle(&self) -> Option<u32> {
        Some(self.handle)
    }
}
