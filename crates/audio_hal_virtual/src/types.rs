//! Core Audio AIDL Data Types, Arguments, and Parcelables.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::Parcelable;
use binder_rt::Parcel;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Audio Format Enum
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum AudioFormat {
    Pcm16Bit = 1,
    Pcm8Bit = 2,
    Pcm32Bit = 3,
    PcmFloat = 4,
    Unknown = 0,
}

impl From<i32> for AudioFormat {
    fn from(val: i32) -> Self {
        match val {
            1 => AudioFormat::Pcm16Bit,
            2 => AudioFormat::Pcm8Bit,
            3 => AudioFormat::Pcm32Bit,
            4 => AudioFormat::PcmFloat,
            _ => AudioFormat::Unknown,
        }
    }
}

impl From<AudioFormat> for i32 {
    fn from(f: AudioFormat) -> Self {
        f as i32
    }
}

impl AudioFormat {
    /// Bytes per sample for given format.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            AudioFormat::Pcm8Bit => 1,
            AudioFormat::Pcm16Bit => 2,
            AudioFormat::Pcm32Bit | AudioFormat::PcmFloat => 4,
            AudioFormat::Unknown => 2,
        }
    }
}

// -----------------------------------------------------------------------------
// Audio Channel Mask
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum AudioChannelMask {
    Mono = 1,
    Stereo = 2,
    Surround51 = 6,
    Surround71 = 8,
}

impl AudioChannelMask {
    pub fn channel_count(&self) -> u32 {
        *self as u32
    }
}

// -----------------------------------------------------------------------------
// Audio Port (android.hardware.audio.core.AudioPort)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPort {
    pub id: i32,
    pub name: String,
    pub is_input: bool,
    pub supported_sample_rates: Vec<u32>,
    pub supported_channel_masks: Vec<u32>,
    pub supported_formats: Vec<AudioFormat>,
}

impl Default for AudioPort {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Virtual Speaker Out".to_string(),
            is_input: false,
            supported_sample_rates: vec![44100, 48000, 96000],
            supported_channel_masks: vec![1, 2], // Mono, Stereo
            supported_formats: vec![AudioFormat::Pcm16Bit, AudioFormat::PcmFloat],
        }
    }
}

impl AudioPort {
    pub fn new_output_speaker(id: i32) -> Self {
        Self {
            id,
            name: "Virtual Speaker Out".to_string(),
            is_input: false,
            supported_sample_rates: vec![44100, 48000, 96000],
            supported_channel_masks: vec![1, 2],
            supported_formats: vec![AudioFormat::Pcm16Bit, AudioFormat::PcmFloat],
        }
    }

    pub fn new_input_microphone(id: i32) -> Self {
        Self {
            id,
            name: "Virtual Microphone In".to_string(),
            is_input: true,
            supported_sample_rates: vec![44100, 48000],
            supported_channel_masks: vec![1, 2],
            supported_formats: vec![AudioFormat::Pcm16Bit, AudioFormat::PcmFloat],
        }
    }
}

// -----------------------------------------------------------------------------
// Audio Route
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioRoute {
    pub route_id: i32,
    pub source_port_ids: Vec<i32>,
    pub sink_port_id: i32,
    pub is_dynamic: bool,
}

// -----------------------------------------------------------------------------
// OpenOutputStreamArguments and Result
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenOutputStreamArguments {
    pub port_config_id: i32,
    pub buffer_size_frames: u32,
    pub sample_rate: u32,
    pub channel_mask: u32,
    pub format: AudioFormat,
}

impl Default for OpenOutputStreamArguments {
    fn default() -> Self {
        Self {
            port_config_id: 1,
            buffer_size_frames: 480, // 10ms at 48kHz
            sample_rate: 48000,
            channel_mask: 2, // Stereo
            format: AudioFormat::Pcm16Bit,
        }
    }
}

impl Parcelable for OpenOutputStreamArguments {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_i32(self.port_config_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.buffer_size_frames)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.sample_rate)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.channel_mask)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.format.into())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> AidlResult<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.port_config_id = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.buffer_size_frames = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.sample_rate = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.channel_mask = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt_raw = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.format = AudioFormat::from(fmt_raw);
        Ok(())
    }
}

#[derive(Clone)]
pub struct OpenOutputStreamResult {
    pub stream: Option<SpIBinder>,
    pub stream_id: i32,
    pub buffer_size_frames: u32,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub format: AudioFormat,
}

impl Default for OpenOutputStreamResult {
    fn default() -> Self {
        Self {
            stream: None,
            stream_id: 1,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_count: 2,
            format: AudioFormat::Pcm16Bit,
        }
    }
}

// -----------------------------------------------------------------------------
// OpenInputStreamArguments and Result
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenInputStreamArguments {
    pub port_config_id: i32,
    pub buffer_size_frames: u32,
    pub sample_rate: u32,
    pub channel_mask: u32,
    pub format: AudioFormat,
}

impl Default for OpenInputStreamArguments {
    fn default() -> Self {
        Self {
            port_config_id: 2,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_mask: 1, // Mono
            format: AudioFormat::Pcm16Bit,
        }
    }
}

impl Parcelable for OpenInputStreamArguments {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_i32(self.port_config_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.buffer_size_frames)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.sample_rate)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.channel_mask)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.format.into())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> AidlResult<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.port_config_id = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.buffer_size_frames = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.sample_rate = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.channel_mask = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt_raw = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.format = AudioFormat::from(fmt_raw);
        Ok(())
    }
}

#[derive(Clone)]
pub struct OpenInputStreamResult {
    pub stream: Option<SpIBinder>,
    pub stream_id: i32,
    pub buffer_size_frames: u32,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub format: AudioFormat,
}

impl Default for OpenInputStreamResult {
    fn default() -> Self {
        Self {
            stream: None,
            stream_id: 2,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_count: 1,
            format: AudioFormat::Pcm16Bit,
        }
    }
}
