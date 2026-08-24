//! Core Binder and AIDL interface traits.

use crate::death::DeathRecipient;
use crate::pointer::{SpIBinder, Strong};
use crate::status::Result;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use std::sync::Arc;

/// Base trait for all AIDL interfaces.
pub trait Interface: Send + Sync + 'static {
    /// Retrieve the underlying `SpIBinder` representation of this interface.
    fn as_binder(&self) -> SpIBinder;

    /// Optional debug dump method.
    fn dump(&self, _writer: &mut dyn std::io::Write, _args: &[&str]) -> Result<()> {
        Ok(())
    }
}

/// Core abstraction representing a local or remote Binder object.
pub trait IBinder: Send + Sync + 'static {
    /// Perform an IPC transaction against this Binder object.
    fn transact(
        &self,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()>;

    /// Check if the remote Binder process/object is still alive.
    fn is_binder_alive(&self) -> bool {
        true
    }

    /// Ping the Binder object (`PING_TRANSACTION`).
    fn ping_binder(&self) -> Result<()> {
        let data = Parcel::new();
        let mut reply = Parcel::new();
        self.transact(binder_rt::PING_TRANSACTION, 0, &data, &mut reply)
    }

    /// Link a `DeathRecipient` callback to receive notifications when this Binder dies.
    fn link_to_death(&self, recipient: Arc<dyn DeathRecipient>) -> Result<()>;

    /// Unlink a previously registered `DeathRecipient`.
    fn unlink_to_death(&self, recipient: &Arc<dyn DeathRecipient>) -> Result<()>;

    /// Return reference to server-side remotable dispatcher if local.
    fn as_transactable(&self) -> Option<&dyn Remotable> {
        None
    }

    /// Return the AIDL class descriptor string if known.
    fn get_class_descriptor(&self) -> Option<&'static str> {
        None
    }

    /// Return remote handle ID if this is a remote proxy.
    fn handle(&self) -> Option<u32> {
        None
    }
}

/// Trait implemented by server-side AIDL stubs (`Bn*`) to dispatch transactions.
pub trait Remotable: Send + Sync + 'static {
    /// Return the canonical AIDL interface descriptor (e.g. `"android.os.ITestService"`).
    fn get_class_descriptor() -> &'static str
    where
        Self: Sized;

    /// Dispatch incoming transaction call to the appropriate interface method.
    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()>;
}

/// Trait implemented by client-side AIDL proxies (`Bp*`).
pub trait Proxy: Send + Sync + 'static {
    /// Return a reference to the wrapped `SpIBinder`.
    fn as_binder(&self) -> &SpIBinder;
}

/// Conversion trait from a generic `SpIBinder` to a strongly typed AIDL interface handle.
pub trait FromIBinder: Interface {
    /// Attempt to convert an `SpIBinder` into `Strong<Self>`.
    fn try_from(binder: SpIBinder) -> Result<Strong<Self>>;
}

/// Trait for types that can be marshaled into and unmarshaled from a `Parcel`.
pub trait Parcelable: Sized {
    /// Write this object into the provided `Parcel`.
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()>;

    /// Read this object from the provided `Parcel`.
    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    /// Read this object from the provided `Parcel` starting at the given byte offset.
    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()>;
}

// -----------------------------------------------------------------------------
// Built-in Parcelable Implementations for Standard Types
// -----------------------------------------------------------------------------

impl Parcelable for bool {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_bool(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_bool(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for i8 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_i8(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_i8(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for u8 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_u8(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_u8(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for i16 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_i16(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_i16(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for u16 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_u16(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_u16(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for i32 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_i32(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_i32(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for u32 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_u32(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_u32(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for i64 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_i64(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_i64(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for u64 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_u64(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_u64(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for f32 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_f32(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_f32(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for f64 {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_f64(*self)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        *self = parcel
            .read_f64(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

impl Parcelable for String {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_utf8(Some(self.as_str()))
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        let s = parcel
            .read_utf8(offset)
            .map_err(|_| crate::status::Status::from_status(crate::status::STATUS_BAD_VALUE))?;
        *self = s.unwrap_or_default();
        Ok(())
    }
}
