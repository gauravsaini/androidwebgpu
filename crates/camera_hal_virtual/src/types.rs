//! Data models, enumerations, and Parcelable implementations for Camera HAL.

use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::Parcelable;
use binder_rt::Parcel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// -----------------------------------------------------------------------------
// Pixel Formats & Constants
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum PixelFormat {
    Rgba8888 = 1,       // HAL_PIXEL_FORMAT_RGBA_8888
    Rgbx8888 = 2,       // HAL_PIXEL_FORMAT_RGBX_8888
    Rgb888 = 3,         // HAL_PIXEL_FORMAT_RGB_888
    Rgb565 = 4,         // HAL_PIXEL_FORMAT_RGB_565
    Raw16 = 0x20,       // HAL_PIXEL_FORMAT_RAW16
    Blob = 0x21,        // HAL_PIXEL_FORMAT_BLOB (JPEG)
    Yuv420888 = 0x23,   // HAL_PIXEL_FORMAT_YCrCb_420_SP / YUV_420_888
    YV12 = 0x32315659,  // HAL_PIXEL_FORMAT_YV12
}

impl PixelFormat {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(Self::Rgba8888),
            2 => Some(Self::Rgbx8888),
            3 => Some(Self::Rgb888),
            4 => Some(Self::Rgb565),
            0x20 => Some(Self::Raw16),
            0x21 => Some(Self::Blob),
            0x23 => Some(Self::Yuv420888),
            0x32315659 => Some(Self::YV12),
            _ => None,
        }
    }

    pub fn bytes_per_pixel(&self) -> f32 {
        match self {
            Self::Rgba8888 | Self::Rgbx8888 => 4.0,
            Self::Rgb888 => 3.0,
            Self::Rgb565 | Self::Raw16 => 2.0,
            Self::Yuv420888 | Self::YV12 => 1.5,
            Self::Blob => 1.0,
        }
    }
}

// -----------------------------------------------------------------------------
// Stream Types & Rotation
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum StreamType {
    Output = 0,
    Input = 1,
}

impl StreamType {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Output),
            1 => Some(Self::Input),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum StreamRotation {
    Rotation0 = 0,
    Rotation90 = 1,
    Rotation180 = 2,
    Rotation270 = 3,
}

impl StreamRotation {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Rotation0),
            1 => Some(Self::Rotation90),
            2 => Some(Self::Rotation180),
            3 => Some(Self::Rotation270),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Request Templates
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum RequestTemplate {
    Preview = 1,
    StillCapture = 2,
    Record = 3,
    VideoSnapshot = 4,
    ZeroShutterLag = 5,
    Manual = 6,
}

impl RequestTemplate {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(Self::Preview),
            2 => Some(Self::StillCapture),
            3 => Some(Self::Record),
            4 => Some(Self::VideoSnapshot),
            5 => Some(Self::ZeroShutterLag),
            6 => Some(Self::Manual),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Camera Metadata
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CameraMetadata {
    pub entries: HashMap<String, String>,
}

impl CameraMetadata {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.entries.insert(key.to_string(), value.to_string());
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Construct static Camera characteristics metadata for Virtual Camera.
    pub fn new_virtual_camera_characteristics() -> Self {
        let mut meta = Self::new();
        // Lens facing: 0 = Front, 1 = Back
        meta.set("android.lens.facing", "1");
        meta.set("android.sensor.orientation", "90");
        meta.set("android.flash.info.available", "false");
        meta.set("android.control.aeAvailableTargetFpsRanges", "[[15, 30], [30, 30], [30, 60]]");
        meta.set("android.scaler.availableStreamConfigurations",
            "[{\"width\": 1280, \"height\": 720, \"format\": 35}, \
              {\"width\": 640, \"height\": 480, \"format\": 35}, \
              {\"width\": 1280, \"height\": 720, \"format\": 1}, \
              {\"width\": 640, \"height\": 480, \"format\": 1}]"
        );
        meta.set("android.scaler.availableMaxDigitalZoom", "4.0");
        meta.set("android.request.maxNumOutputStreams", "[1, 2, 1]"); // raw, non-raw, jpeg
        meta.set("android.request.pipelineMaxDepth", "4");
        meta
    }

    /// Construct default request settings for a given template.
    pub fn new_default_request_settings(template: RequestTemplate) -> Self {
        let mut meta = Self::new();
        meta.set("android.control.mode", "1"); // AUTO
        meta.set("android.control.aeMode", "1"); // ON
        meta.set("android.control.afMode", "4"); // CONTINUOUS_PICTURE
        meta.set("android.control.awbMode", "1"); // AUTO
        match template {
            RequestTemplate::Preview => {
                meta.set("android.control.aeTargetFpsRange", "[30, 30]");
            }
            RequestTemplate::StillCapture => {
                meta.set("android.control.captureIntent", "2"); // STILL_CAPTURE
                meta.set("android.jpeg.quality", "95");
            }
            RequestTemplate::Record => {
                meta.set("android.control.aeTargetFpsRange", "[30, 60]");
                meta.set("android.control.captureIntent", "3"); // VIDEO_RECORD
            }
            _ => {
                meta.set("android.control.aeTargetFpsRange", "[30, 30]");
            }
        }
        meta
    }
}

impl Parcelable for CameraMetadata {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        let json = serde_json::to_string(&self.entries)
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
            self.entries = serde_json::from_str(&json)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            self.entries.clear();
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Stream & StreamConfiguration
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stream {
    pub id: i32,
    pub stream_type: StreamType,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub usage: u64,
    pub data_space: u32,
    pub rotation: StreamRotation,
    pub physical_camera_id: String,
    pub buffer_size: u32,
}

impl Default for Stream {
    fn default() -> Self {
        Self {
            id: 0,
            stream_type: StreamType::Output,
            width: 1280,
            height: 720,
            format: PixelFormat::Yuv420888,
            usage: 0,
            data_space: 0,
            rotation: StreamRotation::Rotation0,
            physical_camera_id: String::new(),
            buffer_size: (1280 * 720 * 3 / 2) as u32,
        }
    }
}

impl Parcelable for Stream {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.stream_type as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.width).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.height).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.format as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.usage).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.data_space).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.rotation as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.physical_camera_id)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.buffer_size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.id = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let st = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.stream_type = StreamType::from_i32(st).ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        self.width = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.height = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.format = PixelFormat::from_i32(fmt).ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        self.usage = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.data_space = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let rot = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.rotation = StreamRotation::from_i32(rot).ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        self.physical_camera_id = parcel.read_utf8(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.buffer_size = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StreamConfiguration {
    pub streams: Vec<Stream>,
    pub operation_mode: u32,
}

impl Parcelable for StreamConfiguration {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.streams.len() as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for stream in &self.streams {
            stream.write_to_parcel(parcel)?;
        }
        parcel.write_u32(self.operation_mode).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let count = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.streams.clear();
        for _ in 0..count {
            let mut stream = Stream::default();
            stream.read_from_parcel_at(parcel, offset)?;
            self.streams.push(stream);
        }
        self.operation_mode = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// HalStream & HalStreamConfiguration
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HalStream {
    pub id: i32,
    pub override_format: PixelFormat,
    pub producer_usage: u64,
    pub consumer_usage: u64,
    pub max_buffers: u32,
    pub override_data_space: u32,
}

impl Default for HalStream {
    fn default() -> Self {
        Self {
            id: 0,
            override_format: PixelFormat::Yuv420888,
            producer_usage: 0x3,
            consumer_usage: 0x3,
            max_buffers: 4,
            override_data_space: 0,
        }
    }
}

impl Parcelable for HalStream {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.override_format as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.producer_usage).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.consumer_usage).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.max_buffers).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.override_data_space).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.id = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.override_format = PixelFormat::from_i32(fmt).ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        self.producer_usage = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.consumer_usage = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.max_buffers = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.override_data_space = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HalStreamConfiguration {
    pub streams: Vec<HalStream>,
}

impl Parcelable for HalStreamConfiguration {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.streams.len() as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for stream in &self.streams {
            stream.write_to_parcel(parcel)?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let count = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.streams.clear();
        for _ in 0..count {
            let mut stream = HalStream::default();
            stream.read_from_parcel_at(parcel, offset)?;
            self.streams.push(stream);
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// StreamBuffer & BufferStatus
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum BufferStatus {
    Ok = 0,
    Error = 1,
}

impl BufferStatus {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Ok),
            1 => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamBuffer {
    pub stream_id: i32,
    pub buffer_id: u64,
    pub buffer_data: Vec<u8>,
    pub status: BufferStatus,
}

impl Default for StreamBuffer {
    fn default() -> Self {
        Self {
            stream_id: 0,
            buffer_id: 0,
            buffer_data: Vec::new(),
            status: BufferStatus::Ok,
        }
    }
}

impl Parcelable for StreamBuffer {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_i32(self.stream_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.buffer_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_i32(self.status as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_byte_slice(Some(&self.buffer_data)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.stream_id = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.buffer_id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let st = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.status = BufferStatus::from_i32(st).ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;
        self.buffer_data = parcel.read_byte_vec(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_default();
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// CaptureRequest & CaptureResult
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CaptureRequest {
    pub frame_number: u32,
    pub fmq_settings_size: u64,
    pub settings: CameraMetadata,
    pub input_buffer: Option<StreamBuffer>,
    pub output_buffers: Vec<StreamBuffer>,
}

impl Parcelable for CaptureRequest {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u32(self.frame_number).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.fmq_settings_size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.settings.write_to_parcel(parcel)?;
        parcel.write_bool(self.input_buffer.is_some()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if let Some(ref input) = self.input_buffer {
            input.write_to_parcel(parcel)?;
        }
        parcel.write_i32(self.output_buffers.len() as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for ob in &self.output_buffers {
            ob.write_to_parcel(parcel)?;
        }
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.frame_number = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.fmq_settings_size = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.settings.read_from_parcel_at(parcel, offset)?;
        let has_input = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_input {
            let mut input = StreamBuffer::default();
            input.read_from_parcel_at(parcel, offset)?;
            self.input_buffer = Some(input);
        } else {
            self.input_buffer = None;
        }
        let count = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.output_buffers.clear();
        for _ in 0..count {
            let mut ob = StreamBuffer::default();
            ob.read_from_parcel_at(parcel, offset)?;
            self.output_buffers.push(ob);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CaptureResult {
    pub frame_number: u32,
    pub fmq_result_size: u64,
    pub result: CameraMetadata,
    pub output_buffers: Vec<StreamBuffer>,
    pub input_buffer: Option<StreamBuffer>,
    pub partial_result: u32,
}

impl Parcelable for CaptureResult {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u32(self.frame_number).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u64(self.fmq_result_size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.result.write_to_parcel(parcel)?;
        parcel.write_i32(self.output_buffers.len() as i32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for ob in &self.output_buffers {
            ob.write_to_parcel(parcel)?;
        }
        parcel.write_bool(self.input_buffer.is_some()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if let Some(ref input) = self.input_buffer {
            input.write_to_parcel(parcel)?;
        }
        parcel.write_u32(self.partial_result).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.frame_number = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.fmq_result_size = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.result.read_from_parcel_at(parcel, offset)?;
        let count = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.output_buffers.clear();
        for _ in 0..count {
            let mut ob = StreamBuffer::default();
            ob.read_from_parcel_at(parcel, offset)?;
            self.output_buffers.push(ob);
        }
        let has_input = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_input {
            let mut input = StreamBuffer::default();
            input.read_from_parcel_at(parcel, offset)?;
            self.input_buffer = Some(input);
        } else {
            self.input_buffer = None;
        }
        self.partial_result = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Notify Messages
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShutterMsg {
    pub frame_number: u32,
    pub timestamp_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub frame_number: u32,
    pub error_stream_id: i32,
    pub error_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotifyMsg {
    Shutter(ShutterMsg),
    Error(ErrorMsg),
}

impl Parcelable for NotifyMsg {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        match self {
            Self::Shutter(s) => {
                parcel.write_i32(1).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                parcel.write_u32(s.frame_number).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                parcel.write_i64(s.timestamp_ns).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
            }
            Self::Error(e) => {
                parcel.write_i32(2).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                parcel.write_u32(e.frame_number).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                parcel.write_i32(e.error_stream_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                parcel.write_i32(e.error_code).map_err(|_| Status::from_status(STATUS_BAD_VALUE))
            }
        }
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let tag = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        match tag {
            1 => {
                let frame_number = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let timestamp_ns = parcel.read_i64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                *self = Self::Shutter(ShutterMsg { frame_number, timestamp_ns });
                Ok(())
            }
            2 => {
                let frame_number = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let error_stream_id = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let error_code = parcel.read_i32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                *self = Self::Error(ErrorMsg { frame_number, error_stream_id, error_code });
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_BAD_VALUE)),
        }
    }
}

// -----------------------------------------------------------------------------
// Vendor Tags
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorTag {
    pub tag_id: u32,
    pub tag_name: String,
    pub tag_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorTagSection {
    pub section_name: String,
    pub tags: Vec<VendorTag>,
}
