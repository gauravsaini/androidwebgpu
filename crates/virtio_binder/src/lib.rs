//! # virtio_binder
//!
//! Paravirtualized Virtio-Binder Transport Device and Guest Shim for AndroidWebGPU.
//! Provides host-side VirtIO ring processing, protocol envelope codec, handle routing,
//! and guest AIDL `RemoteTransport` integration.

pub mod device;
pub mod guest_shim;
pub mod protocol;
pub mod queue;

// -----------------------------------------------------------------------------
// Top-Level Public Re-exports
// -----------------------------------------------------------------------------

pub use device::{DeviceError, VirtioBinderDevice};
pub use guest_shim::{
    DirectDeviceBackend, GuestVirtioTransport, TransportBackend, VirtqueueChainBackend,
};
pub use protocol::{
    ProtocolError, VirtioBinderEventHdr, VirtioBinderReqHdr, VirtioBinderRequest,
    VirtioBinderRespHdr, VirtioBinderResponse, CMD_ACQUIRE, CMD_LINK_DEATH, CMD_PING, CMD_RELEASE,
    CMD_TRANSACT, CMD_UNLINK_DEATH, EVENT_TYPE_ACQUIRE, EVENT_TYPE_DEATH, EVENT_TYPE_RELEASE,
    VIRTIO_BINDER_QUEUE_EVENT, VIRTIO_BINDER_QUEUE_TX_RX, VIRTIO_ID_BINDER,
};
pub use queue::{
    new_shared_virtqueue, QueueError, SharedVirtQueue, VirtQueue, VirtQueueChain, VirtqDesc,
    VRING_DESC_F_INDIRECT, VRING_DESC_F_NEXT, VRING_DESC_F_WRITE,
};
