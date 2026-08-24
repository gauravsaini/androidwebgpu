//! Death recipient tracking and death notification dispatching.

use crate::table::{ClientId, HandleId};
use std::sync::{Arc, Mutex};

/// Represents a death event notification fired when a handle or client terminates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeathNotification {
    /// Client ID where the death occurred or was observed.
    pub client_id: ClientId,
    /// Handle ID that died.
    pub handle: HandleId,
    /// Cookie passed during registration.
    pub cookie: u64,
}

/// Callback function type for handling death events.
pub type DeathCallbackFn = Arc<dyn Fn(DeathNotification) + Send + Sync + 'static>;

/// Thread-safe registry for managing death notification listeners and dispatched events.
#[derive(Default)]
pub struct DeathRegistry {
    /// Active callback listeners.
    listeners: Mutex<Vec<DeathCallbackFn>>,
    /// History or queue of recorded death notifications.
    history: Mutex<Vec<DeathNotification>>,
}

impl DeathRegistry {
    /// Create a new empty death registry.
    pub fn new() -> Self {
        Self {
            listeners: Mutex::new(Vec::new()),
            history: Mutex::new(Vec::new()),
        }
    }

    /// Register a death event callback listener.
    pub fn add_listener<F>(&self, callback: F)
    where
        F: Fn(DeathNotification) + Send + Sync + 'static,
    {
        let mut list = self.listeners.lock().unwrap();
        list.push(Arc::new(callback));
    }

    /// Dispatch a single death notification to all registered listeners.
    pub fn dispatch(&self, notification: DeathNotification) {
        let listeners = {
            let list = self.listeners.lock().unwrap();
            list.clone()
        };

        {
            let mut hist = self.history.lock().unwrap();
            hist.push(notification.clone());
        }

        for listener in listeners {
            listener(notification.clone());
        }
    }

    /// Dispatch a batch of handle/cookie pairs for a specific client.
    pub fn dispatch_batch(&self, client_id: ClientId, events: &[(HandleId, u64)]) {
        for &(handle, cookie) in events {
            self.dispatch(DeathNotification {
                client_id,
                handle,
                cookie,
            });
        }
    }

    /// Drain all recorded death notifications.
    pub fn drain_history(&self) -> Vec<DeathNotification> {
        let mut hist = self.history.lock().unwrap();
        std::mem::take(&mut *hist)
    }

    /// Return the count of recorded death notifications in history.
    pub fn history_count(&self) -> usize {
        self.history.lock().unwrap().len()
    }

    /// Clear all listeners and history.
    pub fn clear(&self) {
        self.listeners.lock().unwrap().clear();
        self.history.lock().unwrap().clear();
    }
}

impl std::fmt::Debug for DeathRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeathRegistry")
            .field("history_count", &self.history_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_death_registry_dispatch_and_drain() {
        let registry = DeathRegistry::new();
        let count = Arc::new(AtomicUsize::new(0));
        let c_clone = Arc::clone(&count);

        registry.add_listener(move |notif| {
            assert_eq!(notif.client_id, 42);
            c_clone.fetch_add(1, Ordering::SeqCst);
        });

        registry.dispatch(DeathNotification {
            client_id: 42,
            handle: 1,
            cookie: 0x999,
        });

        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(registry.history_count(), 1);

        let drained = registry.drain_history();
        assert_eq!(drained.len(), 1);
        assert_eq!(registry.history_count(), 0);

        registry.dispatch_batch(42, &[(2, 0x100), (3, 0x200)]);
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert_eq!(registry.history_count(), 2);

        registry.clear();
        assert_eq!(registry.history_count(), 0);
    }
}

