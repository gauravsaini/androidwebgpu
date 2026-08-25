//! Linux Kernel Binder direct ioctl definitions, command opcodes, and ABI structures.

use bytemuck::{Pod, Zeroable};

// -----------------------------------------------------------------------------
// Direct Kernel ioctl Command Constants
// -----------------------------------------------------------------------------

/// Linux ioctl calculation helper values for Binder (`'b' = 0x62`).
/// _IOC_READ = 2, _IOC_WRITE = 1, _IOC_DIRSHIFT = 30, _IOC_SIZESHIFT = 16, _IOC_TYPESHIFT = 8
pub const BINDER_TYPE_IOC: u32 = 0x62; // 'b'

/// `BINDER_WRITE_READ` = `_IOWR('b', 1, binder_write_read)` = `0xc0306201` (size 48 bytes)
pub const BINDER_WRITE_READ: u64 = 0xc0306201;

/// `BINDER_SET_MAX_THREADS` = `_IOW('b', 5, u32)` = `0x40046205` (size 4 bytes)
pub const BINDER_SET_MAX_THREADS: u64 = 0x40046205;

/// `BINDER_THREAD_EXIT` = `_IOW('b', 8, i32)` = `0x40046208` (size 4 bytes)
pub const BINDER_THREAD_EXIT: u64 = 0x40046208;

/// `BINDER_VERSION` = `_IOWR('b', 9, binder_version)` = `0xc0046209` (size 4 bytes)
pub const BINDER_VERSION: u64 = 0xc0046209;

/// `BINDER_SET_CONTEXT_MGR` = `_IOW('b', 7, i32)` = `0x40046207` (size 4 bytes)
pub const BINDER_SET_CONTEXT_MGR: u64 = 0x40046207;

/// Current Binder Kernel Protocol Version.
pub const BINDER_CURRENT_PROTOCOL_VERSION: i32 = 8;

// -----------------------------------------------------------------------------
// Direct C-ABI Structures
// -----------------------------------------------------------------------------

/// 48-byte C-ABI structure for `binder_write_read`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct binder_write_read {
    pub write_size: u64,
    pub write_consumed: u64,
    pub write_buffer: u64,
    pub read_size: u64,
    pub read_consumed: u64,
    pub read_buffer: u64,
}

impl binder_write_read {
    /// Construct empty write-read descriptor.
    pub const fn new() -> Self {
        Self {
            write_size: 0,
            write_consumed: 0,
            write_buffer: 0,
            read_size: 0,
            read_consumed: 0,
            read_buffer: 0,
        }
    }

    /// Construct descriptor with write and read buffer pointers and sizes.
    pub fn with_buffers(
        write_buf: *const u8,
        write_size: usize,
        read_buf: *mut u8,
        read_size: usize,
    ) -> Self {
        Self {
            write_size: write_size as u64,
            write_consumed: 0,
            write_buffer: write_buf as u64,
            read_size: read_size as u64,
            read_consumed: 0,
            read_buffer: read_buf as u64,
        }
    }

    /// As byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    /// As mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        bytemuck::bytes_of_mut(self)
    }
}

/// 4-byte C-ABI structure for `binder_version`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct binder_version {
    pub protocol_version: i32,
}

impl binder_version {
    pub const fn new(protocol_version: i32) -> Self {
        Self { protocol_version }
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        bytemuck::bytes_of_mut(self)
    }
}

// -----------------------------------------------------------------------------
// Re-exports from `binder_rt::wire`
// -----------------------------------------------------------------------------

pub use binder_rt::wire::{
    BinderHandleCookie, BinderPtrCookie, BinderTransactionData, BC_ACQUIRE, BC_ACQUIRE_DONE,
    BC_ACQUIRE_RESULT, BC_ATTEMPT_ACQUIRE, BC_CLEAR_DEATH_NOTIFICATION, BC_DEAD_BINDER_DONE,
    BC_DECREFS, BC_ENTER_LOOPER, BC_EXIT_LOOPER, BC_FREE_BUFFER, BC_INCREFS, BC_INCREFS_DONE,
    BC_REGISTER_LOOPER, BC_RELEASE, BC_REPLY, BC_REQUEST_DEATH_NOTIFICATION, BC_TRANSACTION,
    BR_ACQUIRE, BR_ACQUIRE_RESULT, BR_CLEAR_DEATH_NOTIFICATION_DONE, BR_DEAD_BINDER, BR_DEAD_REPLY,
    BR_DECREFS, BR_ERROR, BR_FAILED_REPLY, BR_FINISHED, BR_INCREFS, BR_NOOP, BR_OK, BR_RELEASE,
    BR_REPLY, BR_SPAWN_LOOPER, BR_TRANSACTION, BR_TRANSACTION_COMPLETE, DUMP_TRANSACTION,
    FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, LAST_CALL_TRANSACTION, PING_TRANSACTION,
    SHELL_CMD_TRANSACTION, SYSPROPS_TRANSACTION, TF_ACCEPT_FDS, TF_CLEAR_BUF, TF_ONE_WAY,
    TF_ROOT_OBJECT, TF_STATUS_CODE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ioctl_number_encoding() {
        assert_eq!(BINDER_WRITE_READ, 0xc0306201);
        assert_eq!(BINDER_SET_MAX_THREADS, 0x40046205);
        assert_eq!(BINDER_VERSION, 0xc0046209);
        assert_eq!(BINDER_THREAD_EXIT, 0x40046208);
        assert_eq!(std::mem::size_of::<binder_write_read>(), 48);
        assert_eq!(std::mem::size_of::<binder_version>(), 4);
    }
}
