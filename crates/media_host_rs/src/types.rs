//! Media Codec Data Models, Buffer Flags, and Parcelables.

use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::Parcelable;
use binder_rt::Parcel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// -----------------------------------------------------------------------------
// Buffer Flags & Dequeue Status Codes
// -----------------------------------------------------------------------------

pub const BUFFER_FLAG_KEY_FRAME: u32 = 1;
pub const BUFFER_FLAG_CODEC_CONFIG: u32 = 2;
pub const BUFFER_FLAG_END_OF_STREAM: u32 = 4;
pub const BUFFER_FLAG_PARTIAL_FRAME: u32 = 8;

pub const INFO_TRY_AGAIN_LATER: i32 = -1;
pub const INFO_OUTPUT_FORMAT_CHANGED: i32 = -2;
pub const INFO_OUTPUT_BUFFERS_CHANGED: i32 = -3;

pub const CONFIGURE_FLAG_ENCODE: u32 = 1;

// -----------------------------------------------------------------------------
// MediaFormat
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MediaFormat {
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bitrate: u32,
    pub frame_rate: u32,
    pub color_format: u32,
    pub csd_0: Vec<u8>,
    pub csd_1: Vec<u8>,
    pub string_keys: HashMap<String, String>,
    pub int_keys: HashMap<String, i32>,
}

impl MediaFormat {
    pub fn new_video_format(mime: &str, width: u32, height: u32) -> Self {
        Self {
            mime: mime.to_string(),
            width,
            height,
            bitrate: 2_000_000,
            frame_rate: 30,
            color_format: 19, // COLOR_FormatYUV420Planar
            csd_0: Vec::new(),
            csd_1: Vec::new(),
            string_keys: HashMap::new(),
            int_keys: HashMap::new(),
        }
    }

    pub fn set_string(&mut self, key: &str, value: &str) {
        self.string_keys.insert(key.to_string(), value.to_string());
    }

    pub fn set_int(&mut self, key: &str, value: i32) {
        self.int_keys.insert(key.to_string(), value);
    }
}

impl Parcelable for MediaFormat {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        let json = serde_json::to_string(self)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&json))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let json = parcel
            .read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        if !json.is_empty() {
            *self = serde_json::from_str(&json)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// BufferInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BufferInfo {
    pub offset: u32,
    pub size: u32,
    pub presentation_time_us: i64,
    pub flags: u32,
}

impl BufferInfo {
    pub fn new(offset: u32, size: u32, presentation_time_us: i64, flags: u32) -> Self {
        Self {
            offset,
            size,
            presentation_time_us,
            flags,
        }
    }

    pub fn is_key_frame(&self) -> bool {
        (self.flags & BUFFER_FLAG_KEY_FRAME) != 0
    }

    pub fn is_eos(&self) -> bool {
        (self.flags & BUFFER_FLAG_END_OF_STREAM) != 0
    }
}

impl Parcelable for BufferInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u32(self.offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i64(self.presentation_time_us).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.flags).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.offset = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.size = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.presentation_time_us = parcel.read_i64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.flags = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// MediaCodecInfo
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCodecInfo {
    pub name: String,
    pub mime_types: Vec<String>,
    pub is_encoder: bool,
}

impl Parcelable for MediaCodecInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_utf8(Some(&self.name)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.mime_types.len() as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for m in &self.mime_types {
            parcel.write_utf8(Some(m)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        parcel.write_bool(self.is_encoder).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.name = parcel.read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        let count = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.mime_types.clear();
        for _ in 0..count {
            if let Ok(Some(m)) = parcel.read_utf8(offset) {
                self.mime_types.push(m);
            }
        }
        self.is_encoder = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}
