//! Bidirectional handle mapping table and host handle entry structures.

use crate::BridgeError;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Client identifier representing a guest process or transport channel.
pub type ClientId = u32;

/// Handle identifier representing a local Binder handle within a client.
pub type HandleId = u32;

/// Host-side handle table entry tracking a registered Binder service.
#[derive(Clone)]
pub struct HostHandleEntry {
    /// Local handle ID visible to the client.
    pub handle: HandleId,
    /// Owning client ID.
    pub client_id: ClientId,
    /// AIDL interface class descriptor string.
    pub descriptor: String,
    /// Trait object representing the host service instance.
    pub service: Arc<dyn aidl_compat::IBinder>,
    /// Strong reference count.
    pub strong_count: usize,
    /// Weak reference count.
    pub weak_count: usize,
    /// Registered death recipient cookie values.
    pub death_recipients: Vec<u64>,
}

impl HostHandleEntry {
    /// Create a new host handle entry with strong count initialized to 1.
    pub fn new(
        handle: HandleId,
        client_id: ClientId,
        descriptor: impl Into<String>,
        service: Arc<dyn aidl_compat::IBinder>,
    ) -> Self {
        Self {
            handle,
            client_id,
            descriptor: descriptor.into(),
            service,
            strong_count: 1,
            weak_count: 0,
            death_recipients: Vec::new(),
        }
    }

    /// Return the raw pointer address of the underlying service trait object.
    pub fn service_ptr(&self) -> usize {
        Arc::as_ptr(&self.service) as *const () as usize
    }

    /// Acquire strong references by `count`.
    pub fn acquire_strong(&mut self, count: usize) {
        self.strong_count = self.strong_count.saturating_add(count);
    }

    /// Release strong references by `count`.
    ///
    /// Returns the remaining strong count or an error on underflow.
    pub fn release_strong(&mut self, count: usize) -> Result<usize, BridgeError> {
        if count > self.strong_count {
            return Err(BridgeError::InvalidRefCount(self.handle, self.client_id));
        }
        self.strong_count -= count;
        Ok(self.strong_count)
    }

    /// Acquire weak references by `count`.
    pub fn acquire_weak(&mut self, count: usize) {
        self.weak_count = self.weak_count.saturating_add(count);
    }

    /// Release weak references by `count`.
    ///
    /// Returns the remaining weak count or an error on underflow.
    pub fn release_weak(&mut self, count: usize) -> Result<usize, BridgeError> {
        if count > self.weak_count {
            return Err(BridgeError::InvalidRefCount(self.handle, self.client_id));
        }
        self.weak_count -= count;
        Ok(self.weak_count)
    }

    /// Register a death recipient cookie.
    pub fn add_death_recipient(&mut self, cookie: u64) -> Result<(), BridgeError> {
        if self.death_recipients.contains(&cookie) {
            return Err(BridgeError::DeathRecipientAlreadyRegistered(
                cookie,
                self.handle,
                self.client_id,
            ));
        }
        self.death_recipients.push(cookie);
        Ok(())
    }

    /// Unregister a previously registered death recipient cookie.
    pub fn remove_death_recipient(&mut self, cookie: u64) -> Result<(), BridgeError> {
        if let Some(pos) = self.death_recipients.iter().position(|&c| c == cookie) {
            self.death_recipients.swap_remove(pos);
            Ok(())
        } else {
            Err(BridgeError::DeathRecipientNotFound(
                cookie,
                self.handle,
                self.client_id,
            ))
        }
    }
}

impl std::fmt::Debug for HostHandleEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostHandleEntry")
            .field("handle", &self.handle)
            .field("client_id", &self.client_id)
            .field("descriptor", &self.descriptor)
            .field("strong_count", &self.strong_count)
            .field("weak_count", &self.weak_count)
            .field("death_recipients", &self.death_recipients)
            .finish()
    }
}

/// Bidirectional mapping table managing handle allocations and service pointers.
#[derive(Default, Debug)]
pub struct HandleTable {
    /// Mapping from `(ClientId, HandleId)` to `HostHandleEntry`.
    entries: HashMap<(ClientId, HandleId), HostHandleEntry>,
    /// Reverse mapping from `(ClientId, service_ptr)` to `HandleId`.
    service_to_handle: HashMap<(ClientId, usize), HandleId>,
    /// Set of active handles owned by each client.
    client_handles: HashMap<ClientId, HashSet<HandleId>>,
    /// Next handle ID generator per client.
    client_next_handle: HashMap<ClientId, HandleId>,
}

impl HandleTable {
    /// Create a new empty handle table.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            service_to_handle: HashMap::new(),
            client_handles: HashMap::new(),
            client_next_handle: HashMap::new(),
        }
    }

    /// Allocate the next available handle ID for a client.
    pub fn allocate_handle_id(&mut self, client_id: ClientId) -> HandleId {
        let next_id = self.client_next_handle.entry(client_id).or_insert(1);
        loop {
            let id = *next_id;
            *next_id = next_id.wrapping_add(1);
            if *next_id == 0 {
                *next_id = 1;
            }
            if !self.entries.contains_key(&(client_id, id)) {
                return id;
            }
        }
    }

    /// Insert a new handle entry into the table.
    pub fn insert(&mut self, entry: HostHandleEntry) -> Result<(), BridgeError> {
        let key = (entry.client_id, entry.handle);
        if self.entries.contains_key(&key) {
            return Err(BridgeError::HandleAlreadyExists(
                entry.handle,
                entry.client_id,
            ));
        }

        let ptr = entry.service_ptr();
        self.service_to_handle.insert((entry.client_id, ptr), entry.handle);
        self.client_handles
            .entry(entry.client_id)
            .or_default()
            .insert(entry.handle);
        self.entries.insert(key, entry);
        Ok(())
    }

    /// Look up an entry by client ID and handle ID.
    pub fn get(&self, client_id: ClientId, handle: HandleId) -> Option<&HostHandleEntry> {
        self.entries.get(&(client_id, handle))
    }

    /// Look up a mutable entry by client ID and handle ID.
    pub fn get_mut(
        &mut self,
        client_id: ClientId,
        handle: HandleId,
    ) -> Option<&mut HostHandleEntry> {
        self.entries.get_mut(&(client_id, handle))
    }

    /// Look up existing handle for a client by service pointer.
    pub fn find_by_service(&self, client_id: ClientId, service_ptr: usize) -> Option<HandleId> {
        self.service_to_handle.get(&(client_id, service_ptr)).copied()
    }

    /// Remove a handle entry from the table.
    pub fn remove(&mut self, client_id: ClientId, handle: HandleId) -> Option<HostHandleEntry> {
        if let Some(entry) = self.entries.remove(&(client_id, handle)) {
            let ptr = entry.service_ptr();
            self.service_to_handle.remove(&(client_id, ptr));
            if let Some(handles) = self.client_handles.get_mut(&client_id) {
                handles.remove(&handle);
                if handles.is_empty() {
                    self.client_handles.remove(&client_id);
                }
            }
            Some(entry)
        } else {
            None
        }
    }

    /// Reclaim all handle entries owned by a client.
    pub fn remove_all_for_client(&mut self, client_id: ClientId) -> Vec<HostHandleEntry> {
        let handles: Vec<HandleId> = self
            .client_handles
            .remove(&client_id)
            .into_iter()
            .flat_map(|set| set.into_iter())
            .collect();

        let mut removed = Vec::with_capacity(handles.len());
        for handle in handles {
            if let Some(entry) = self.entries.remove(&(client_id, handle)) {
                let ptr = entry.service_ptr();
                self.service_to_handle.remove(&(client_id, ptr));
                removed.push(entry);
            }
        }
        self.client_next_handle.remove(&client_id);
        removed
    }

    /// Return the count of handles owned by a client.
    pub fn client_handle_count(&self, client_id: ClientId) -> usize {
        self.client_handles
            .get(&client_id)
            .map(|h| h.len())
            .unwrap_or(0)
    }

    /// Return the total count of handles across all clients.
    pub fn total_handles(&self) -> usize {
        self.entries.len()
    }

    /// Return a list of all handle IDs owned by a client.
    pub fn list_client_handles(&self, client_id: ClientId) -> Vec<HandleId> {
        self.client_handles
            .get(&client_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
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
    fn test_host_handle_entry_refcounting_and_death_recipients() {
        let flag = Arc::new(AtomicBool::new(false));
        let svc: Arc<dyn IBinder> = Arc::new(TestBinder {
            dropped: Arc::clone(&flag),
        });
        let mut entry = HostHandleEntry::new(1, 10, "test.descriptor", svc);

        assert_eq!(entry.strong_count, 1);
        assert_eq!(entry.weak_count, 0);

        entry.acquire_strong(2);
        assert_eq!(entry.strong_count, 3);

        assert_eq!(entry.release_strong(1).unwrap(), 2);
        assert_eq!(entry.release_strong(2).unwrap(), 0);
        assert_eq!(
            entry.release_strong(1),
            Err(BridgeError::InvalidRefCount(1, 10))
        );

        entry.acquire_weak(4);
        assert_eq!(entry.weak_count, 4);
        assert_eq!(entry.release_weak(2).unwrap(), 2);
        assert_eq!(entry.release_weak(2).unwrap(), 0);
        assert_eq!(
            entry.release_weak(1),
            Err(BridgeError::InvalidRefCount(1, 10))
        );

        // Death recipients
        entry.add_death_recipient(100).unwrap();
        assert_eq!(
            entry.add_death_recipient(100),
            Err(BridgeError::DeathRecipientAlreadyRegistered(100, 1, 10))
        );
        entry.remove_death_recipient(100).unwrap();
        assert_eq!(
            entry.remove_death_recipient(100),
            Err(BridgeError::DeathRecipientNotFound(100, 1, 10))
        );
    }

    #[test]
    fn test_handle_table_allocations_and_reclaim() {
        let mut table = HandleTable::new();
        let flag = Arc::new(AtomicBool::new(false));
        let svc: Arc<dyn IBinder> = Arc::new(TestBinder {
            dropped: Arc::clone(&flag),
        });

        let h1 = table.allocate_handle_id(1);
        let h2 = table.allocate_handle_id(1);
        assert_eq!(h1, 1);
        assert_eq!(h2, 2);

        let entry = HostHandleEntry::new(h1, 1, "test.desc", Arc::clone(&svc));
        table.insert(entry).unwrap();

        // Duplicate insert should error
        let dup_entry = HostHandleEntry::new(h1, 1, "test.desc2", Arc::clone(&svc));
        assert_eq!(
            table.insert(dup_entry),
            Err(BridgeError::HandleAlreadyExists(h1, 1))
        );

        assert_eq!(table.client_handle_count(1), 1);
        assert_eq!(table.total_handles(), 1);
        assert_eq!(table.list_client_handles(1), vec![1]);

        let ptr = Arc::as_ptr(&svc) as *const () as usize;
        assert_eq!(table.find_by_service(1, ptr), Some(1));
        assert_eq!(table.find_by_service(2, ptr), None);

        // Reclaim
        let removed = table.remove_all_for_client(1);
        assert_eq!(removed.len(), 1);
        assert_eq!(table.total_handles(), 0);
        assert_eq!(table.client_handle_count(1), 0);
    }
}

