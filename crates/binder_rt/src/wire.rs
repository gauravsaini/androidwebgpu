//! Binder kernel transaction envelopes and protocol opcodes.

use bytemuck::{Pod, Zeroable};

// -----------------------------------------------------------------------------
// Transaction Flags
// -----------------------------------------------------------------------------

/// Asynchronous one-way call, no reply is expected.
pub const TF_ONE_WAY: u32 = 0x01;
/// Contents are the root object.
pub const TF_ROOT_OBJECT: u32 = 0x04;
/// Contents are a 32-bit status code.
pub const TF_STATUS_CODE: u32 = 0x08;
/// Allow incoming file descriptors.
pub const TF_ACCEPT_FDS: u32 = 0x10;
/// Clear buffer after transaction completes.
pub const TF_CLEAR_BUF: u32 = 0x20;

// -----------------------------------------------------------------------------
// Special Transaction Codes
// -----------------------------------------------------------------------------

pub const FIRST_CALL_TRANSACTION: u32 = 0x00000001;
pub const LAST_CALL_TRANSACTION: u32 = 0x00ffffff;

/// Ping the remote object (`_PNG`).
pub const PING_TRANSACTION: u32 = 0x5f504e47;
/// Dump service debug information (`_DMP`).
pub const DUMP_TRANSACTION: u32 = 0x5f444d50;
/// Execute a shell command (`_CMD`).
pub const SHELL_CMD_TRANSACTION: u32 = 0x5f434d44;
/// Query interface descriptor (`_NTF`).
pub const INTERFACE_TRANSACTION: u32 = 0x5f4e5446;
/// System properties transaction (`_SPR`).
pub const SYSPROPS_TRANSACTION: u32 = 0x5f535052;

// -----------------------------------------------------------------------------
// Userspace -> Kernel Binder Commands (BC_*)
// -----------------------------------------------------------------------------

pub const BC_TRANSACTION: u32 = 0x40406300;
pub const BC_REPLY: u32 = 0x40406301;
pub const BC_ACQUIRE_RESULT: u32 = 0x40046302;
pub const BC_FREE_BUFFER: u32 = 0x40086303;
pub const BC_INCREFS: u32 = 0x40046304;
pub const BC_ACQUIRE: u32 = 0x40046305;
pub const BC_RELEASE: u32 = 0x40046306;
pub const BC_DECREFS: u32 = 0x40046307;
pub const BC_INCREFS_DONE: u32 = 0x40106308;
pub const BC_ACQUIRE_DONE: u32 = 0x40106309;
pub const BC_ATTEMPT_ACQUIRE: u32 = 0x4018630a;
pub const BC_REGISTER_LOOPER: u32 = 0x0000630b;
pub const BC_ENTER_LOOPER: u32 = 0x0000630c;
pub const BC_EXIT_LOOPER: u32 = 0x0000630d;
pub const BC_REQUEST_DEATH_NOTIFICATION: u32 = 0x4010630e;
pub const BC_CLEAR_DEATH_NOTIFICATION: u32 = 0x4010630f;
pub const BC_DEAD_BINDER_DONE: u32 = 0x40086310;

// -----------------------------------------------------------------------------
// Kernel -> Userspace Binder Return Commands (BR_*)
// -----------------------------------------------------------------------------

pub const BR_ERROR: u32 = 0x80047200;
pub const BR_OK: u32 = 0x00007201;
pub const BR_TRANSACTION: u32 = 0x80407202;
pub const BR_REPLY: u32 = 0x80407203;
pub const BR_ACQUIRE_RESULT: u32 = 0x80047204;
pub const BR_DEAD_REPLY: u32 = 0x00007205;
pub const BR_TRANSACTION_COMPLETE: u32 = 0x00007206;
pub const BR_INCREFS: u32 = 0x80107207;
pub const BR_ACQUIRE: u32 = 0x80107208;
pub const BR_RELEASE: u32 = 0x80107209;
pub const BR_DECREFS: u32 = 0x8010720a;
pub const BR_NOOP: u32 = 0x0000720c;
pub const BR_SPAWN_LOOPER: u32 = 0x0000720d;
pub const BR_FINISHED: u32 = 0x0000720e;
pub const BR_DEAD_BINDER: u32 = 0x8008720f;
pub const BR_CLEAR_DEATH_NOTIFICATION_DONE: u32 = 0x80087210;
pub const BR_FAILED_REPLY: u32 = 0x00007211;

// -----------------------------------------------------------------------------
// 64-bit C-ABI Transaction Envelope
// -----------------------------------------------------------------------------

/// 64-byte C-ABI structure representing `binder_transaction_data`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct BinderTransactionData {
    /// Target handle or object pointer.
    pub target: u64,
    /// Target cookie.
    pub cookie: u64,
    /// Transaction code / method ID.
    pub code: u32,
    /// Transaction flags (`TF_*`).
    pub flags: u32,
    /// Sender process ID.
    pub sender_pid: i32,
    /// Sender effective UID.
    pub sender_euid: u32,
    /// Length of payload data in bytes.
    pub data_size: u64,
    /// Length of offsets array in bytes.
    pub offsets_size: u64,
    /// Data buffer pointer or offset.
    pub data_buffer: u64,
    /// Offsets buffer pointer or offset.
    pub offsets_buffer: u64,
}

impl BinderTransactionData {
    /// Construct a new transaction descriptor.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        target: u64,
        cookie: u64,
        code: u32,
        flags: u32,
        sender_pid: i32,
        sender_euid: u32,
        data_size: u64,
        offsets_size: u64,
        data_buffer: u64,
        offsets_buffer: u64,
    ) -> Self {
        Self {
            target,
            cookie,
            code,
            flags,
            sender_pid,
            sender_euid,
            data_size,
            offsets_size,
            data_buffer,
            offsets_buffer,
        }
    }

    /// Return target handle ID (low 32 bits of target).
    pub const fn target_handle(&self) -> u32 {
        self.target as u32
    }

    /// Return true if this is an asynchronous one-way call.
    pub const fn is_one_way(&self) -> bool {
        (self.flags & TF_ONE_WAY) != 0
    }

    /// Return byte slice representation.
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    /// Parse from byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= std::mem::size_of::<Self>() {
            let mut val = Self::zeroed();
            let dest = bytemuck::bytes_of_mut(&mut val);
            dest.copy_from_slice(&bytes[..std::mem::size_of::<Self>()]);
            Some(val)
        } else {
            None
        }
    }
}

/// Pointer and cookie pair for death notifications and reference callbacks.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct BinderPtrCookie {
    pub ptr: u64,
    pub cookie: u64,
}

/// Handle and cookie pair for death notifications.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct BinderHandleCookie {
    pub handle: u32,
    pub padding: u32,
    pub cookie: u64,
}
