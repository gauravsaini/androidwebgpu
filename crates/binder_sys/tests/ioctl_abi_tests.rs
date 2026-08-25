//! Direct Linux Kernel Binder ioctl encoding and C-ABI layout verification tests.

use binder_sys::*;
use bytemuck::{bytes_of, from_bytes};

#[test]
fn test_kernel_ioctl_constants() {
    // Exact 64-bit Linux binder ioctl numbers
    assert_eq!(BINDER_WRITE_READ, 0xc0306201);
    assert_eq!(BINDER_SET_MAX_THREADS, 0x40046205);
    assert_eq!(BINDER_VERSION, 0xc0046209);
    assert_eq!(BINDER_THREAD_EXIT, 0x40046208);
    assert_eq!(BINDER_SET_CONTEXT_MGR, 0x40046207);
    assert_eq!(BINDER_CURRENT_PROTOCOL_VERSION, 8);
}

#[test]
fn test_binder_write_read_abi_layout() {
    assert_eq!(std::mem::size_of::<binder_write_read>(), 48);
    assert_eq!(std::mem::align_of::<binder_write_read>(), 8);

    let bwr = binder_write_read::new();
    assert_eq!(bwr.write_size, 0);
    assert_eq!(bwr.write_consumed, 0);
    assert_eq!(bwr.write_buffer, 0);
    assert_eq!(bwr.read_size, 0);
    assert_eq!(bwr.read_consumed, 0);
    assert_eq!(bwr.read_buffer, 0);

    let dummy_write = [1u8, 2, 3, 4];
    let mut dummy_read = [0u8; 8];
    let bwr2 = binder_write_read::with_buffers(
        dummy_write.as_ptr(),
        dummy_write.len(),
        dummy_read.as_mut_ptr(),
        dummy_read.len(),
    );

    assert_eq!(bwr2.write_size, 4);
    assert_eq!(bwr2.write_buffer, dummy_write.as_ptr() as u64);
    assert_eq!(bwr2.read_size, 8);
    assert_eq!(bwr2.read_buffer, dummy_read.as_mut_ptr() as u64);

    // Byte serialization round-trip
    let bytes = bwr2.as_bytes();
    assert_eq!(bytes.len(), 48);
    let parsed = from_bytes::<binder_write_read>(bytes);
    assert_eq!(*parsed, bwr2);
}

#[test]
fn test_binder_version_abi_layout() {
    assert_eq!(std::mem::size_of::<binder_version>(), 4);
    assert_eq!(std::mem::align_of::<binder_version>(), 4);

    let ver = binder_version::new(8);
    assert_eq!(ver.protocol_version, 8);

    let bytes = ver.as_bytes();
    assert_eq!(bytes.len(), 4);
    let parsed = from_bytes::<binder_version>(bytes);
    assert_eq!(*parsed, ver);
}

#[test]
fn test_binder_transaction_data_abi_layout() {
    assert_eq!(std::mem::size_of::<BinderTransactionData>(), 64);
    assert_eq!(std::mem::align_of::<BinderTransactionData>(), 8);

    let tr = BinderTransactionData::new(
        10, 0x1234, 1, 0, 1001, 1000, 128, 16, 0x1000, 0x2000,
    );

    assert_eq!(tr.target, 10);
    assert_eq!(tr.target_handle(), 10);
    assert_eq!(tr.cookie, 0x1234);
    assert_eq!(tr.code, 1);
    assert_eq!(tr.flags, 0);
    assert!(!tr.is_one_way());
    assert_eq!(tr.sender_pid, 1001);
    assert_eq!(tr.sender_euid, 1000);
    assert_eq!(tr.data_size, 128);
    assert_eq!(tr.offsets_size, 16);
    assert_eq!(tr.data_buffer, 0x1000);
    assert_eq!(tr.offsets_buffer, 0x2000);

    let bytes = tr.as_bytes();
    assert_eq!(bytes.len(), 64);
    let parsed = BinderTransactionData::from_bytes(bytes).unwrap();
    assert_eq!(parsed, tr);
}

#[test]
fn test_binder_cookie_structs_layout() {
    assert_eq!(std::mem::size_of::<BinderPtrCookie>(), 16);
    assert_eq!(std::mem::size_of::<BinderHandleCookie>(), 16);

    let pc = BinderPtrCookie {
        ptr: 0xdeadbeef,
        cookie: 0xcafebabe,
    };
    let pc_bytes = bytes_of(&pc);
    let pc_parsed = from_bytes::<BinderPtrCookie>(pc_bytes);
    assert_eq!(*pc_parsed, pc);

    let hc = BinderHandleCookie {
        handle: 42,
        padding: 0,
        cookie: 0xfeedface,
    };
    let hc_bytes = bytes_of(&hc);
    let hc_parsed = from_bytes::<BinderHandleCookie>(hc_bytes);
    assert_eq!(*hc_parsed, hc);
}
