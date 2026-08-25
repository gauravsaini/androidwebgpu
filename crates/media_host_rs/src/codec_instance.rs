//! AIDL `IMediaCodec` Interface, Remotable Server Stub, and Client Proxy.

use crate::error::MediaCodecError;
use crate::types::*;
use crate::webcodecs_bridge::{DecodedVideoFrame, WebCodecsVideoDecoderBridge};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub const IMEDIA_CODEC_DESCRIPTOR: &str = "android.media.IMediaCodec";

pub mod imedia_codec_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const CONFIGURE: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const START: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const STOP: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const FLUSH: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const RESET: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const RELEASE: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const QUEUE_INPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 6; // 7
    pub const DEQUEUE_INPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 7; // 8
    pub const DEQUEUE_OUTPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 8; // 9
    pub const RELEASE_OUTPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 9; // 10
    pub const GET_OUTPUT_FORMAT: u32 = FIRST_CALL_TRANSACTION + 10; // 11
    pub const SET_INPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 11; // 12
    pub const GET_OUTPUT_BUFFER: u32 = FIRST_CALL_TRANSACTION + 12; // 13
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecState {
    Uninitialized,
    Configured,
    Executing,
    Flushed,
    Released,
}

/// AIDL Interface for an active MediaCodec session.
pub trait IMediaCodec: Interface + Send + Sync {
    /// Configure codec with format, optional target surface, and flags.
    fn configure(
        &self,
        format: &MediaFormat,
        surface: Option<SpIBinder>,
        flags: u32,
    ) -> AidlResult<()>;

    /// Start processing.
    fn start(&self) -> AidlResult<()>;

    /// Stop processing.
    fn stop(&self) -> AidlResult<()>;

    /// Flush in-flight buffers.
    fn flush(&self) -> AidlResult<()>;

    /// Reset codec to uninitialized state.
    fn reset(&self) -> AidlResult<()>;

    /// Release codec resources.
    fn release(&self) -> AidlResult<()>;

    /// Submit input buffer for processing.
    fn queue_input_buffer(
        &self,
        index: u32,
        offset: u32,
        size: u32,
        pts_us: i64,
        flags: u32,
    ) -> AidlResult<()>;

    /// Dequeue next available input buffer index.
    fn dequeue_input_buffer(&self, timeout_us: i64) -> AidlResult<i32>;

    /// Dequeue next decoded output buffer and its info.
    fn dequeue_output_buffer(&self, info: &mut BufferInfo, timeout_us: i64) -> AidlResult<i32>;

    /// Return output buffer to codec or render to surface.
    fn release_output_buffer(
        &self,
        index: u32,
        render: bool,
        render_time_ns: i64,
    ) -> AidlResult<()>;

    /// Query output format.
    fn get_output_format(&self) -> AidlResult<MediaFormat>;

    /// Set input buffer data for slot index.
    fn set_input_buffer(&self, index: u32, data: &[u8]) -> AidlResult<()>;

    /// Get decoded output buffer data for slot index.
    fn get_output_buffer(&self, index: u32) -> AidlResult<Vec<u8>>;
}

// -----------------------------------------------------------------------------
// MediaCodecServiceInstance Implementation
// -----------------------------------------------------------------------------

pub struct MediaCodecServiceInstance {
    name: String,
    mime_type: String,
    is_encoder: bool,
    state: RwLock<CodecState>,
    format: RwLock<Option<MediaFormat>>,
    bridge: RwLock<Option<Arc<WebCodecsVideoDecoderBridge>>>,
    input_buffers: Mutex<HashMap<u32, Vec<u8>>>,
    input_available: Mutex<Vec<u32>>,
    output_buffers: Mutex<HashMap<u32, DecodedVideoFrame>>,
    output_available: Mutex<Vec<u32>>,
    next_output_slot: AtomicU32,
}

impl MediaCodecServiceInstance {
    pub const BUFFER_SLOTS: u32 = 8;

    pub fn new(name: &str, mime_type: &str, is_encoder: bool) -> Self {
        let mut input_buffers = HashMap::new();
        let mut input_avail = Vec::new();
        for i in 0..Self::BUFFER_SLOTS {
            input_buffers.insert(i, Vec::new());
            input_avail.push(i);
        }

        Self {
            name: name.to_string(),
            mime_type: mime_type.to_string(),
            is_encoder,
            state: RwLock::new(CodecState::Uninitialized),
            format: RwLock::new(None),
            bridge: RwLock::new(None),
            input_buffers: Mutex::new(input_buffers),
            input_available: Mutex::new(input_avail),
            output_buffers: Mutex::new(HashMap::new()),
            output_available: Mutex::new(Vec::new()),
            next_output_slot: AtomicU32::new(0),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn is_encoder(&self) -> bool {
        self.is_encoder
    }
}

impl Interface for MediaCodecServiceInstance {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MediaCodecBinder {
            inner: Arc::new(Self {
                name: self.name.clone(),
                mime_type: self.mime_type.clone(),
                is_encoder: self.is_encoder,
                state: RwLock::new(*self.state.read().unwrap()),
                format: RwLock::new(self.format.read().unwrap().clone()),
                bridge: RwLock::new(self.bridge.read().unwrap().clone()),
                input_buffers: Mutex::new(self.input_buffers.lock().unwrap().clone()),
                input_available: Mutex::new(self.input_available.lock().unwrap().clone()),
                output_buffers: Mutex::new(self.output_buffers.lock().unwrap().clone()),
                output_available: Mutex::new(self.output_available.lock().unwrap().clone()),
                next_output_slot: AtomicU32::new(self.next_output_slot.load(Ordering::Relaxed)),
            }),
        })
    }
}

impl IMediaCodec for MediaCodecServiceInstance {
    fn configure(
        &self,
        format: &MediaFormat,
        surface: Option<SpIBinder>,
        _flags: u32,
    ) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        if *state_guard != CodecState::Uninitialized {
            return Err(MediaCodecError::InvalidState("Must be uninitialized to configure".to_string()).into());
        }

        let bridge = Arc::new(WebCodecsVideoDecoderBridge::new(
            &format.mime,
            format.width.max(1),
            format.height.max(1),
        ));
        bridge.set_surface(surface);

        *self.bridge.write().unwrap() = Some(bridge);
        *self.format.write().unwrap() = Some(format.clone());
        *state_guard = CodecState::Configured;

        Ok(())
    }

    fn start(&self) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        match *state_guard {
            CodecState::Configured | CodecState::Flushed => {
                *state_guard = CodecState::Executing;
                Ok(())
            }
            _ => Err(MediaCodecError::InvalidState("Cannot start in current state".to_string()).into()),
        }
    }

    fn stop(&self) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CodecState::Uninitialized;
        Ok(())
    }

    fn flush(&self) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        if *state_guard != CodecState::Executing {
            return Err(MediaCodecError::InvalidState("Must be executing to flush".to_string()).into());
        }

        if let Some(ref bridge) = *self.bridge.read().unwrap() {
            bridge.flush();
        }

        // Reset input available slots
        let mut in_avail = self.input_available.lock().unwrap();
        in_avail.clear();
        for i in 0..Self::BUFFER_SLOTS {
            in_avail.push(i);
        }

        self.output_buffers.lock().unwrap().clear();
        self.output_available.lock().unwrap().clear();

        *state_guard = CodecState::Flushed;
        Ok(())
    }

    fn reset(&self) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CodecState::Uninitialized;
        *self.format.write().unwrap() = None;
        *self.bridge.write().unwrap() = None;
        Ok(())
    }

    fn release(&self) -> AidlResult<()> {
        let mut state_guard = self.state.write().unwrap();
        *state_guard = CodecState::Released;
        Ok(())
    }

    fn queue_input_buffer(
        &self,
        index: u32,
        offset: u32,
        size: u32,
        pts_us: i64,
        flags: u32,
    ) -> AidlResult<()> {
        let state = *self.state.read().unwrap();
        if state != CodecState::Executing {
            return Err(MediaCodecError::InvalidState("Must be executing to queue input".to_string()).into());
        }

        let input_data = {
            let guard = self.input_buffers.lock().unwrap();
            let buf = guard.get(&index).ok_or_else(|| MediaCodecError::InvalidBufferIndex(index))?;
            let start = (offset as usize).min(buf.len());
            let end = (start + size as usize).min(buf.len());
            buf[start..end].to_vec()
        };

        if let Some(ref bridge) = *self.bridge.read().unwrap() {
            if let Ok(Some(decoded_frame)) = bridge.decode_packet(&input_data, pts_us, flags) {
                let slot = self.next_output_slot.fetch_add(1, Ordering::Relaxed) % Self::BUFFER_SLOTS;
                self.output_buffers.lock().unwrap().insert(slot, decoded_frame);
                self.output_available.lock().unwrap().push(slot);
            }
        }

        // Return input slot to available pool
        self.input_available.lock().unwrap().push(index);
        Ok(())
    }

    fn dequeue_input_buffer(&self, _timeout_us: i64) -> AidlResult<i32> {
        let state = *self.state.read().unwrap();
        if state != CodecState::Executing {
            return Err(MediaCodecError::InvalidState("Must be executing to dequeue input".to_string()).into());
        }

        let mut avail = self.input_available.lock().unwrap();
        if let Some(slot) = avail.pop() {
            Ok(slot as i32)
        } else {
            Ok(INFO_TRY_AGAIN_LATER)
        }
    }

    fn dequeue_output_buffer(&self, info: &mut BufferInfo, _timeout_us: i64) -> AidlResult<i32> {
        let state = *self.state.read().unwrap();
        if state != CodecState::Executing {
            return Err(MediaCodecError::InvalidState("Must be executing to dequeue output".to_string()).into());
        }

        let mut avail = self.output_available.lock().unwrap();
        if let Some(slot) = avail.pop() {
            let guard = self.output_buffers.lock().unwrap();
            if let Some(frame) = guard.get(&slot) {
                info.offset = 0;
                info.size = frame.data.len() as u32;
                info.presentation_time_us = frame.pts_us;
                info.flags = if frame.is_keyframe {
                    BUFFER_FLAG_KEY_FRAME
                } else if frame.is_eos {
                    BUFFER_FLAG_END_OF_STREAM
                } else {
                    0
                };
                Ok(slot as i32)
            } else {
                Ok(INFO_TRY_AGAIN_LATER)
            }
        } else {
            Ok(INFO_TRY_AGAIN_LATER)
        }
    }

    fn release_output_buffer(
        &self,
        index: u32,
        render: bool,
        render_time_ns: i64,
    ) -> AidlResult<()> {
        let frame = {
            let mut guard = self.output_buffers.lock().unwrap();
            guard.remove(&index)
        };

        if let Some(f) = frame {
            if render {
                if let Some(ref bridge) = *self.bridge.read().unwrap() {
                    bridge
                        .render_frame_to_surface(&f, render_time_ns)
                        .map_err(Status::from)?;
                }
            }
        }
        Ok(())
    }

    fn get_output_format(&self) -> AidlResult<MediaFormat> {
        let fmt = self.format.read().unwrap();
        fmt.clone().ok_or_else(|| MediaCodecError::InvalidState("No format configured".to_string()).into())
    }

    fn set_input_buffer(&self, index: u32, data: &[u8]) -> AidlResult<()> {
        let mut guard = self.input_buffers.lock().unwrap();
        if index >= Self::BUFFER_SLOTS {
            return Err(MediaCodecError::InvalidBufferIndex(index).into());
        }
        guard.insert(index, data.to_vec());
        Ok(())
    }

    fn get_output_buffer(&self, index: u32) -> AidlResult<Vec<u8>> {
        let guard = self.output_buffers.lock().unwrap();
        if let Some(frame) = guard.get(&index) {
            Ok(frame.data.clone())
        } else {
            Err(MediaCodecError::InvalidBufferIndex(index).into())
        }
    }
}

// -----------------------------------------------------------------------------
// Remotable Binder Stub for MediaCodec
// -----------------------------------------------------------------------------

pub struct MediaCodecBinder {
    pub inner: Arc<MediaCodecServiceInstance>,
}

impl Interface for MediaCodecBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MediaCodecBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for MediaCodecBinder {
    fn get_class_descriptor() -> &'static str {
        IMEDIA_CODEC_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            imedia_codec_codes::CONFIGURE => {
                let mut format = MediaFormat::default();
                format.read_from_parcel_at(data, &mut offset)?;

                let surface_binder = if let Ok(flat) = data.read_binder(&mut offset) {
                    let transport = Arc::new(binder_sys::BinderKernelTransport::new());
                    Some(aidl_compat::RemoteBinder::new_with_transport(
                        flat.handle(),
                        flat.cookie,
                        None,
                        transport,
                    ))
                } else {
                    None
                };

                let flags = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                self.inner.configure(&format, surface_binder, flags)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::START => {
                self.inner.start()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::STOP => {
                self.inner.stop()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::FLUSH => {
                self.inner.flush()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::RESET => {
                self.inner.reset()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::RELEASE => {
                self.inner.release()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::QUEUE_INPUT_BUFFER => {
                let index = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let buf_offset = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let size = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let pts_us = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let flags = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                self.inner
                    .queue_input_buffer(index, buf_offset, size, pts_us, flags)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::DEQUEUE_INPUT_BUFFER => {
                let timeout_us = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let res = self.inner.dequeue_input_buffer(timeout_us)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            imedia_codec_codes::DEQUEUE_OUTPUT_BUFFER => {
                let timeout_us = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut info = BufferInfo::default();
                let res = self.inner.dequeue_output_buffer(&mut info, timeout_us)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                info.write_to_parcel(reply)?;
                Ok(())
            }
            imedia_codec_codes::RELEASE_OUTPUT_BUFFER => {
                let index = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let render = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let render_time_ns = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                self.inner
                    .release_output_buffer(index, render, render_time_ns)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::GET_OUTPUT_FORMAT => {
                let fmt = self.inner.get_output_format()?;
                reply.write_status(&Status::ok()).unwrap();
                fmt.write_to_parcel(reply)?;
                Ok(())
            }
            imedia_codec_codes::SET_INPUT_BUFFER => {
                let index = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let buf = data
                    .read_byte_vec(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                self.inner.set_input_buffer(index, &buf)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imedia_codec_codes::GET_OUTPUT_BUFFER => {
                let index = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let buf = self.inner.get_output_buffer(index)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_byte_slice(Some(&buf))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for MediaCodecBinder {
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
                    .write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
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
        Some(IMEDIA_CODEC_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct MediaCodecProxy {
    binder: SpIBinder,
}

impl MediaCodecProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for MediaCodecProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for MediaCodecProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IMediaCodec for MediaCodecProxy {
    fn configure(
        &self,
        format: &MediaFormat,
        surface: Option<SpIBinder>,
        flags: u32,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        format.write_to_parcel(&mut data)?;

        if let Some(surf) = surface {
            let handle = surf.handle().unwrap_or(0);
            data.write_binder(handle, 0)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_i32(0)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        data.write_u32(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::CONFIGURE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn start(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::START, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn stop(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::STOP, 0, &data, &mut reply)?;

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
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::FLUSH, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn reset(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::RESET, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn release(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imedia_codec_codes::RELEASE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn queue_input_buffer(
        &self,
        index: u32,
        offset: u32,
        size: u32,
        pts_us: i64,
        flags: u32,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(index).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(size).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(pts_us).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(flags).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::QUEUE_INPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut off = 0;
        let status = reply
            .read_status(&mut off)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn dequeue_input_buffer(&self, timeout_us: i64) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(timeout_us)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::DEQUEUE_INPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn dequeue_output_buffer(&self, info: &mut BufferInfo, timeout_us: i64) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(timeout_us)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::DEQUEUE_OUTPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let res = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if res >= 0 {
            info.read_from_parcel_at(&reply, &mut offset)?;
        }
        Ok(res)
    }

    fn release_output_buffer(
        &self,
        index: u32,
        render: bool,
        render_time_ns: i64,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(index).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(render).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(render_time_ns).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::RELEASE_OUTPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn get_output_format(&self) -> AidlResult<MediaFormat> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::GET_OUTPUT_FORMAT,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let mut fmt = MediaFormat::default();
        fmt.read_from_parcel_at(&reply, &mut offset)?;
        Ok(fmt)
    }

    fn set_input_buffer(&self, index: u32, data_bytes: &[u8]) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(index).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_byte_slice(Some(data_bytes)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::SET_INPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn get_output_buffer(&self, index: u32) -> AidlResult<Vec<u8>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_u32(index).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_codes::GET_OUTPUT_BUFFER,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        reply
            .read_byte_vec(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
            .map(|opt| opt.unwrap_or_default())
    }
}
