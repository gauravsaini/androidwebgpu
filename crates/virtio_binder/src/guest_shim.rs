//! Guest-side Virtio-Binder Transport Shim.
//!
//! Provides `GuestVirtioTransport` implementing `aidl_compat::RemoteTransport`
//! to route AIDL client proxy transactions across the VirtIO queue boundary.

use crate::device::VirtioBinderDevice;
use crate::protocol::*;
use crate::queue::VirtQueueChain;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_OK};
use aidl_compat::stub::{RemoteBinder, RemoteTransport};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{BR_DEAD_REPLY, BR_TRANSACTION_COMPLETE, TF_ONE_WAY};
use binder_rt::Parcel;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// -----------------------------------------------------------------------------
// Transport Backend Abstraction
// -----------------------------------------------------------------------------

/// Trait defining the lower-level transport channel communicating with the host device.
pub trait TransportBackend: Send + Sync + 'static {
    /// Send a raw request packet and receive the corresponding response packet.
    fn exchange_packet(&self, req_bytes: &[u8]) -> std::result::Result<Vec<u8>, ProtocolError>;

    /// Drain pending asynchronous event headers from Queue 1 if available.
    fn drain_events(&self) -> Vec<VirtioBinderEventHdr> {
        Vec::new()
    }
}

/// Direct in-memory backend for testing and zero-overhead VM integration.
pub struct DirectDeviceBackend {
    device: Arc<VirtioBinderDevice>,
}

impl DirectDeviceBackend {
    pub fn new(device: Arc<VirtioBinderDevice>) -> Self {
        Self { device }
    }
}

impl TransportBackend for DirectDeviceBackend {
    fn exchange_packet(&self, req_bytes: &[u8]) -> std::result::Result<Vec<u8>, ProtocolError> {
        self.device
            .process_packet(req_bytes)
            .map_err(|e| match e {
                crate::device::DeviceError::Protocol(p) => p,
                _ => ProtocolError::ParcelConversionError(e.to_string()),
            })
    }

    fn drain_events(&self) -> Vec<VirtioBinderEventHdr> {
        let eq_lock = self.device.event_queue();
        let mut eq = eq_lock.lock().unwrap();
        eq.drain_events()
    }
}

/// Virtqueue descriptor-chain backend simulating true VirtIO ring buffer mechanics.
pub struct VirtqueueChainBackend {
    device: Arc<VirtioBinderDevice>,
    reply_buffer_capacity: usize,
}

impl VirtqueueChainBackend {
    pub fn new(device: Arc<VirtioBinderDevice>, reply_buffer_capacity: usize) -> Self {
        Self {
            device,
            reply_buffer_capacity,
        }
    }
}

impl TransportBackend for VirtqueueChainBackend {
    fn exchange_packet(&self, req_bytes: &[u8]) -> std::result::Result<Vec<u8>, ProtocolError> {
        let mut chain =
            VirtQueueChain::from_request_bytes(0, req_bytes, self.reply_buffer_capacity);
        self.device
            .process_virtqueue_chain(&mut chain)
            .map_err(|e| match e {
                crate::device::DeviceError::Protocol(p) => p,
                _ => ProtocolError::ParcelConversionError(e.to_string()),
            })?;
        Ok(chain.take_written_data())
    }

    fn drain_events(&self) -> Vec<VirtioBinderEventHdr> {
        let eq_lock = self.device.event_queue();
        let mut eq = eq_lock.lock().unwrap();
        eq.drain_events()
    }
}

// -----------------------------------------------------------------------------
// GuestVirtioTransport Struct
// -----------------------------------------------------------------------------

/// Guest-side VirtIO transport driver shim.
pub struct GuestVirtioTransport {
    backend: Arc<dyn TransportBackend>,
    next_msg_id: AtomicU64,
}

impl GuestVirtioTransport {
    /// Construct a new `GuestVirtioTransport` using a custom backend.
    pub fn new(backend: Arc<dyn TransportBackend>) -> Self {
        Self {
            backend,
            next_msg_id: AtomicU64::new(1),
        }
    }

    /// Construct a `GuestVirtioTransport` directly connected to a host device.
    pub fn new_with_device(device: Arc<VirtioBinderDevice>) -> Self {
        Self::new(Arc::new(DirectDeviceBackend::new(device)))
    }

    /// Construct a `GuestVirtioTransport` using descriptor chain simulation with specified reply buffer size.
    pub fn new_with_virtqueue(device: Arc<VirtioBinderDevice>, reply_capacity: usize) -> Self {
        Self::new(Arc::new(VirtqueueChainBackend::new(
            device,
            reply_capacity,
        )))
    }

    /// Allocate next unique 64-bit message sequence number.
    pub fn allocate_msg_id(&self) -> u64 {
        self.next_msg_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a ping command to the specified handle.
    pub fn ping(&self, handle: u32) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::new_ping(msg_id, handle);
        let req_bytes = req.serialize();
        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if resp.hdr.status != STATUS_OK {
            Err(Status::from_status(resp.hdr.status))
        } else {
            Ok(())
        }
    }

    /// Send an acquire reference command for target handle.
    pub fn acquire_handle(&self, handle: u32) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::new_acquire(msg_id, handle);
        let req_bytes = req.serialize();
        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if resp.hdr.status != STATUS_OK {
            Err(Status::from_status(resp.hdr.status))
        } else {
            Ok(())
        }
    }

    /// Send a release reference command for target handle.
    pub fn release_handle(&self, handle: u32) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::new_release(msg_id, handle);
        let req_bytes = req.serialize();
        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if resp.hdr.status != STATUS_OK {
            Err(Status::from_status(resp.hdr.status))
        } else {
            Ok(())
        }
    }

    /// Link a death recipient cookie to a remote handle across the transport.
    pub fn link_death(&self, handle: u32, cookie: u64) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::new_link_death(msg_id, handle, cookie);
        let req_bytes = req.serialize();
        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if resp.hdr.status != STATUS_OK {
            Err(Status::from_status(resp.hdr.status))
        } else {
            Ok(())
        }
    }

    /// Unlink a death recipient cookie from a remote handle across the transport.
    pub fn unlink_death(&self, handle: u32, cookie: u64) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::new_unlink_death(msg_id, handle, cookie);
        let req_bytes = req.serialize();
        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if resp.hdr.status != STATUS_OK {
            Err(Status::from_status(resp.hdr.status))
        } else {
            Ok(())
        }
    }

    /// Drain any pending asynchronous death or lifecycle events from Queue 1.
    pub fn drain_events(&self) -> Vec<VirtioBinderEventHdr> {
        self.backend.drain_events()
    }

    /// Create an `SpIBinder` proxy object backed by this `GuestVirtioTransport`.
    pub fn create_remote_binder(
        self: &Arc<Self>,
        handle: u32,
        cookie: u64,
        descriptor: Option<&'static str>,
    ) -> SpIBinder {
        RemoteBinder::new_with_transport(handle, cookie, descriptor, Arc::clone(self) as _)
    }
}

// -----------------------------------------------------------------------------
// RemoteTransport Implementation
// -----------------------------------------------------------------------------

impl RemoteTransport for GuestVirtioTransport {
    fn transact(
        &self,
        handle: u32,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        let msg_id = self.allocate_msg_id();
        let req = VirtioBinderRequest::from_parcel(msg_id, handle, code, flags, 0, data);
        let req_bytes = req.serialize();

        let resp_bytes = self
            .backend
            .exchange_packet(&req_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let resp = VirtioBinderResponse::deserialize(&resp_bytes)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        // Validate msg_id match
        if resp.hdr.msg_id != msg_id {
            log::error!(
                "GuestVirtioTransport: mismatched msg_id! expected {}, got {}",
                msg_id,
                resp.hdr.msg_id
            );
            return Err(Status::from_status(STATUS_BAD_VALUE));
        }

        // Handle one-way asynchronous transactions
        if (flags & TF_ONE_WAY) != 0 || resp.hdr.result_code == BR_TRANSACTION_COMPLETE as i32 {
            return Ok(());
        }

        // Check for transport/kernel errors
        if resp.hdr.status != STATUS_OK {
            return Err(Status::from_status(resp.hdr.status));
        }

        if resp.hdr.result_code == BR_DEAD_REPLY as i32 {
            return Err(Status::from_status(STATUS_DEAD_OBJECT));
        }

        // Unpack response parcel
        let reply_parcel = resp.to_parcel();
        *reply = reply_parcel;

        Ok(())
    }
}
