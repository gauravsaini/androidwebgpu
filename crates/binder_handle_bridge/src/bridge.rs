//! Thread-safe handle manager for cross-boundary Binder object translation and lifecycle.

use crate::death::DeathRegistry;
use crate::table::{ClientId, HandleId, HandleTable, HostHandleEntry};
use crate::BridgeError;
use std::sync::{Arc, RwLock};

/// Thread-safe manager for Binder handle allocation, reference counting, and lifecycle.
#[derive(Clone, Default)]
pub struct HandleBridge {
    /// Internal handle table protected by a read-write lock.
    table: Arc<RwLock<HandleTable>>,
    /// Death notification registry and dispatcher.
    death_registry: Arc<DeathRegistry>,
}

impl HandleBridge {
    /// Create a new `HandleBridge` instance with empty table and registry.
    pub fn new() -> Self {
        Self {
            table: Arc::new(RwLock::new(HandleTable::new())),
            death_registry: Arc::new(DeathRegistry::new()),
        }
    }

    /// Register a host service instance for a client.
    ///
    /// If the client already possesses a handle for this exact service pointer, its strong
    /// reference count is incremented by 1 and the existing `HandleId` is returned.
    /// Otherwise, a new sequential `HandleId` is allocated for the client.
    pub fn register_service(
        &self,
        client_id: ClientId,
        descriptor: &str,
        service: Arc<dyn aidl_compat::IBinder>,
    ) -> HandleId {
        let ptr = Arc::as_ptr(&service) as *const () as usize;
        let mut table = self.table.write().unwrap();

        if let Some(existing_handle) = table.find_by_service(client_id, ptr) {
            if let Some(entry) = table.get_mut(client_id, existing_handle) {
                entry.acquire_strong(1);
                return existing_handle;
            }
        }

        let handle = table.allocate_handle_id(client_id);
        let entry = HostHandleEntry::new(handle, client_id, descriptor, service);
        table.insert(entry).expect("Freshly allocated handle must insert cleanly");
        handle
    }

    /// Register a host service with an explicitly specified handle ID (e.g. handle 0).
    pub fn register_service_with_handle(
        &self,
        client_id: ClientId,
        handle: HandleId,
        descriptor: &str,
        service: Arc<dyn aidl_compat::IBinder>,
    ) -> Result<HandleId, BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = HostHandleEntry::new(handle, client_id, descriptor, service);
        table.insert(entry)?;
        Ok(handle)
    }

    /// Retrieve a reference to the underlying host service if the handle is valid and active.
    pub fn get_service(
        &self,
        client_id: ClientId,
        handle: HandleId,
    ) -> Option<Arc<dyn aidl_compat::IBinder>> {
        let table = self.table.read().unwrap();
        table.get(client_id, handle).and_then(|entry| {
            if entry.strong_count > 0 {
                Some(Arc::clone(&entry.service))
            } else {
                None
            }
        })
    }

    /// Retrieve the AIDL interface class descriptor for a handle.
    pub fn get_descriptor(&self, client_id: ClientId, handle: HandleId) -> Option<String> {
        let table = self.table.read().unwrap();
        table.get(client_id, handle).map(|e| e.descriptor.clone())
    }

    /// Acquire additional strong references on a client's handle.
    pub fn acquire_ref(
        &self,
        client_id: ClientId,
        handle: HandleId,
        count: usize,
    ) -> Result<(), BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;
        entry.acquire_strong(count);
        Ok(())
    }

    /// Release strong references on a client's handle.
    ///
    /// Returns `true` if the strong reference count reached zero and the handle entry was dropped.
    pub fn release_ref(
        &self,
        client_id: ClientId,
        handle: HandleId,
        count: usize,
    ) -> Result<bool, BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;

        let remaining = entry.release_strong(count)?;
        if remaining == 0 {
            table.remove(client_id, handle);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Acquire additional weak references on a client's handle.
    pub fn acquire_weak_ref(
        &self,
        client_id: ClientId,
        handle: HandleId,
        count: usize,
    ) -> Result<(), BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;
        entry.acquire_weak(count);
        Ok(())
    }

    /// Release weak references on a client's handle.
    pub fn release_weak_ref(
        &self,
        client_id: ClientId,
        handle: HandleId,
        count: usize,
    ) -> Result<bool, BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;
        let remaining = entry.release_weak(count)?;
        Ok(remaining == 0)
    }

    /// Transfer a handle from one client to another.
    ///
    /// Allocates or reuses a handle in `to_client` referencing the same underlying service.
    /// The source client's reference remains intact until explicitly released.
    pub fn transfer_handle(
        &self,
        from_client: ClientId,
        to_client: ClientId,
        handle: HandleId,
    ) -> Result<HandleId, BridgeError> {
        let mut table = self.table.write().unwrap();

        let (service, descriptor) = {
            let src_entry = table
                .get(from_client, handle)
                .ok_or(BridgeError::HandleNotFound(handle, from_client))?;
            (Arc::clone(&src_entry.service), src_entry.descriptor.clone())
        };

        if from_client == to_client {
            if let Some(entry) = table.get_mut(to_client, handle) {
                entry.acquire_strong(1);
                return Ok(handle);
            }
        }

        let ptr = Arc::as_ptr(&service) as *const () as usize;
        if let Some(existing_handle) = table.find_by_service(to_client, ptr) {
            if let Some(dst_entry) = table.get_mut(to_client, existing_handle) {
                dst_entry.acquire_strong(1);
                return Ok(existing_handle);
            }
        }

        let new_handle = table.allocate_handle_id(to_client);
        let new_entry = HostHandleEntry::new(new_handle, to_client, descriptor, service);
        table.insert(new_entry)?;
        Ok(new_handle)
    }

    /// Register a death recipient cookie for a client handle.
    pub fn register_death_recipient(
        &self,
        client_id: ClientId,
        handle: HandleId,
        cookie: u64,
    ) -> Result<(), BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;
        entry.add_death_recipient(cookie)
    }

    /// Unregister a death recipient cookie for a client handle.
    pub fn unregister_death_recipient(
        &self,
        client_id: ClientId,
        handle: HandleId,
        cookie: u64,
    ) -> Result<(), BridgeError> {
        let mut table = self.table.write().unwrap();
        let entry = table
            .get_mut(client_id, handle)
            .ok_or(BridgeError::HandleNotFound(handle, client_id))?;
        entry.remove_death_recipient(cookie)
    }

    /// Handle client termination or crash.
    ///
    /// Reclaims all handles owned by `client_id`, drops the associated host service references,
    /// and dispatches death notifications for all registered cookies.
    /// Returns the list of `(HandleId, cookie)` pairs that were triggered.
    pub fn on_client_died(&self, client_id: ClientId) -> Vec<(HandleId, u64)> {
        let removed_entries = {
            let mut table = self.table.write().unwrap();
            table.remove_all_for_client(client_id)
        };

        let mut events = Vec::new();
        for entry in removed_entries {
            for cookie in entry.death_recipients {
                events.push((entry.handle, cookie));
            }
        }

        if !events.is_empty() {
            self.death_registry.dispatch_batch(client_id, &events);
        }

        events
    }

    /// Return reference to the death notification registry.
    pub fn death_registry(&self) -> &Arc<DeathRegistry> {
        &self.death_registry
    }

    /// Return the strong reference count for a handle.
    pub fn get_strong_count(&self, client_id: ClientId, handle: HandleId) -> Option<usize> {
        let table = self.table.read().unwrap();
        table.get(client_id, handle).map(|e| e.strong_count)
    }

    /// Return the weak reference count for a handle.
    pub fn get_weak_count(&self, client_id: ClientId, handle: HandleId) -> Option<usize> {
        let table = self.table.read().unwrap();
        table.get(client_id, handle).map(|e| e.weak_count)
    }

    /// Return the list of registered death recipient cookies for a handle.
    pub fn get_death_recipients(&self, client_id: ClientId, handle: HandleId) -> Option<Vec<u64>> {
        let table = self.table.read().unwrap();
        table.get(client_id, handle).map(|e| e.death_recipients.clone())
    }

    /// Return the total count of active handles for a specific client.
    pub fn handle_count(&self, client_id: ClientId) -> usize {
        let table = self.table.read().unwrap();
        table.client_handle_count(client_id)
    }

    /// Return the total count of active handles across all clients.
    pub fn total_handles(&self) -> usize {
        let table = self.table.read().unwrap();
        table.total_handles()
    }

    /// Return a list of all active handle IDs for a specific client.
    pub fn list_handles(&self, client_id: ClientId) -> Vec<HandleId> {
        let table = self.table.read().unwrap();
        table.list_client_handles(client_id)
    }
}

impl std::fmt::Debug for HandleBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandleBridge")
            .field("total_handles", &self.total_handles())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::{DeathRecipient, IBinder, Result as AidlResult};
    use binder_rt::types::{TransactionCode, TransactionFlags};
    use binder_rt::Parcel;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestBinder {
        dropped: Arc<AtomicBool>,
    }

    impl Drop for TestBinder {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    impl IBinder for TestBinder {
        fn transact(
            &self,
            _code: TransactionCode,
            _flags: TransactionFlags,
            _data: &Parcel,
            _reply: &mut Parcel,
        ) -> AidlResult<()> {
            Ok(())
        }
        fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
            Ok(())
        }
        fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_bridge_debug_and_list_handles() {
        let bridge = HandleBridge::new();
        let flag1 = Arc::new(AtomicBool::new(false));
        let flag2 = Arc::new(AtomicBool::new(false));
        let svc1: Arc<dyn IBinder> = Arc::new(TestBinder {
            dropped: flag1,
        });
        let svc2: Arc<dyn IBinder> = Arc::new(TestBinder {
            dropped: flag2,
        });

        let h1 = bridge.register_service(1, "desc.one", svc1);
        let h2 = bridge.register_service(1, "desc.two", svc2);

        let handles = bridge.list_handles(1);
        assert_eq!(handles.len(), 2);
        assert!(handles.contains(&h1));
        assert!(handles.contains(&h2));

        let dbg = format!("{:?}", bridge);
        assert!(dbg.contains("HandleBridge"));
        assert!(dbg.contains("total_handles: 2"));
    }
}

