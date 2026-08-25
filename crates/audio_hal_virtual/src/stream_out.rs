//! AIDL `IStreamOut` Interface, Server Stub, and Client Proxy.

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

pub const ISTREAM_OUT_DESCRIPTOR: &str = "android.hardware.audio.core.IStreamOut";

// -----------------------------------------------------------------------------
// Transaction Codes
// -----------------------------------------------------------------------------

pub mod istream_out_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const WRITE: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_BUFFER_SIZE_FRAMES: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_SAMPLE_RATE: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const GET_CHANNEL_COUNT: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const PAUSE: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const RESUME: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const FLUSH: u32 = FIRST_CALL_TRANSACTION + 6; // 7
    pub const DRAIN: u32 = FIRST_CALL_TRANSACTION + 7; // 8
    pub const CLOSE: u32 = FIRST_CALL_TRANSACTION + 8; // 9
}

// -----------------------------------------------------------------------------
// IStreamOut Trait
// -----------------------------------------------------------------------------

pub trait IStreamOut: Interface + Send + Sync {
    /// Write PCM audio buffer containing `frame_count` frames.
    fn write(&self, buffer: &[u8], frame_count: u32) -> AidlResult<u32>;

    fn get_buffer_size_frames(&self) -> AidlResult<u32>;

    fn get_sample_rate(&self) -> AidlResult<u32>;

    fn get_channel_count(&self) -> AidlResult<u32>;

    fn pause(&self) -> AidlResult<()>;

    fn resume(&self) -> AidlResult<()>;

    fn flush(&self) -> AidlResult<()>;

    fn drain(&self) -> AidlResult<()>;

    fn close(&self) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// StreamOut Server Stub Implementation
// -----------------------------------------------------------------------------

pub type StreamOutWriteCallback = Arc<dyn Fn(&[u8], u32) + Send + Sync>;

pub struct StreamOut {
    id: i32,
    sample_rate: u32,
    channel_count: u32,
    format: AudioFormat,
    buffer_size_frames: u32,
    is_paused: AtomicBool,
    is_closed: AtomicBool,
    frames_written: AtomicU64,
    pcm_sink_buffer: Arc<RwLock<VecDeque<u8>>>,
    write_callback: Option<StreamOutWriteCallback>,
}

impl StreamOut {
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
            is_paused: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
            frames_written: AtomicU64::new(0),
            pcm_sink_buffer: Arc::new(RwLock::new(VecDeque::with_capacity(65536))),
            write_callback: None,
        }
    }

    pub fn with_sink_buffer(
        id: i32,
        sample_rate: u32,
        channel_count: u32,
        format: AudioFormat,
        buffer_size_frames: u32,
        pcm_sink_buffer: Arc<RwLock<VecDeque<u8>>>,
    ) -> Self {
        Self {
            id,
            sample_rate,
            channel_count,
            format,
            buffer_size_frames,
            is_paused: AtomicBool::new(false),
            is_closed: AtomicBool::new(false),
            frames_written: AtomicU64::new(0),
            pcm_sink_buffer,
            write_callback: None,
        }
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn frames_written(&self) -> u64 {
        self.frames_written.load(Ordering::Relaxed)
    }

    pub fn pcm_sink_buffer(&self) -> &Arc<RwLock<VecDeque<u8>>> {
        &self.pcm_sink_buffer
    }

    fn clone_internal(&self) -> Self {
        Self {
            id: self.id,
            sample_rate: self.sample_rate,
            channel_count: self.channel_count,
            format: self.format,
            buffer_size_frames: self.buffer_size_frames,
            is_paused: AtomicBool::new(self.is_paused.load(Ordering::SeqCst)),
            is_closed: AtomicBool::new(self.is_closed.load(Ordering::SeqCst)),
            frames_written: AtomicU64::new(self.frames_written.load(Ordering::SeqCst)),
            pcm_sink_buffer: Arc::clone(&self.pcm_sink_buffer),
            write_callback: self.write_callback.clone(),
        }
    }
}

impl Interface for StreamOut {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IStreamOut for StreamOut {
    fn write(&self, buffer: &[u8], frame_count: u32) -> AidlResult<u32> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Status::from(AudioHalError::StreamClosed));
        }

        if self.is_paused.load(Ordering::SeqCst) {
            return Ok(0);
        }

        let bytes_per_frame = (self.channel_count as usize) * self.format.bytes_per_sample();
        let expected_bytes = (frame_count as usize) * bytes_per_frame;
        let actual_bytes = buffer.len().min(expected_bytes);
        let actual_frames = (actual_bytes / bytes_per_frame) as u32;

        if let Some(ref cb) = self.write_callback {
            cb(&buffer[..actual_bytes], actual_frames);
        } else {
            let mut sink = self.pcm_sink_buffer.write().unwrap();
            sink.extend(&buffer[..actual_bytes]);
        }

        self.frames_written
            .fetch_add(actual_frames as u64, Ordering::Relaxed);
        Ok(actual_frames)
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

    fn pause(&self) -> AidlResult<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Status::from(AudioHalError::StreamClosed));
        }
        self.is_paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn resume(&self) -> AidlResult<()> {
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(Status::from(AudioHalError::StreamClosed));
        }
        self.is_paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn flush(&self) -> AidlResult<()> {
        let mut sink = self.pcm_sink_buffer.write().unwrap();
        sink.clear();
        Ok(())
    }

    fn drain(&self) -> AidlResult<()> {
        Ok(())
    }

    fn close(&self) -> AidlResult<()> {
        self.is_closed.store(true, Ordering::SeqCst);
        self.flush()
    }
}

impl Remotable for StreamOut {
    fn get_class_descriptor() -> &'static str {
        ISTREAM_OUT_DESCRIPTOR
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
                reply.write_utf8(Some(ISTREAM_OUT_DESCRIPTOR)).unwrap();
                Ok(())
            }
            istream_out_codes::WRITE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISTREAM_OUT_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let buffer = data
                    .read_byte_vec(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let frame_count = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.write(&buffer, frame_count);
                match res {
                    Ok(written) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply
                            .write_u32(written)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            istream_out_codes::GET_BUFFER_SIZE_FRAMES => {
                let res = self.get_buffer_size_frames()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_out_codes::GET_SAMPLE_RATE => {
                let res = self.get_sample_rate()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_out_codes::GET_CHANNEL_COUNT => {
                let res = self.get_channel_count()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            istream_out_codes::PAUSE => {
                let res = self.pause();
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
            istream_out_codes::RESUME => {
                let res = self.resume();
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
            istream_out_codes::FLUSH => {
                let res = self.flush();
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
            istream_out_codes::DRAIN => {
                let res = self.drain();
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
            istream_out_codes::CLOSE => {
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

impl IBinder for StreamOut {
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
                    .write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
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
        Some(ISTREAM_OUT_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// StreamOutProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct StreamOutProxy {
    binder: SpIBinder,
}

impl StreamOutProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for StreamOutProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for StreamOutProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IStreamOut for StreamOutProxy {
    fn write(&self, buffer: &[u8], frame_count: u32) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_byte_slice(Some(buffer))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(frame_count)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::WRITE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let written = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(written)
    }

    fn get_buffer_size_frames(&self) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::GET_BUFFER_SIZE_FRAMES, 0, &data, &mut reply)?;

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
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::GET_SAMPLE_RATE, 0, &data, &mut reply)?;

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
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::GET_CHANNEL_COUNT, 0, &data, &mut reply)?;

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

    fn pause(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::PAUSE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn resume(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::RESUME, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn flush(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::FLUSH, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn drain(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::DRAIN, 0, &data, &mut reply)?;

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
        data.write_utf16(Some(ISTREAM_OUT_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(istream_out_codes::CLOSE, 0, &data, &mut reply)?;

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
