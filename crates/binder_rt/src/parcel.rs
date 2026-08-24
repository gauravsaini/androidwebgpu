//! Binary Parcel reader and writer implementation for Android Binder IPC.

use crate::status::{ExceptionCode, Status, StatusCode, EX_NONE, EX_SERVICE_SPECIFIC};
use crate::types::{BinderSizeT, FlatBinderObject, BINDER_TYPE_FD};
use bytemuck::Zeroable;

/// Errors that can occur during Parcel serialization or deserialization.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParcelError {
    #[error("Not enough data in parcel buffer: requested {requested} bytes at offset {offset}, available {available}")]
    NotEnoughData {
        offset: usize,
        requested: usize,
        available: usize,
    },
    #[error("Invalid alignment at offset {0}")]
    UnalignedOffset(usize),
    #[error("Malformed UTF-8 string at offset {0}")]
    MalformedUtf8(usize),
    #[error("Malformed UTF-16 string at offset {0}")]
    MalformedUtf16(usize),
    #[error("Missing null terminator for string at offset {0}")]
    MissingNullTerminator(usize),
    #[error("Bad parcelable format or negative count: {0}")]
    BadParcelable(i32),
    #[error("Null pointer exception")]
    NullPointer,
    #[error("Binder object offset {0} not registered in offsets table")]
    ObjectOffsetNotFound(usize),
    #[error("Invalid Binder object type: {0:#x}")]
    InvalidObjectType(u32),
}

/// Binary container and serializer for Android Binder IPC data.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parcel {
    data: Vec<u8>,
    offsets: Vec<BinderSizeT>,
}

impl Parcel {
    /// Create a new empty parcel.
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            offsets: Vec::new(),
        }
    }

    /// Create an empty parcel with pre-allocated buffer capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            offsets: Vec::new(),
        }
    }

    /// Construct a parcel directly from raw data bytes.
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
            offsets: Vec::new(),
        }
    }

    /// Construct a parcel with both data bytes and binder object offsets.
    pub fn from_parts(data: Vec<u8>, offsets: Vec<BinderSizeT>) -> Self {
        Self { data, offsets }
    }

    /// Return immutable reference to payload data bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Return mutable reference to payload data bytes.
    pub fn data_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }

    /// Return immutable slice of object offsets.
    pub fn offsets(&self) -> &[BinderSizeT] {
        &self.offsets
    }

    /// Return mutable reference to offsets.
    pub fn offsets_mut(&mut self) -> &mut Vec<BinderSizeT> {
        &mut self.offsets
    }

    /// Return length of data in bytes.
    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    /// Return length of offsets array in bytes (number of offsets * 8).
    pub fn offsets_size(&self) -> usize {
        self.offsets.len() * std::mem::size_of::<BinderSizeT>()
    }

    /// Return true if the parcel contains no data.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all data and offsets.
    pub fn clear(&mut self) {
        self.data.clear();
        self.offsets.clear();
    }

    /// Deconstruct the parcel into its constituent parts.
    pub fn into_parts(self) -> (Vec<u8>, Vec<BinderSizeT>) {
        (self.data, self.offsets)
    }

    /// Append all data and offset entries from another parcel.
    pub fn append_all(&mut self, other: &Parcel) {
        let base_offset = self.data.len() as BinderSizeT;
        for &offset in &other.offsets {
            self.offsets.push(base_offset + offset);
        }
        self.data.extend_from_slice(&other.data);
    }

    // -------------------------------------------------------------------------
    // Internal Alignment and Padding Helpers
    // -------------------------------------------------------------------------

    /// Strict 4-byte padding formula: `(4 - (len % 4)) % 4`.
    #[inline(always)]
    pub const fn padding_size(len: usize) -> usize {
        (4 - (len % 4)) % 4
    }

    #[inline(always)]
    fn pad_to_4bytes(&mut self, written_len: usize) {
        let pad = Self::padding_size(written_len);
        if pad > 0 {
            self.data.extend_from_slice(&[0u8; 3][..pad]);
        }
    }

    #[inline(always)]
    fn check_bounds(data: &[u8], offset: usize, requested: usize) -> Result<(), ParcelError> {
        let available = data.len().saturating_sub(offset);
        if available < requested {
            Err(ParcelError::NotEnoughData {
                offset,
                requested,
                available,
            })
        } else {
            Ok(())
        }
    }

    // -------------------------------------------------------------------------
    // Primitives
    // -------------------------------------------------------------------------

    /// Write boolean value (encoded as 4-byte i32: 1 for true, 0 for false).
    pub fn write_bool(&mut self, val: bool) -> Result<(), ParcelError> {
        self.write_i32(if val { 1 } else { 0 })
    }

    /// Read boolean value (reads 4-byte i32, returns val != 0).
    pub fn read_bool(&self, offset: &mut usize) -> Result<bool, ParcelError> {
        let val = self.read_i32(offset)?;
        Ok(val != 0)
    }

    /// Write signed 8-bit integer (4-byte aligned scalar write).
    pub fn write_i8(&mut self, val: i8) -> Result<(), ParcelError> {
        self.write_i32(val as i32)
    }

    /// Read signed 8-bit integer.
    pub fn read_i8(&self, offset: &mut usize) -> Result<i8, ParcelError> {
        let val = self.read_i32(offset)?;
        Ok(val as i8)
    }

    /// Write unsigned 8-bit integer (4-byte aligned scalar write).
    pub fn write_u8(&mut self, val: u8) -> Result<(), ParcelError> {
        self.write_i32(val as i32)
    }

    /// Read unsigned 8-bit integer.
    pub fn read_u8(&self, offset: &mut usize) -> Result<u8, ParcelError> {
        let val = self.read_i32(offset)?;
        Ok(val as u8)
    }

    /// Write signed 16-bit integer (4-byte aligned scalar write).
    pub fn write_i16(&mut self, val: i16) -> Result<(), ParcelError> {
        self.write_i32(val as i32)
    }

    /// Read signed 16-bit integer.
    pub fn read_i16(&self, offset: &mut usize) -> Result<i16, ParcelError> {
        let val = self.read_i32(offset)?;
        Ok(val as i16)
    }

    /// Write unsigned 16-bit integer (4-byte aligned scalar write).
    pub fn write_u16(&mut self, val: u16) -> Result<(), ParcelError> {
        self.write_i32(val as i32)
    }

    /// Read unsigned 16-bit integer.
    pub fn read_u16(&self, offset: &mut usize) -> Result<u16, ParcelError> {
        let val = self.read_i32(offset)?;
        Ok(val as u16)
    }

    /// Write signed 32-bit integer (little-endian).
    pub fn write_i32(&mut self, val: i32) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Read signed 32-bit integer (little-endian).
    pub fn read_i32(&self, offset: &mut usize) -> Result<i32, ParcelError> {
        Self::check_bounds(&self.data, *offset, 4)?;
        let bytes = [
            self.data[*offset],
            self.data[*offset + 1],
            self.data[*offset + 2],
            self.data[*offset + 3],
        ];
        *offset += 4;
        Ok(i32::from_le_bytes(bytes))
    }

    /// Write unsigned 32-bit integer (little-endian).
    pub fn write_u32(&mut self, val: u32) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Read unsigned 32-bit integer (little-endian).
    pub fn read_u32(&self, offset: &mut usize) -> Result<u32, ParcelError> {
        Self::check_bounds(&self.data, *offset, 4)?;
        let bytes = [
            self.data[*offset],
            self.data[*offset + 1],
            self.data[*offset + 2],
            self.data[*offset + 3],
        ];
        *offset += 4;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Write signed 64-bit integer (little-endian).
    pub fn write_i64(&mut self, val: i64) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Read signed 64-bit integer (little-endian).
    pub fn read_i64(&self, offset: &mut usize) -> Result<i64, ParcelError> {
        Self::check_bounds(&self.data, *offset, 8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[*offset..*offset + 8]);
        *offset += 8;
        Ok(i64::from_le_bytes(bytes))
    }

    /// Write unsigned 64-bit integer (little-endian).
    pub fn write_u64(&mut self, val: u64) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_le_bytes());
        Ok(())
    }

    /// Read unsigned 64-bit integer (little-endian).
    pub fn read_u64(&self, offset: &mut usize) -> Result<u64, ParcelError> {
        Self::check_bounds(&self.data, *offset, 8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[*offset..*offset + 8]);
        *offset += 8;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Write 32-bit floating point value.
    pub fn write_f32(&mut self, val: f32) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_bits().to_le_bytes());
        Ok(())
    }

    /// Read 32-bit floating point value.
    pub fn read_f32(&self, offset: &mut usize) -> Result<f32, ParcelError> {
        let bits = self.read_u32(offset)?;
        Ok(f32::from_bits(bits))
    }

    /// Write 64-bit floating point value.
    pub fn write_f64(&mut self, val: f64) -> Result<(), ParcelError> {
        self.data.extend_from_slice(&val.to_bits().to_le_bytes());
        Ok(())
    }

    /// Read 64-bit floating point value.
    pub fn read_f64(&self, offset: &mut usize) -> Result<f64, ParcelError> {
        let bits = self.read_u64(offset)?;
        Ok(f64::from_bits(bits))
    }

    /// Write 16-bit char (encoded as 4-byte unsigned integer).
    pub fn write_char(&mut self, val: char) -> Result<(), ParcelError> {
        self.write_u32(val as u32)
    }

    /// Read 16-bit char.
    pub fn read_char(&self, offset: &mut usize) -> Result<char, ParcelError> {
        let val = self.read_u32(offset)?;
        char::from_u32(val).ok_or(ParcelError::MalformedUtf16(*offset))
    }

    // -------------------------------------------------------------------------
    // Strings (UTF-8 and UTF-16)
    // -------------------------------------------------------------------------

    /// Write UTF-8 string: `i32` length + UTF-8 bytes + `0x00` null terminator + 4-byte padding.
    /// If `val` is `None`, writes length `-1` with no payload.
    pub fn write_utf8(&mut self, val: Option<&str>) -> Result<(), ParcelError> {
        match val {
            None => {
                self.write_i32(-1)?;
            }
            Some(s) => {
                let bytes = s.as_bytes();
                self.write_i32(bytes.len() as i32)?;
                self.data.extend_from_slice(bytes);
                self.data.push(0x00);
                self.pad_to_4bytes(bytes.len() + 1);
            }
        }
        Ok(())
    }

    /// Read UTF-8 string: reads `i32` byte count, string bytes, null terminator, and padding.
    pub fn read_utf8(&self, offset: &mut usize) -> Result<Option<String>, ParcelError> {
        let len = self.read_i32(offset)?;
        if len == -1 {
            return Ok(None);
        }
        if len < 0 {
            return Err(ParcelError::BadParcelable(len));
        }

        let str_len = len as usize;
        let payload_len = str_len + 1; // bytes + null terminator
        let total_advance = payload_len + Self::padding_size(payload_len);

        Self::check_bounds(&self.data, *offset, total_advance)?;

        let str_bytes = &self.data[*offset..*offset + str_len];
        let term = self.data[*offset + str_len];
        if term != 0x00 {
            return Err(ParcelError::MissingNullTerminator(*offset + str_len));
        }

        let s = std::str::from_utf8(str_bytes)
            .map_err(|_| ParcelError::MalformedUtf8(*offset))?
            .to_string();

        *offset += total_advance;
        Ok(Some(s))
    }

    /// Write UTF-16LE string: `i32` code-unit count + UTF-16LE bytes + `0x0000` null terminator + padding.
    /// If `val` is `None`, writes length `-1` with no payload.
    pub fn write_utf16(&mut self, val: Option<&str>) -> Result<(), ParcelError> {
        match val {
            None => {
                self.write_i32(-1)?;
            }
            Some(s) => {
                let units: Vec<u16> = s.encode_utf16().collect();
                self.write_i32(units.len() as i32)?;
                for unit in &units {
                    self.data.extend_from_slice(&unit.to_le_bytes());
                }
                self.data.extend_from_slice(&0u16.to_le_bytes()); // 0x0000
                let byte_count = (units.len() + 1) * 2;
                self.pad_to_4bytes(byte_count);
            }
        }
        Ok(())
    }

    /// Read UTF-16LE string: reads `i32` character count, UTF-16LE code units, null terminator, and padding.
    pub fn read_utf16(&self, offset: &mut usize) -> Result<Option<String>, ParcelError> {
        let char_count = self.read_i32(offset)?;
        if char_count == -1 {
            return Ok(None);
        }
        if char_count < 0 {
            return Err(ParcelError::BadParcelable(char_count));
        }

        let count = char_count as usize;
        let byte_count = (count + 1) * 2;
        let total_advance = byte_count + Self::padding_size(byte_count);

        Self::check_bounds(&self.data, *offset, total_advance)?;

        let mut units = Vec::with_capacity(count);
        for i in 0..count {
            let unit_off = *offset + i * 2;
            let unit = u16::from_le_bytes([self.data[unit_off], self.data[unit_off + 1]]);
            units.push(unit);
        }

        let term_off = *offset + count * 2;
        let term = u16::from_le_bytes([self.data[term_off], self.data[term_off + 1]]);
        if term != 0x0000 {
            return Err(ParcelError::MissingNullTerminator(term_off));
        }

        let s = String::from_utf16(&units).map_err(|_| ParcelError::MalformedUtf16(*offset))?;

        *offset += total_advance;
        Ok(Some(s))
    }

    // -------------------------------------------------------------------------
    // Raw Byte Arrays and Vectors
    // -------------------------------------------------------------------------

    /// Write raw byte slice: `i32` length + raw bytes + 4-byte padding.
    pub fn write_byte_slice(&mut self, val: Option<&[u8]>) -> Result<(), ParcelError> {
        match val {
            None => {
                self.write_i32(-1)?;
            }
            Some(bytes) => {
                self.write_i32(bytes.len() as i32)?;
                self.data.extend_from_slice(bytes);
                self.pad_to_4bytes(bytes.len());
            }
        }
        Ok(())
    }

    /// Read raw byte vector: `i32` length + bytes + padding.
    pub fn read_byte_vec(&self, offset: &mut usize) -> Result<Option<Vec<u8>>, ParcelError> {
        let len = self.read_i32(offset)?;
        if len == -1 {
            return Ok(None);
        }
        if len < 0 {
            return Err(ParcelError::BadParcelable(len));
        }

        let count = len as usize;
        let total_advance = count + Self::padding_size(count);

        Self::check_bounds(&self.data, *offset, total_advance)?;

        let bytes = self.data[*offset..*offset + count].to_vec();
        *offset += total_advance;
        Ok(Some(bytes))
    }

    /// Write generic vector: `i32` count + serialized elements.
    pub fn write_vector<T, F>(
        &mut self,
        val: Option<&[T]>,
        mut write_elem: F,
    ) -> Result<(), ParcelError>
    where
        F: FnMut(&mut Parcel, &T) -> Result<(), ParcelError>,
    {
        match val {
            None => {
                self.write_i32(-1)?;
            }
            Some(slice) => {
                self.write_i32(slice.len() as i32)?;
                for elem in slice {
                    write_elem(self, elem)?;
                }
            }
        }
        Ok(())
    }

    /// Read generic vector: `i32` count + deserialized elements.
    pub fn read_vector<T, F>(
        &self,
        offset: &mut usize,
        mut read_elem: F,
    ) -> Result<Option<Vec<T>>, ParcelError>
    where
        F: FnMut(&Parcel, &mut usize) -> Result<T, ParcelError>,
    {
        let len = self.read_i32(offset)?;
        if len == -1 {
            return Ok(None);
        }
        if len < 0 {
            return Err(ParcelError::BadParcelable(len));
        }

        let count = len as usize;
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            result.push(read_elem(self, offset)?);
        }
        Ok(Some(result))
    }

    // -------------------------------------------------------------------------
    // Nullables & Presence Flags
    // -------------------------------------------------------------------------

    /// Write nullable value using standard AIDL presence flag (`0` for None, `1` for Some).
    pub fn write_nullable_presence<T, F>(
        &mut self,
        val: Option<&T>,
        mut write_elem: F,
    ) -> Result<(), ParcelError>
    where
        F: FnMut(&mut Parcel, &T) -> Result<(), ParcelError>,
    {
        match val {
            None => self.write_i32(0),
            Some(inner) => {
                self.write_i32(1)?;
                write_elem(self, inner)
            }
        }
    }

    /// Read nullable value using standard AIDL presence flag.
    pub fn read_nullable_presence<T, F>(
        &self,
        offset: &mut usize,
        mut read_elem: F,
    ) -> Result<Option<T>, ParcelError>
    where
        F: FnMut(&Parcel, &mut usize) -> Result<T, ParcelError>,
    {
        let present = self.read_i32(offset)?;
        if present == 0 {
            Ok(None)
        } else {
            Ok(Some(read_elem(self, offset)?))
        }
    }

    // -------------------------------------------------------------------------
    // Objects and File Descriptors
    // -------------------------------------------------------------------------

    /// Write a 24-byte `FlatBinderObject` and register its offset in the offsets array.
    pub fn write_binder_object(&mut self, obj: &FlatBinderObject) -> Result<(), ParcelError> {
        let obj_offset = self.data.len() as BinderSizeT;
        self.offsets.push(obj_offset);
        self.data.extend_from_slice(bytemuck::bytes_of(obj));
        Ok(())
    }

    /// Read a 24-byte `FlatBinderObject`.
    pub fn read_binder_object(&self, offset: &mut usize) -> Result<FlatBinderObject, ParcelError> {
        Self::check_bounds(&self.data, *offset, std::mem::size_of::<FlatBinderObject>())?;

        let obj_offset = *offset as BinderSizeT;
        if !self.offsets.is_empty() && !self.offsets.contains(&obj_offset) {
            return Err(ParcelError::ObjectOffsetNotFound(*offset));
        }

        let slice = &self.data[*offset..*offset + 24];
        let mut obj = FlatBinderObject::zeroed();
        bytemuck::bytes_of_mut(&mut obj).copy_from_slice(slice);

        *offset += 24;
        Ok(obj)
    }

    /// Write a remote binder handle object.
    pub fn write_binder(&mut self, handle: u32, cookie: u64) -> Result<(), ParcelError> {
        let obj = FlatBinderObject::new_handle(handle, 0, cookie);
        self.write_binder_object(&obj)
    }

    /// Read a binder object, returning `FlatBinderObject`.
    pub fn read_binder(&self, offset: &mut usize) -> Result<FlatBinderObject, ParcelError> {
        self.read_binder_object(offset)
    }

    /// Write a file descriptor object.
    pub fn write_file_descriptor(&mut self, fd: i32, cookie: u64) -> Result<(), ParcelError> {
        let obj = FlatBinderObject::new_fd(fd, cookie);
        self.write_binder_object(&obj)
    }

    /// Read a file descriptor object, returning raw fd.
    pub fn read_file_descriptor(&self, offset: &mut usize) -> Result<i32, ParcelError> {
        let obj = self.read_binder_object(offset)?;
        if obj.hdr.type_ != BINDER_TYPE_FD {
            return Err(ParcelError::InvalidObjectType(obj.hdr.type_));
        }
        Ok(obj.fd())
    }

    // -------------------------------------------------------------------------
    // AIDL Status Code Serialization
    // -------------------------------------------------------------------------

    /// Serialize an AIDL `Status` to this parcel.
    pub fn write_status(&mut self, status: &Status) -> Result<(), ParcelError> {
        let exception_code = status.exception as i32;
        self.write_i32(exception_code)?;

        if exception_code == EX_NONE {
            return Ok(());
        }

        if exception_code == EX_SERVICE_SPECIFIC {
            self.write_i32(status.service_specific_error)?;
        }

        self.write_utf8(status.message.as_deref())?;
        Ok(())
    }

    /// Deserialize an AIDL `Status` from this parcel.
    pub fn read_status(&self, offset: &mut usize) -> Result<Status, ParcelError> {
        let exception_code = self.read_i32(offset)?;
        if exception_code == EX_NONE {
            return Ok(Status::ok());
        }

        let exception = ExceptionCode::from(exception_code);
        let service_specific_error = if exception_code == EX_SERVICE_SPECIFIC {
            self.read_i32(offset)?
        } else {
            0
        };

        let message = self.read_utf8(offset)?;

        Ok(Status {
            exception,
            service_specific_error,
            status: if exception == ExceptionCode::TransactionFailed {
                StatusCode::FailedTransaction
            } else {
                StatusCode::Ok
            },
            message,
        })
    }
}
