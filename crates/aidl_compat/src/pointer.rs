//! Smart pointer types managing strong and weak Binder object references.

use crate::death::DeathRecipient;
use crate::status::Result;
use crate::traits::{IBinder, Interface};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::{Arc, Weak};

/// Strong smart pointer managing ownership of an `IBinder` instance.
#[derive(Clone)]
pub struct SpIBinder(Arc<dyn IBinder>);

impl SpIBinder {
    /// Construct a new `SpIBinder` wrapping a concrete `IBinder` instance.
    pub fn new<B: IBinder + 'static>(binder: B) -> Self {
        Self(Arc::new(binder))
    }

    /// Construct an `SpIBinder` directly from an existing `Arc<dyn IBinder>`.
    pub fn from_arc(binder: Arc<dyn IBinder>) -> Self {
        Self(binder)
    }

    /// Return reference to internal `Arc<dyn IBinder>`.
    pub fn as_arc(&self) -> &Arc<dyn IBinder> {
        &self.0
    }

    /// Consume this wrapper and return the inner `Arc<dyn IBinder>`.
    pub fn into_arc(self) -> Arc<dyn IBinder> {
        self.0
    }

    /// Downgrade this strong reference into a weak reference `WpIBinder`.
    pub fn downgrade(&self) -> WpIBinder {
        WpIBinder(Arc::downgrade(&self.0))
    }

    /// Check whether the remote or local Binder object is still alive.
    pub fn is_binder_alive(&self) -> bool {
        self.0.is_binder_alive()
    }

    /// Ping the Binder object synchronously.
    pub fn ping_binder(&self) -> Result<()> {
        self.0.ping_binder()
    }

    /// Register a `DeathRecipient` to be notified when this Binder dies.
    pub fn link_to_death(&self, recipient: Arc<dyn DeathRecipient>) -> Result<()> {
        self.0.link_to_death(recipient)
    }

    /// Unregister a previously linked `DeathRecipient`.
    pub fn unlink_to_death(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()> {
        self.0.unlink_to_death(recipient)
    }

    /// Perform a Binder transaction.
    pub fn transact(
        &self,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.0.transact(code, flags, data, reply)
    }

    /// Return the class descriptor if available.
    pub fn get_class_descriptor(&self) -> Option<&'static str> {
        self.0.get_class_descriptor()
    }

    /// Return remote handle ID if this is a remote proxy.
    pub fn handle(&self) -> Option<u32> {
        self.0.handle()
    }
}

impl Deref for SpIBinder {
    type Target = dyn IBinder;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl IBinder for SpIBinder {
    fn transact(
        &self,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.0.transact(code, flags, data, reply)
    }

    fn is_binder_alive(&self) -> bool {
        self.0.is_binder_alive()
    }

    fn ping_binder(&self) -> Result<()> {
        self.0.ping_binder()
    }

    fn link_to_death(&self, recipient: Arc<dyn DeathRecipient>) -> Result<()> {
        self.0.link_to_death(recipient)
    }

    fn unlink_to_death(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()> {
        self.0.unlink_to_death(recipient)
    }

    fn as_transactable(&self) -> Option<&dyn crate::traits::Remotable> {
        self.0.as_transactable()
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        self.0.get_class_descriptor()
    }

    fn handle(&self) -> Option<u32> {
        self.0.handle()
    }
}

impl std::fmt::Debug for SpIBinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpIBinder")
            .field("handle", &self.handle())
            .field("descriptor", &self.get_class_descriptor())
            .finish()
    }
}

impl PartialEq for SpIBinder {
    fn eq(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        match (self.handle(), other.handle()) {
            (Some(h1), Some(h2)) => h1 == h2,
            _ => false,
        }
    }
}

impl Eq for SpIBinder {}

impl Hash for SpIBinder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if let Some(h) = self.handle() {
            h.hash(state);
        } else {
            (Arc::as_ptr(&self.0) as *const () as usize).hash(state);
        }
    }
}

impl From<Arc<dyn IBinder>> for SpIBinder {
    fn from(arc: Arc<dyn IBinder>) -> Self {
        Self(arc)
    }
}

impl From<SpIBinder> for Arc<dyn IBinder> {
    fn from(sp: SpIBinder) -> Self {
        sp.0
    }
}

/// Weak smart pointer to an `IBinder` instance.
#[derive(Clone)]
pub struct WpIBinder(Weak<dyn IBinder>);

impl WpIBinder {
    /// Construct a new `WpIBinder` from a weak reference.
    pub fn new(weak: Weak<dyn IBinder>) -> Self {
        Self(weak)
    }

    /// Attempt to upgrade this weak pointer to a strong `SpIBinder`.
    pub fn upgrade(&self) -> Option<SpIBinder> {
        self.0.upgrade().map(SpIBinder)
    }
}

impl std::fmt::Debug for WpIBinder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WpIBinder").finish()
    }
}

/// Strongly typed reference to an AIDL interface implementation.
pub struct Strong<I: ?Sized>(Arc<I>);

impl<I: ?Sized> Strong<I> {
    /// Create a new `Strong<I>` from an `Arc<I>`.
    pub fn new(interface: Arc<I>) -> Self {
        Self(interface)
    }

    /// Extract inner `Arc<I>`.
    pub fn into_inner(self) -> Arc<I> {
        self.0
    }

    /// Return reference to inner `Arc<I>`.
    pub fn as_inner(&self) -> &Arc<I> {
        &self.0
    }
}

impl<I: Interface + ?Sized> Strong<I> {
    /// Return the underlying `SpIBinder` for this interface.
    pub fn as_binder(&self) -> SpIBinder {
        self.0.as_binder()
    }
}

impl<I: ?Sized> Deref for Strong<I> {
    type Target = I;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<I: ?Sized> Clone for Strong<I> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<I: ?Sized> From<Arc<I>> for Strong<I> {
    fn from(arc: Arc<I>) -> Self {
        Self(arc)
    }
}

impl<I: ?Sized> From<Strong<I>> for Arc<I> {
    fn from(strong: Strong<I>) -> Self {
        strong.0
    }
}

impl<I: ?Sized> std::fmt::Debug for Strong<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Strong")
            .field(&(Arc::as_ptr(&self.0) as *const ()))
            .finish()
    }
}
