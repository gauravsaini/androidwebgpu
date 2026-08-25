//! AIDL `IStreamIn` Interface, Server Stub, and Client Proxy.

use crate::error::AudioHalError;
use crate::types::AudioFormat;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

pub const ISTREAM_IN_DESCRIPTOR: &str = "android.hardware.audio.core.IStreamIn";

// -----------------------------------------------------------------------------
// Transaction Codes
// -----------------------------------------------------------------------------

pub mod istream_in_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const READ: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_BUFFER_SIZE_FRAMES: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_SAMPLE_RATE: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const GET_CHANNEL_COUNT: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const STANDBY: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const CLOSE: u32 = FIRST_CALL_TRANSACTION + 5; // 6
}

// -----------------------------------------------------------------------------
// IStreamIn Trait
// -----------------------------------------------------------------------------

pub trait IStreamIn: Interface + Send + Sync {
    /// Read PCM audio buffer containing up to `frame_count` frames.
    fn read(&self, buffer: &mut [u8], frame_count: u32) -> AidlResult<u32>;

    fn get_buffer_size_frames(&self) -> AidlResult<u32>;

    fn get_sample_rate(&self) -> AidlResult<u32>;

    fn get_channel_count(&self) -> AidlResult<u32>;

    fn standby(&self) -> AidlResult<()>;

    fn close(&self) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// StreamIn Server Stub Implementation
// -----------------------------------------------------------------------------

pub struct StreamIn {
    id: i32,
    sample_rate: u32,
    channel_count: u32,
    format: AudioFormat,
    buffer_size_frames: u32,
    is_standby: AtomicBool,
    is_closed: AtomicBool,
    frames_read: AtomicU64,
    pcm_source_buffer: Arc<RwLock<VecDeque<u8>>>,
}

impl StreamIn {
    pub fn new(
        id: i32,
        sample_rate: u32,
        channel_count: u32,
        format: AudioFormat,
        buffer_size_frames: u32,
    ) -> Self {
        Self {
            id,
            sample_rate,
            channel_count,
            format,
            buffer_size_frames,
            is_standby: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
            frames_read: AtomicU64::new(0),
            pcm_source_buffer: Arc::new(RwLock::new(VecDeque::with_capacity(65536))),
        }
    }

    pub fn with_source_buffer(
        id: i32,
        sample_rate: u32,
        channel_count: u32,
        format: AudioFormat,
        buffer_size_frames: u32,
        pcm_source_buffer: Arc<RwLock<VecDeque<u8>>>,
    ) -> Self {
        Self {
            id,
            sample_rate,
            channel_count,
            format,
            buffer_size_frames,
            is_standby: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
            frames_read: AtomicU64::new(0),
            pcm_source_buffer,
        }
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn frames_read(&self) -> u64 {
        self.frames_read.load(Ordering::Relaxed)
    }

    pub fn pcm_source_buffer(&self) -> &Arc<RwLock<VecDeque<u8>>> {
        &self.pcm_source_buffer
    }

    /// Feed incoming mic samples into the source buffer.
    pub fn feed_pcm(&self, samples: &[u8]) {
        let mut src = self.pcm_source_buffer.write().unwrap();
        src.extend(samples);
    }

    fn clone_internal(&self) -> Self {
        Self {
            id: self.id,
            sample_rate: self.sample_rate,
            channel_count: self.channel_count,
            format: self.format,
            buffer_size_frames: self.buffer_size_frames,
            is_standby: AtomicBool::new(self.is_standby.load(Ordering::SeqCst)),
            is_closed: AtomicBool::new(self.is_closed.load(Ordering::SeqCst)),
            frames_read: AtomicU64::new(self.frames_read.load(Ordering::SeqCst)),
            pcm_source_buffer: Arc::clone(&self.pcm_source_buffer),
        }
    }
}

impl Interface for StreamIn {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IStreamIn for StreamIn {
    fn read(&self, buffer: &mut [u8], frame_count: u32) -> AidlResult<u32> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Status::from(AudioHalError::StreamClosed));
        }

        let bytes_per_frame = (self.channel_count as usize) * self.format.bytes_per_sample();
        let target_bytes = (frame_count as usize) * bytes_per_frame;
        let read_bytes_bound = buffer.len().min(target_bytes);

        let mut src = self.pcm_source_buffer.write().unwrap();
        let available = src.len();
        let to_read = available.min(read_bytes_bound);

        for slot in buffer[..to_read].iter_mut() {
            *slot = src.pop_front().unwrap_or(0);
        }

        // Fill remaining with silence if underrun
        for slot in buffer[to_read..read_bytes_bound].iter_mut() {
            *slot = 0;
        }

        let frames_read = (read_bytes_bound / bytes_per_frame) as u32;
        self.frames_read
            .fetch_add(frames_read as u64, Ordering::Relaxed);
        Ok(frames_read)
    }

    fn get_buffer_size_frames(&self) -> AidlResult<u32> {
        Ok(self.buffer_size_frames)
    }

    fn get_sample_rate(&self) -> AidlResult<u32> {
        Ok(self.sample_rate)
    }

    fn get_channel_count(&self) -> AidlResult<u32> {
        Ok(self.channel_count)
    }

    fn standby(&self) -> AidlResult<()> {
        self.is_standby.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn close(&self) -> AidlResult<()> {
        self.is_closed.store(true, Ordering::SeqCst);
        let mut src = self.pcm_source_buffer.write().unwrap();
        src.clear();
        Ok(())
    }
}

impl Remotable for StreamIn {
    fn get_class_descriptor() -> &'static str {
        ISTREAM_IN_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf8(Some(ISTREAM_IN_DESCRIPTOR)).unwrap();
                Ok(())
            }
            istream_in_codes::READ => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISTREAM_IN_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let frame_count = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let bytes_per_frame =
                    (self.channel_count as usize) * self.format.bytes_per_sample();
                let mut buffer = vec![0u8; (frame_count as usize) * bytes_per_frame];

                let res = self.read(&mut buffer, frame_count);
                match res {
                    Ok(frames) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply
                            .write_u32(frames)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_byte_slice(Some(&buffer))
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            istream_in_codes::GET_BUFFER_SIZE_FRAMES => {
                let res = self.get_buffer_size_frames()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_in_codes::GET_SAMPLE_RATE => {
                let res = self.get_sample_rate()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_in_codes::GET_CHANNEL_COUNT => {
                let res = self.get_channel_count()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_in_codes::STANDBY => {
                let res = self.standby();
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            istream_in_codes::CLOSE => {
                let res = self.close();
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            _ => {
                reply
                    .write_status(&Status::from_status(STATUS_UNKNOWN_TRANSACTION))
                    .unwrap();
                Ok(())
            }
        }
    }
}

impl IBinder for StreamIn {
    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn ping_binder(&self) -> AidlResult<()> {
        Ok(())
    }

    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply
                    .write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(ISTREAM_IN_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// StreamInProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct StreamInProxy {
    binder: SpIBinder,
}

impl StreamInProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for StreamInProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for StreamInProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IStreamIn for StreamInProxy {
    fn read(&self, buffer: &mut [u8], frame_count: u32) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(frame_count)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::READ, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let frames = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let read_bytes = reply
            .read_byte_vec(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();

        let copy_len = buffer.len().min(read_bytes.len());
        buffer[..copy_len].copy_from_slice(&read_bytes[..copy_len]);
        Ok(frames)
    }

    fn get_buffer_size_frames(&self) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::GET_BUFFER_SIZE_FRAMES, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let size = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(size)
    }

    fn get_sample_rate(&self) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::GET_SAMPLE_RATE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let rate = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(rate)
    }

    fn get_channel_count(&self) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::GET_CHANNEL_COUNT, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let count = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(count)
    }

    fn standby(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::STANDBY, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn close(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_IN_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_in_codes::CLOSE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }
}
