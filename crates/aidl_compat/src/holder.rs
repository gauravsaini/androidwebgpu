//! Extensible parcelables (`ParcelableHolder`) and parcelable file descriptors.

use crate::status::{Result, Status, STATUS_BAD_VALUE};
use crate::traits::Parcelable;
use binder_rt::Parcel;

/// Dynamic container for parcelables conforming to AOSP `ParcelableHolder`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParcelableHolder {
    data: Option<Vec<u8>>,
    parcelable_name: Option<String>,
    stability: i32,
}

impl ParcelableHolder {
    /// Construct a new `ParcelableHolder` with specified stability level.
    pub fn new(stability: i32) -> Self {
        Self {
            data: None,
            parcelable_name: None,
            stability,
        }
    }

    /// Return the stability level of this holder.
    pub fn get_stability(&self) -> i32 {
        self.stability
    }

    /// Store a parcelable object into this holder.
    pub fn set_parcelable<T: Parcelable>(&mut self, parcelable: &T, name: &str) -> Result<()> {
        let mut parcel = Parcel::new();
        parcelable.write_to_parcel(&mut parcel)?;
        self.data = Some(parcel.into_parts().0);
        self.parcelable_name = Some(name.to_string());
        Ok(())
    }

    /// Retrieve and deserialize a parcelable object from this holder.
    pub fn get_parcelable<T: Parcelable + Default>(&self) -> Result<Option<T>> {
        match &self.data {
            None => Ok(None),
            Some(bytes) => {
                let parcel = Parcel::from_slice(bytes);
                let mut val = T::default();
                val.read_from_parcel(&parcel)?;
                Ok(Some(val))
            }
        }
    }

    /// Return the recorded name of the stored parcelable.
    pub fn get_parcelable_name(&self) -> Option<&str> {
        self.parcelable_name.as_deref()
    }

    /// Clear the stored parcelable and reset holder state.
    pub fn reset(&mut self) {
        self.data = None;
        self.parcelable_name = None;
    }

    /// Write this holder to parcel.
    pub fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        <Self as Parcelable>::write_to_parcel(self, parcel)
    }

    /// Read this holder from parcel.
    pub fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        <Self as Parcelable>::read_from_parcel(self, parcel)
    }

    /// Read this holder from parcel at specified offset.
    pub fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        <Self as Parcelable>::read_from_parcel_at(self, parcel, offset)
    }
}

impl Parcelable for ParcelableHolder {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_i32(self.stability)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(self.parcelable_name.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_byte_slice(self.data.as_deref())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        self.stability = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.parcelable_name = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.data = parcel
            .read_byte_vec(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

/// Parcelable wrapper for file descriptors / paravirtualized host resource handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParcelFileDescriptor {
    fd: i32,
}

impl ParcelFileDescriptor {
    /// Create a new `ParcelFileDescriptor`.
    pub fn new(fd: i32) -> Self {
        Self { fd }
    }

    /// Return raw file descriptor integer.
    pub fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    /// Consume wrapper and return raw file descriptor integer.
    pub fn into_raw_fd(self) -> i32 {
        self.fd
    }

    /// Write this descriptor to parcel.
    pub fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        <Self as Parcelable>::write_to_parcel(self, parcel)
    }

    /// Read this descriptor from parcel.
    pub fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        <Self as Parcelable>::read_from_parcel(self, parcel)
    }

    /// Read this descriptor from parcel at specified offset.
    pub fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        <Self as Parcelable>::read_from_parcel_at(self, parcel, offset)
    }
}

impl Default for ParcelFileDescriptor {
    fn default() -> Self {
        Self { fd: -1 }
    }
}

impl Parcelable for ParcelFileDescriptor {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> Result<()> {
        parcel
            .write_file_descriptor(self.fd, 0)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> Result<()> {
        self.fd = parcel
            .read_file_descriptor(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}
