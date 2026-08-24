//! # binder_handle_bridge
//!
//! Cross-Boundary Handle Bridge & Lifecycle Management for AndroidWebGPU.
//!
//! Provides bidirectional handle translation between guest handle spaces and host-side
//! AIDL service trait objects (`Arc<dyn IBinder>`), distributed reference counting
//! across VM boundaries, multi-hop handle transfers, and fault-tolerant death notification cleanup.

pub mod bridge;
pub mod death;
pub mod table;

pub use bridge::HandleBridge;
pub use death::{DeathCallbackFn, DeathNotification, DeathRegistry};
pub use table::{ClientId, HandleId, HandleTable, HostHandleEntry};

// Re-export IBinder for convenience
pub use aidl_compat::IBinder;

/// Error types returned by handle bridge operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum BridgeError {
    /// Specified handle ID was not found for the given client.
    #[error("Handle {0} not found for client {1}")]
    HandleNotFound(HandleId, ClientId),

    /// Client was not found in the handle table.
    #[error("Client {0} not found")]
    ClientNotFound(ClientId),

    /// Handle ID is already registered for this client.
    #[error("Handle {0} already exists for client {1}")]
    HandleAlreadyExists(HandleId, ClientId),

    /// Reference count decrement exceeds current count.
    #[error("Invalid reference count delta or underflow on handle {0} for client {1}")]
    InvalidRefCount(HandleId, ClientId),

    /// A death recipient with the same cookie is already registered.
    #[error("Death recipient with cookie {0} already registered for handle {1} on client {2}")]
    DeathRecipientAlreadyRegistered(u64, HandleId, ClientId),

    /// Specified death recipient cookie was not found on the handle.
    #[error("Death recipient with cookie {0} not found for handle {1} on client {2}")]
    DeathRecipientNotFound(u64, HandleId, ClientId),

    /// AIDL descriptor mismatch on handle retrieval or validation.
    #[error("Descriptor mismatch for handle {0}: expected '{1}', got '{2}'")]
    DescriptorMismatch(HandleId, String, String),

    /// Target remote Binder object is no longer alive.
    #[error("Binder is dead for handle {0}")]
    DeadBinder(HandleId),
}
