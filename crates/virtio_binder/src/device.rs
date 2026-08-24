//! Host-side Virtio-Binder Device implementation.
//!
//! Manages registered host AIDL services (`Arc<dyn IBinder>`), dispatches incoming
//! virtqueue transactions, manages handle reference counting, and routes death events
//! to the asynchronous event queue.

use crate::protocol::*;
use crate::queue::{new_shared_virtqueue, QueueError, SharedVirtQueue, VirtQueueChain};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use binder_rt::status::{STATUS_DEAD_OBJECT, STATUS_INVALID_OPERATION, STATUS_NAME_NOT_FOUND, STATUS_OK};
use binder_rt::wire::{BR_DEAD_REPLY, BR_FAILED_REPLY, BR_OK, BR_REPLY};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

// -----------------------------------------------------------------------------
// Device Errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("Queue processing error: {0}")]
    Queue(#[from] QueueError),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Service handle {0} not found")]
    ServiceNotFound(u32),

    #[error("Service handle {0} is dead")]
    DeadObject(u32),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

// -----------------------------------------------------------------------------
// Host Service Registration & Handle State
// -----------------------------------------------------------------------------

#[derive(Clone)]
struct ServiceEntry {
    service: Arc<dyn IBinder>,
    descriptor: Option<String>,
    ref_count: usize,
    death_cookies: Vec<u64>,
}

// -----------------------------------------------------------------------------
// VirtioBinderDevice Struct
// -----------------------------------------------------------------------------

/// Paravirtualized Host-Side Virtio-Binder Device.
pub struct VirtioBinderDevice {
    services: RwLock<HashMap<u32, ServiceEntry>>,
    tx_rx_queue: SharedVirtQueue,
    event_queue: SharedVirtQueue,
    next_auto_handle: Mutex<u32>,
}

impl VirtioBinderDevice {
    /// Construct a new `VirtioBinderDevice` with default empty queues and service table.
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            tx_rx_queue: new_shared_virtqueue(VIRTIO_BINDER_QUEUE_TX_RX),
            event_queue: new_shared_virtqueue(VIRTIO_BINDER_QUEUE_EVENT),
            next_auto_handle: Mutex::new(100),
        }
    }

    /// Return reference to Queue 0 (`tx_rx_queue`).
    pub fn tx_rx_queue(&self) -> SharedVirtQueue {
        Arc::clone(&self.tx_rx_queue)
    }

    /// Return reference to Queue 1 (`event_queue`).
    pub fn event_queue(&self) -> SharedVirtQueue {
        Arc::clone(&self.event_queue)
    }

    // -------------------------------------------------------------------------
    // Service Registration & Lifecycle
    // -------------------------------------------------------------------------

    /// Register a host service at a specific handle ID.
    pub fn register_service(&self, handle: u32, service: Arc<dyn IBinder>) {
        let descriptor = service.get_class_descriptor().map(|s| s.to_string());
        let mut map = self.services.write().unwrap();
        map.insert(
            handle,
            ServiceEntry {
                service,
                descriptor,
                ref_count: 1,
                death_cookies: Vec::new(),
            },
        );
    }

    /// Register a host `SpIBinder` at a specific handle ID.
    pub fn register_binder(&self, handle: u32, binder: SpIBinder) {
        self.register_service(handle, binder.into_arc());
    }

    /// Register a host service and automatically allocate a new handle ID.
    pub fn register_service_auto(&self, service: Arc<dyn IBinder>) -> u32 {
        let handle = {
            let mut next = self.next_auto_handle.lock().unwrap();
            let h = *next;
            *next += 1;
            h
        };
        self.register_service(handle, service);
        handle
    }

    /// Register a host `SpIBinder` and automatically allocate a new handle ID.
    pub fn register_binder_auto(&self, binder: SpIBinder) -> u32 {
        self.register_service_auto(binder.into_arc())
    }

    /// Unregister a service by handle ID.
    pub fn unregister_service(&self, handle: u32) -> Option<Arc<dyn IBinder>> {
        let mut map = self.services.write().unwrap();
        map.remove(&handle).map(|entry| entry.service)
    }

    /// Retrieve a service by handle ID.
    pub fn get_service(&self, handle: u32) -> Option<Arc<dyn IBinder>> {
        let map = self.services.read().unwrap();
        map.get(&handle).map(|entry| Arc::clone(&entry.service))
    }

    /// Retrieve the AIDL descriptor of a registered service if known.
    pub fn get_service_descriptor(&self, handle: u32) -> Option<String> {
        let map = self.services.read().unwrap();
        map.get(&handle).and_then(|entry| entry.descriptor.clone())
    }

    /// Acquire reference count for a handle.
    pub fn acquire_handle(&self, handle: u32) -> Result<(), DeviceError> {
        let mut map = self.services.write().unwrap();
        if let Some(entry) = map.get_mut(&handle) {
            entry.ref_count += 1;
            Ok(())
        } else {
            Err(DeviceError::ServiceNotFound(handle))
        }
    }

    /// Release reference count for a handle. Returns `true` if handle was removed on count == 0.
    pub fn release_handle(&self, handle: u32) -> Result<bool, DeviceError> {
        let mut map = self.services.write().unwrap();
        if let Some(entry) = map.get_mut(&handle) {
            if entry.ref_count > 1 {
                entry.ref_count -= 1;
                Ok(false)
            } else {
                map.remove(&handle);
                Ok(true)
            }
        } else {
            Err(DeviceError::ServiceNotFound(handle))
        }
    }

    /// Return current reference count for a handle.
    pub fn get_ref_count(&self, handle: u32) -> Option<usize> {
        let map = self.services.read().unwrap();
        map.get(&handle).map(|entry| entry.ref_count)
    }

    /// Link a death recipient cookie to a target handle.
    pub fn link_death(&self, handle: u32, cookie: u64) -> Result<(), DeviceError> {
        let mut map = self.services.write().unwrap();
        if let Some(entry) = map.get_mut(&handle) {
            if !entry.death_cookies.contains(&cookie) {
                entry.death_cookies.push(cookie);
            }
            Ok(())
        } else {
            Err(DeviceError::ServiceNotFound(handle))
        }
    }

    /// Unlink a death recipient cookie from a target handle.
    pub fn unlink_death(&self, handle: u32, cookie: u64) -> Result<bool, DeviceError> {
        let mut map = self.services.write().unwrap();
        if let Some(entry) = map.get_mut(&handle) {
            if let Some(pos) = entry.death_cookies.iter().position(|&c| c == cookie) {
                entry.death_cookies.remove(pos);
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(DeviceError::ServiceNotFound(handle))
        }
    }

    /// Trigger death notification for a handle, pushing death events to Queue 1.
    pub fn trigger_death(&self, handle: u32) {
        let cookies = {
            let mut map = self.services.write().unwrap();
            if let Some(entry) = map.get_mut(&handle) {
                let cookies = entry.death_cookies.clone();
                entry.death_cookies.clear();
                cookies
            } else {
                Vec::new()
            }
        };

        let queue = Arc::clone(&self.event_queue);
        let mut eq = queue.lock().unwrap();
        for cookie in cookies {
            eq.push_event(VirtioBinderEventHdr::new_death(handle, cookie));
        }
    }

    // -------------------------------------------------------------------------
    // Request Dispatch & Execution
    // -------------------------------------------------------------------------

    /// Process a parsed `VirtioBinderRequest` and generate a `VirtioBinderResponse`.
    pub fn process_request(&self, req: &VirtioBinderRequest) -> VirtioBinderResponse {
        match req.hdr.cmd {
            CMD_PING => {
                if req.hdr.target_handle == 0 {
                    // Handle 0 represents the host ServiceManager
                    VirtioBinderResponse::new(
                        req.hdr.msg_id,
                        STATUS_OK,
                        BR_REPLY as i32,
                        0,
                        Vec::new(),
                        Vec::new(),
                    )
                } else {
                    let map = self.services.read().unwrap();
                    if let Some(entry) = map.get(&req.hdr.target_handle) {
                        if !entry.service.is_binder_alive() {
                            VirtioBinderResponse::error(
                                req.hdr.msg_id,
                                STATUS_DEAD_OBJECT,
                                BR_DEAD_REPLY as i32,
                            )
                        } else {
                            match entry.service.ping_binder() {
                                Ok(()) => VirtioBinderResponse::new(
                                    req.hdr.msg_id,
                                    STATUS_OK,
                                    BR_REPLY as i32,
                                    0,
                                    Vec::new(),
                                    Vec::new(),
                                ),
                                Err(status) => VirtioBinderResponse::error(
                                    req.hdr.msg_id,
                                    status.status_code() as i32,
                                    BR_FAILED_REPLY as i32,
                                ),
                            }
                        }
                    } else {
                        VirtioBinderResponse::error(
                            req.hdr.msg_id,
                            STATUS_DEAD_OBJECT,
                            BR_DEAD_REPLY as i32,
                        )
                    }
                }
            }

            CMD_TRANSACT => {
                let target_service = {
                    let map = self.services.read().unwrap();
                    map.get(&req.hdr.target_handle)
                        .map(|e| Arc::clone(&e.service))
                };

                let service = match target_service {
                    Some(s) => s,
                    None => {
                        return VirtioBinderResponse::error(
                            req.hdr.msg_id,
                            STATUS_DEAD_OBJECT,
                            BR_DEAD_REPLY as i32,
                        );
                    }
                };

                if !service.is_binder_alive() {
                    return VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        STATUS_DEAD_OBJECT,
                        BR_DEAD_REPLY as i32,
                    );
                }

                let req_parcel = req.to_parcel();
                let mut reply_parcel = Parcel::new();

                let transact_res =
                    service.transact(req.hdr.code, req.hdr.flags, &req_parcel, &mut reply_parcel);

                if req.hdr.is_one_way() {
                    return VirtioBinderResponse::one_way_complete(req.hdr.msg_id);
                }

                match transact_res {
                    Ok(()) => VirtioBinderResponse::ok_from_parcel(req.hdr.msg_id, &reply_parcel),
                    Err(status) => VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        status.status_code() as i32,
                        BR_FAILED_REPLY as i32,
                    ),
                }
            }

            CMD_ACQUIRE => {
                let res = self.acquire_handle(req.hdr.target_handle);
                match res {
                    Ok(()) => VirtioBinderResponse::new(
                        req.hdr.msg_id,
                        STATUS_OK,
                        BR_OK as i32,
                        0,
                        Vec::new(),
                        Vec::new(),
                    ),
                    Err(_) => VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        STATUS_NAME_NOT_FOUND,
                        BR_FAILED_REPLY as i32,
                    ),
                }
            }

            CMD_RELEASE => {
                let res = self.release_handle(req.hdr.target_handle);
                match res {
                    Ok(_) => VirtioBinderResponse::new(
                        req.hdr.msg_id,
                        STATUS_OK,
                        BR_OK as i32,
                        0,
                        Vec::new(),
                        Vec::new(),
                    ),
                    Err(_) => VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        STATUS_NAME_NOT_FOUND,
                        BR_FAILED_REPLY as i32,
                    ),
                }
            }

            CMD_LINK_DEATH => {
                let res = self.link_death(req.hdr.target_handle, req.hdr.cookie);
                match res {
                    Ok(()) => VirtioBinderResponse::new(
                        req.hdr.msg_id,
                        STATUS_OK,
                        BR_OK as i32,
                        0,
                        Vec::new(),
                        Vec::new(),
                    ),
                    Err(_) => VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        STATUS_NAME_NOT_FOUND,
                        BR_FAILED_REPLY as i32,
                    ),
                }
            }

            CMD_UNLINK_DEATH => {
                let res = self.unlink_death(req.hdr.target_handle, req.hdr.cookie);
                match res {
                    Ok(_) => VirtioBinderResponse::new(
                        req.hdr.msg_id,
                        STATUS_OK,
                        BR_OK as i32,
                        0,
                        Vec::new(),
                        Vec::new(),
                    ),
                    Err(_) => VirtioBinderResponse::error(
                        req.hdr.msg_id,
                        STATUS_NAME_NOT_FOUND,
                        BR_FAILED_REPLY as i32,
                    ),
                }
            }

            unknown => {
                log::warn!("VirtioBinderDevice: received unknown command {}", unknown);
                VirtioBinderResponse::error(
                    req.hdr.msg_id,
                    STATUS_INVALID_OPERATION,
                    BR_FAILED_REPLY as i32,
                )
            }
        }
    }

    /// Process a raw contiguous byte packet, returning serialized response bytes.
    pub fn process_packet(&self, req_bytes: &[u8]) -> Result<Vec<u8>, DeviceError> {
        let req = VirtioBinderRequest::deserialize(req_bytes).map_err(DeviceError::Protocol)?;
        let resp = self.process_request(&req);
        Ok(resp.serialize())
    }

    /// Process a single virtqueue descriptor chain (Queue 0), reading request and writing response.
    pub fn process_virtqueue_chain(
        &self,
        chain: &mut VirtQueueChain,
    ) -> Result<usize, DeviceError> {
        let req = chain.parse_request()?;
        let resp = self.process_request(&req);
        let written = chain.write_response(&resp)?;
        Ok(written)
    }

    /// Process all pending descriptor chains in Queue 0.
    pub fn process_all_chains(&self) -> Result<usize, DeviceError> {
        let mut chains = Vec::new();
        {
            let mut q = self.tx_rx_queue.lock().unwrap();
            while let Some(chain) = q.pop_chain() {
                chains.push(chain);
            }
        }

        let count = chains.len();
        for mut chain in chains {
            self.process_virtqueue_chain(&mut chain)?;
        }
        Ok(count)
    }
}

impl Default for VirtioBinderDevice {
    fn default() -> Self {
        Self::new()
    }
}
