//! Client-side AIDL proxy representations and transaction helpers.

use crate::pointer::SpIBinder;
use crate::status::{Result, Status, STATUS_BAD_VALUE};
use crate::traits::{Interface, Proxy};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::TF_ONE_WAY;
use binder_rt::Parcel;
use std::marker::PhantomData;

/// Generic proxy wrapper around an `SpIBinder` for interface `I`.
pub struct BpInterface<I: ?Sized> {
    binder: SpIBinder,
    _marker: PhantomData<fn() -> I>,
}

impl<I: ?Sized> BpInterface<I> {
    /// Construct a new `BpInterface` wrapping the given `SpIBinder`.
    pub fn new(binder: SpIBinder) -> Self {
        Self {
            binder,
            _marker: PhantomData,
        }
    }

    /// Return reference to wrapped `SpIBinder`.
    pub fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl<I: ?Sized + 'static> Interface for BpInterface<I> {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl<I: ?Sized + 'static> Proxy for BpInterface<I> {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl<I: ?Sized> Clone for BpInterface<I> {
    fn clone(&self) -> Self {
        Self {
            binder: self.binder.clone(),
            _marker: PhantomData,
        }
    }
}

impl<I: ?Sized> std::fmt::Debug for BpInterface<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BpInterface")
            .field("binder", &self.binder)
            .finish()
    }
}

/// Helper function to execute a synchronous RPC transaction and check the response status header.
pub fn transact_sync(
    binder: &SpIBinder,
    code: TransactionCode,
    flags: TransactionFlags,
    data: &Parcel,
) -> Result<(Parcel, usize)> {
    let mut reply = Parcel::new();
    binder.transact(code, flags, data, &mut reply)?;

    if (flags & TF_ONE_WAY) != 0 {
        return Ok((reply, 0));
    }

    let mut offset = 0;
    let status = reply
        .read_status(&mut offset)
        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

    if !status.is_ok() {
        return Err(status);
    }

    Ok((reply, offset))
}
