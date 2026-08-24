//! Fundamental types and structures for Android Binder IPC.

use bytemuck::{Pod, Zeroable};

/// Size type used for offsets in 64-bit Binder ABI (`binder_size_t`).
pub type BinderSizeT = u64;

/// Pointer integer type in 64-bit Binder ABI (`binder_uintptr_t`).
pub type BinderUintptrT = u64;

/// Transaction code type.
pub type TransactionCode = u32;

/// Transaction flags bitfield type.
pub type TransactionFlags = u32;

/// Well-known Binder object types.
pub const BINDER_TYPE_BINDER: u32 = 0x73622a85; // 's', 'b', '*', 0x85
pub const BINDER_TYPE_WEAK_BINDER: u32 = 0x77622a85; // 'w', 'b', '*', 0x85
pub const BINDER_TYPE_HANDLE: u32 = 0x73682a85; // 's', 'h', '*', 0x85
pub const BINDER_TYPE_WEAK_HANDLE: u32 = 0x77682a85; // 'w', 'h', '*', 0x85
pub const BINDER_TYPE_FD: u32 = 0x66642a85; // 'f', 'd', '*', 0x85
pub const BINDER_TYPE_FDA: u32 = 0x66646185; // 'f', 'd', 'a', 0x85
pub const BINDER_TYPE_PTR: u32 = 0x70742a85; // 'p', 't', '*', 0x85

/// Header for any Binder object embedded in a parcel.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct BinderObjectHeader {
    pub type_: u32,
}

/// 24-byte C-ABI structure for representing Binder objects and file descriptors in parcels.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable)]
pub struct FlatBinderObject {
    pub hdr: BinderObjectHeader,
    pub flags: u32,
    pub binder: u64,
    pub cookie: u64,
}

impl FlatBinderObject {
    /// Create a new strong binder object with local object pointer.
    pub const fn new_binder(binder_ptr: u64, flags: u32, cookie: u64) -> Self {
        Self {
            hdr: BinderObjectHeader {
                type_: BINDER_TYPE_BINDER,
            },
            flags,
            binder: binder_ptr,
            cookie,
        }
    }

    /// Create a new remote binder handle reference.
    pub const fn new_handle(handle: u32, flags: u32, cookie: u64) -> Self {
        Self {
            hdr: BinderObjectHeader {
                type_: BINDER_TYPE_HANDLE,
            },
            flags,
            binder: handle as u64,
            cookie,
        }
    }

    /// Create a new file descriptor binder object.
    pub const fn new_fd(fd: i32, cookie: u64) -> Self {
        Self {
            hdr: BinderObjectHeader {
                type_: BINDER_TYPE_FD,
            },
            flags: 0x7f | 0x100, // FLAT_BINDER_FLAG_ACCEPTS_FDS standard
            binder: (fd as u32) as u64,
            cookie,
        }
    }

    /// Return handle ID if this is a remote handle.
    pub const fn handle(&self) -> u32 {
        self.binder as u32
    }

    /// Return raw file descriptor if this is an FD object.
    pub const fn fd(&self) -> i32 {
        self.binder as u32 as i32
    }

    /// Return true if this is a local strong binder object.
    pub const fn is_binder(&self) -> bool {
        self.hdr.type_ == BINDER_TYPE_BINDER
    }

    /// Return true if this is a remote binder handle.
    pub const fn is_handle(&self) -> bool {
        self.hdr.type_ == BINDER_TYPE_HANDLE
    }

    /// Return true if this is a file descriptor object.
    pub const fn is_fd(&self) -> bool {
        self.hdr.type_ == BINDER_TYPE_FD
    }
}
