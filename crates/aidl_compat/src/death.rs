//! Death recipient callbacks and notification management.

use crate::status::{Result, Status, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT};
use std::sync::{Arc, Mutex};

/// Trait implemented by objects that receive death notifications when a remote Binder dies.
pub trait DeathRecipient: Send + Sync + 'static {
    /// Called when the remote Binder object has died or disconnected.
    fn binder_died(&self);
}

/// Helper wrapper for closure-based death recipients.
pub struct DeathCallback<F: Fn() + Send + Sync + 'static>(pub F);

impl<F: Fn() + Send + Sync + 'static> DeathRecipient for DeathCallback<F> {
    fn binder_died(&self) {
        (self.0)();
    }
}

/// Thread-safe registry and dispatcher for `DeathRecipient` callbacks.
#[derive(Default)]
pub struct DeathRecipientRegistry {
    recipients: Mutex<Vec<Arc<dyn DeathRecipient>>>,
}

impl DeathRecipientRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            recipients: Mutex::new(Vec::new()),
        }
    }

    /// Link a new death recipient to this registry.
    pub fn link(&self, recipient: Arc<dyn DeathRecipient>, is_alive: bool) -> Result<()> {
        if !is_alive {
            return Err(Status::from_status(STATUS_DEAD_OBJECT));
        }
        let mut list = self.recipients.lock().unwrap();
        list.push(recipient);
        Ok(())
    }

    /// Unlink a previously registered death recipient.
    pub fn unlink(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()> {
        let mut list = self.recipients.lock().unwrap();
        if let Some(pos) = list.iter().position(|r| Arc::ptr_eq(r, recipient)) {
            list.swap_remove(pos);
            Ok(())
        } else {
            Err(Status::from_status(STATUS_BAD_VALUE))
        }
    }

    /// Notify all registered recipients that the binder has died.
    pub fn notify_death(&self) {
        let list = {
            let mut guard = self.recipients.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        for recipient in list {
            recipient.binder_died();
        }
    }

    /// Return the current count of registered recipients.
    pub fn recipient_count(&self) -> usize {
        self.recipients.lock().unwrap().len()
    }

    /// Clear all registered recipients without notifying.
    pub fn clear(&self) {
        self.recipients.lock().unwrap().clear();
    }
}

impl std::fmt::Debug for DeathRecipientRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeathRecipientRegistry")
            .field("recipient_count", &self.recipient_count())
            .finish()
    }
}
