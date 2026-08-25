//! AIDL `IModule` Interface, Server Stub, Client Proxy, and Service Registration.

use crate::stream_in::StreamIn;
use crate::stream_out::StreamOut;
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, RwLock};

pub const IMODULE_DESCRIPTOR: &str = "android.hardware.audio.core.IModule";
pub const IMODULE_DEFAULT_INSTANCE: &str = "android.hardware.audio.core.IModule/default";

// -----------------------------------------------------------------------------
// Transaction Codes
// -----------------------------------------------------------------------------

pub mod imodule_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const OPEN_OUTPUT_STREAM: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const OPEN_INPUT_STREAM: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_MASTER_MUTE: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const SET_MASTER_MUTE: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const GET_MASTER_VOLUME: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const SET_MASTER_VOLUME: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const GET_AUDIO_PORTS: u32 = FIRST_CALL_TRANSACTION + 6; // 7
    pub const GET_AUDIO_ROUTES: u32 = FIRST_CALL_TRANSACTION + 7; // 8
}

// -----------------------------------------------------------------------------
// IModule Trait
// -----------------------------------------------------------------------------

pub trait IModule: Interface + Send + Sync {
    fn open_output_stream(
        &self,
        args: &OpenOutputStreamArguments,
    ) -> AidlResult<OpenOutputStreamResult>;

    fn open_input_stream(
        &self,
        args: &OpenInputStreamArguments,
    ) -> AidlResult<OpenInputStreamResult>;

    fn get_master_mute(&self) -> AidlResult<bool>;

    fn set_master_mute(&self, mute: bool) -> AidlResult<()>;

    fn get_master_volume(&self) -> AidlResult<f32>;

    fn set_master_volume(&self, volume: f32) -> AidlResult<()>;

    fn get_audio_ports(&self) -> AidlResult<Vec<AudioPort>>;

    fn get_audio_routes(&self) -> AidlResult<Vec<AudioRoute>>;
}

// -----------------------------------------------------------------------------
// Active Streams Registry (for local in-process client proxies)
// -----------------------------------------------------------------------------

static ACTIVE_OUTPUT_STREAMS: RwLock<Option<HashMap<i32, Arc<StreamOut>>>> = RwLock::new(None);
static ACTIVE_INPUT_STREAMS: RwLock<Option<HashMap<i32, Arc<StreamIn>>>> = RwLock::new(None);

pub fn register_active_output_stream(stream: Arc<StreamOut>) {
    let mut guard = ACTIVE_OUTPUT_STREAMS.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard.as_mut().unwrap().insert(stream.id(), stream);
}

pub fn get_active_output_stream(id: i32) -> Option<Arc<StreamOut>> {
    let guard = ACTIVE_OUTPUT_STREAMS.read().unwrap();
    guard.as_ref().and_then(|map| map.get(&id).cloned())
}

pub fn register_active_input_stream(stream: Arc<StreamIn>) {
    let mut guard = ACTIVE_INPUT_STREAMS.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard.as_mut().unwrap().insert(stream.id(), stream);
}

pub fn get_active_input_stream(id: i32) -> Option<Arc<StreamIn>> {
    let guard = ACTIVE_INPUT_STREAMS.read().unwrap();
    guard.as_ref().and_then(|map| map.get(&id).cloned())
}

// -----------------------------------------------------------------------------
// AudioModuleService Server Stub Implementation
// -----------------------------------------------------------------------------

pub struct AudioModuleService {
    output_streams: Arc<RwLock<HashMap<i32, Arc<StreamOut>>>>,
    input_streams: Arc<RwLock<HashMap<i32, Arc<StreamIn>>>>,
    next_stream_id: AtomicI32,
    master_mute: AtomicBool,
    master_volume: Arc<RwLock<f32>>,
    ports: Arc<RwLock<Vec<AudioPort>>>,
    routes: Arc<RwLock<Vec<AudioRoute>>>,
}

impl Default for AudioModuleService {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioModuleService {
    pub fn new() -> Self {
        let ports = vec![
            AudioPort::new_output_speaker(1),
            AudioPort::new_input_microphone(2),
        ];
        let routes = vec![
            AudioRoute {
                route_id: 1,
                source_port_ids: vec![1],
                sink_port_id: 1,
                is_dynamic: false,
            },
            AudioRoute {
                route_id: 2,
                source_port_ids: vec![2],
                sink_port_id: 2,
                is_dynamic: false,
            },
        ];

        Self {
            output_streams: Arc::new(RwLock::new(HashMap::new())),
            input_streams: Arc::new(RwLock::new(HashMap::new())),
            next_stream_id: AtomicI32::new(1),
            master_mute: AtomicBool::new(false),
            master_volume: Arc::new(RwLock::new(1.0)),
            ports: Arc::new(RwLock::new(ports)),
            routes: Arc::new(RwLock::new(routes)),
        }
    }

    pub fn get_output_stream(&self, stream_id: i32) -> Option<Arc<StreamOut>> {
        self.output_streams.read().unwrap().get(&stream_id).cloned()
    }

    pub fn get_input_stream(&self, stream_id: i32) -> Option<Arc<StreamIn>> {
        self.input_streams.read().unwrap().get(&stream_id).cloned()
    }

    fn clone_internal(&self) -> Self {
        Self {
            output_streams: Arc::clone(&self.output_streams),
            input_streams: Arc::clone(&self.input_streams),
            next_stream_id: AtomicI32::new(self.next_stream_id.load(Ordering::SeqCst)),
            master_mute: AtomicBool::new(self.master_mute.load(Ordering::SeqCst)),
            master_volume: Arc::clone(&self.master_volume),
            ports: Arc::clone(&self.ports),
            routes: Arc::clone(&self.routes),
        }
    }
}

impl Interface for AudioModuleService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IModule for AudioModuleService {
    fn open_output_stream(
        &self,
        args: &OpenOutputStreamArguments,
    ) -> AidlResult<OpenOutputStreamResult> {
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = args.channel_mask.max(1);

        let stream = Arc::new(StreamOut::new(
            stream_id,
            args.sample_rate,
            channel_count,
            args.format,
            args.buffer_size_frames,
        ));

        register_active_output_stream(Arc::clone(&stream));

        self.output_streams
            .write()
            .unwrap()
            .insert(stream_id, Arc::clone(&stream));

        let binder: Arc<dyn IBinder> = stream;
        Ok(OpenOutputStreamResult {
            stream: Some(SpIBinder::from_arc(binder)),
            stream_id,
            buffer_size_frames: args.buffer_size_frames,
            sample_rate: args.sample_rate,
            channel_count,
            format: args.format,
        })
    }

    fn open_input_stream(
        &self,
        args: &OpenInputStreamArguments,
    ) -> AidlResult<OpenInputStreamResult> {
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        let channel_count = args.channel_mask.max(1);

        let stream = Arc::new(StreamIn::new(
            stream_id,
            args.sample_rate,
            channel_count,
            args.format,
            args.buffer_size_frames,
        ));

        register_active_input_stream(Arc::clone(&stream));

        self.input_streams
            .write()
            .unwrap()
            .insert(stream_id, Arc::clone(&stream));

        let binder: Arc<dyn IBinder> = stream;
        Ok(OpenInputStreamResult {
            stream: Some(SpIBinder::from_arc(binder)),
            stream_id,
            buffer_size_frames: args.buffer_size_frames,
            sample_rate: args.sample_rate,
            channel_count,
            format: args.format,
        })
    }

    fn get_master_mute(&self) -> AidlResult<bool> {
        Ok(self.master_mute.load(Ordering::SeqCst))
    }

    fn set_master_mute(&self, mute: bool) -> AidlResult<()> {
        self.master_mute.store(mute, Ordering::SeqCst);
        Ok(())
    }

    fn get_master_volume(&self) -> AidlResult<f32> {
        Ok(*self.master_volume.read().unwrap())
    }

    fn set_master_volume(&self, volume: f32) -> AidlResult<()> {
        let clamped = volume.clamp(0.0, 1.0);
        let mut vol = self.master_volume.write().unwrap();
        *vol = clamped;
        Ok(())
    }

    fn get_audio_ports(&self) -> AidlResult<Vec<AudioPort>> {
        Ok(self.ports.read().unwrap().clone())
    }

    fn get_audio_routes(&self) -> AidlResult<Vec<AudioRoute>> {
        Ok(self.routes.read().unwrap().clone())
    }
}

// -----------------------------------------------------------------------------
// Remotable and IBinder Implementations
// -----------------------------------------------------------------------------

impl Remotable for AudioModuleService {
    fn get_class_descriptor() -> &'static str {
        IMODULE_DESCRIPTOR
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
                reply.write_utf8(Some(IMODULE_DESCRIPTOR)).unwrap();
                Ok(())
            }
            imodule_codes::OPEN_OUTPUT_STREAM => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IMODULE_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mut args = OpenOutputStreamArguments::default();
                args.read_from_parcel_at(data, &mut offset)?;

                let res = self.open_output_stream(&args);
                match res {
                    Ok(out) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply
                            .write_i32(out.stream_id)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(out.buffer_size_frames)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(out.sample_rate)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(out.channel_count)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_i32(out.format.into())
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        let handle = out
                            .stream
                            .and_then(|s| s.handle())
                            .unwrap_or(out.stream_id as u32);
                        reply
                            .write_u32(handle)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            imodule_codes::OPEN_INPUT_STREAM => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IMODULE_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mut args = OpenInputStreamArguments::default();
                args.read_from_parcel_at(data, &mut offset)?;

                let res = self.open_input_stream(&args);
                match res {
                    Ok(inp) => {
                        reply.write_status(&Status::ok()).unwrap();
                        reply
                            .write_i32(inp.stream_id)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(inp.buffer_size_frames)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(inp.sample_rate)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_u32(inp.channel_count)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply
                            .write_i32(inp.format.into())
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        let handle = inp
                            .stream
                            .and_then(|s| s.handle())
                            .unwrap_or(inp.stream_id as u32);
                        reply
                            .write_u32(handle)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            imodule_codes::GET_MASTER_MUTE => {
                let res = self.get_master_mute()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_bool(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            imodule_codes::SET_MASTER_MUTE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IMODULE_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mute = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.set_master_mute(mute)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imodule_codes::GET_MASTER_VOLUME => {
                let res = self.get_master_volume()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_f32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            imodule_codes::SET_MASTER_VOLUME => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IMODULE_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let vol = data
                    .read_f32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.set_master_volume(vol)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            imodule_codes::GET_AUDIO_PORTS => {
                let ports = self.get_audio_ports()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(ports.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for p in &ports {
                    reply
                        .write_i32(p.id)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    reply
                        .write_utf16(Some(&p.name))
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    reply
                        .write_bool(p.is_input)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            imodule_codes::GET_AUDIO_ROUTES => {
                let routes = self.get_audio_routes()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(routes.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for r in &routes {
                    reply
                        .write_i32(r.route_id)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    reply
                        .write_i32(r.sink_port_id)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    reply
                        .write_bool(r.is_dynamic)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
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

impl IBinder for AudioModuleService {
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
                    .write_utf16(Some(IMODULE_DESCRIPTOR))
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
        Some(IMODULE_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// AudioModuleProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct AudioModuleProxy {
    binder: SpIBinder,
}

impl AudioModuleProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for AudioModuleProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for AudioModuleProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IModule for AudioModuleProxy {
    fn open_output_stream(
        &self,
        args: &OpenOutputStreamArguments,
    ) -> AidlResult<OpenOutputStreamResult> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        args.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::OPEN_OUTPUT_STREAM, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let stream_id = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let buffer_size_frames = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let sample_rate = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let channel_count = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt_raw = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let stream_binder = if let Some(stream_arc) = get_active_output_stream(stream_id) {
            SpIBinder::from_arc(stream_arc as Arc<dyn IBinder>)
        } else {
            SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0))
        };
        Ok(OpenOutputStreamResult {
            stream: Some(stream_binder),
            stream_id,
            buffer_size_frames,
            sample_rate,
            channel_count,
            format: AudioFormat::from(fmt_raw),
        })
    }

    fn open_input_stream(
        &self,
        args: &OpenInputStreamArguments,
    ) -> AidlResult<OpenInputStreamResult> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        args.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::OPEN_INPUT_STREAM, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let stream_id = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let buffer_size_frames = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let sample_rate = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let channel_count = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let fmt_raw = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle = reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let stream_binder = if let Some(stream_arc) = get_active_input_stream(stream_id) {
            SpIBinder::from_arc(stream_arc as Arc<dyn IBinder>)
        } else {
            SpIBinder::new(aidl_compat::RemoteBinder::new(handle, 0))
        };
        Ok(OpenInputStreamResult {
            stream: Some(stream_binder),
            stream_id,
            buffer_size_frames,
            sample_rate,
            channel_count,
            format: AudioFormat::from(fmt_raw),
        })
    }

    fn get_master_mute(&self) -> AidlResult<bool> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::GET_MASTER_MUTE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let mute = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(mute)
    }

    fn set_master_mute(&self, mute: bool) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(mute)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::SET_MASTER_MUTE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn get_master_volume(&self) -> AidlResult<f32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::GET_MASTER_VOLUME, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let vol = reply
            .read_f32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(vol)
    }

    fn set_master_volume(&self, volume: f32) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_f32(volume)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::SET_MASTER_VOLUME, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn get_audio_ports(&self) -> AidlResult<Vec<AudioPort>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::GET_AUDIO_PORTS, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let count = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let mut list = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let id = reply
                .read_i32(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let name = reply
                .read_utf16(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                .unwrap_or_default();
            let is_input = reply
                .read_bool(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            list.push(AudioPort {
                id,
                name,
                is_input,
                supported_sample_rates: vec![44100, 48000],
                supported_channel_masks: vec![1, 2],
                supported_formats: vec![AudioFormat::Pcm16Bit],
            });
        }
        Ok(list)
    }

    fn get_audio_routes(&self) -> AidlResult<Vec<AudioRoute>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMODULE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(imodule_codes::GET_AUDIO_ROUTES, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let count = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let mut list = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let route_id = reply
                .read_i32(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let sink_port_id = reply
                .read_i32(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            let is_dynamic = reply
                .read_bool(&mut offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            list.push(AudioRoute {
                route_id,
                source_port_ids: vec![sink_port_id],
                sink_port_id,
                is_dynamic,
            });
        }
        Ok(list)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `AudioModuleService` with handle 0 ServiceManager as `"android.hardware.audio.core.IModule/default"`.
pub fn register_audio_service(service: Arc<AudioModuleService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service(IMODULE_DEFAULT_INSTANCE, SpIBinder::from_arc(binder))
}
