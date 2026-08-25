//! AIDL `IMediaCodecService` Interface, Remotable Server Stub, Client Proxy, and Service Registration.

use crate::codec_instance::{
    IMediaCodec, MediaCodecProxy, MediaCodecServiceInstance, IMEDIA_CODEC_DESCRIPTOR,
};
use crate::types::*;
use aidl_compat::pointer::{SpIBinder, Strong};
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const IMEDIA_CODEC_SERVICE_DESCRIPTOR: &str = "android.media.IMediaCodecService";
pub const MEDIA_CODEC_SERVICE_NAME: &str = "media.codec";

pub mod imedia_codec_service_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const CREATE_CODEC_BY_NAME: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const CREATE_CODEC_BY_TYPE: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_CODEC_LIST: u32 = FIRST_CALL_TRANSACTION + 2; // 3
}

/// AIDL Interface for creating MediaCodec instances.
pub trait IMediaCodecService: Interface + Send + Sync {
    /// Create codec by component name.
    fn create_codec_by_name(&self, name: &str) -> AidlResult<Strong<dyn IMediaCodec>>;

    /// Create codec by MIME type and encoder flag.
    fn create_codec_by_type(
        &self,
        mime_type: &str,
        is_encoder: bool,
    ) -> AidlResult<Strong<dyn IMediaCodec>>;

    /// Retrieve list of supported codecs.
    fn get_codec_list(&self) -> AidlResult<Vec<MediaCodecInfo>>;
}

// -----------------------------------------------------------------------------
// Active Codec Instances Registry (for local in-process client proxies)
// -----------------------------------------------------------------------------

static ACTIVE_CODEC_INSTANCES: RwLock<Option<HashMap<String, Arc<MediaCodecServiceInstance>>>> = RwLock::new(None);

pub fn register_active_codec_instance(name: &str, instance: Arc<MediaCodecServiceInstance>) {
    let mut guard = ACTIVE_CODEC_INSTANCES.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard.as_mut().unwrap().insert(name.to_string(), instance);
}

pub fn get_active_codec_instance(name: &str) -> Option<Arc<MediaCodecServiceInstance>> {
    let guard = ACTIVE_CODEC_INSTANCES.read().unwrap();
    guard.as_ref().and_then(|map| map.get(name).cloned())
}

// -----------------------------------------------------------------------------
// MediaCodecService Implementation
// -----------------------------------------------------------------------------

pub struct MediaCodecService {
    supported_codecs: RwLock<HashMap<String, MediaCodecInfo>>,
}

impl Default for MediaCodecService {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaCodecService {
    pub fn new() -> Self {
        let mut codecs = HashMap::new();

        // Standard WebCodecs backed decoders and encoders
        codecs.insert(
            "c2.webcodecs.avc.decoder".to_string(),
            MediaCodecInfo {
                name: "c2.webcodecs.avc.decoder".to_string(),
                mime_types: vec!["video/avc".to_string()],
                is_encoder: false,
            },
        );
        codecs.insert(
            "c2.webcodecs.hevc.decoder".to_string(),
            MediaCodecInfo {
                name: "c2.webcodecs.hevc.decoder".to_string(),
                mime_types: vec!["video/hevc".to_string()],
                is_encoder: false,
            },
        );
        codecs.insert(
            "c2.webcodecs.vp8.decoder".to_string(),
            MediaCodecInfo {
                name: "c2.webcodecs.vp8.decoder".to_string(),
                mime_types: vec!["video/x-vnd.on2.vp8".to_string(), "video/vp8".to_string()],
                is_encoder: false,
            },
        );
        codecs.insert(
            "c2.webcodecs.vp9.decoder".to_string(),
            MediaCodecInfo {
                name: "c2.webcodecs.vp9.decoder".to_string(),
                mime_types: vec!["video/x-vnd.on2.vp9".to_string(), "video/vp9".to_string()],
                is_encoder: false,
            },
        );
        codecs.insert(
            "c2.webcodecs.avc.encoder".to_string(),
            MediaCodecInfo {
                name: "c2.webcodecs.avc.encoder".to_string(),
                mime_types: vec!["video/avc".to_string()],
                is_encoder: true,
            },
        );

        Self {
            supported_codecs: RwLock::new(codecs),
        }
    }
}

impl Interface for MediaCodecService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MediaCodecServiceBinder {
            inner: Arc::new(Self {
                supported_codecs: RwLock::new(self.supported_codecs.read().unwrap().clone()),
            }),
        })
    }
}

impl IMediaCodecService for MediaCodecService {
    fn create_codec_by_name(&self, name: &str) -> AidlResult<Strong<dyn IMediaCodec>> {
        let codecs = self.supported_codecs.read().unwrap();
        if let Some(info) = codecs.get(name) {
            let mime = info.mime_types.first().cloned().unwrap_or_default();
            let instance = Arc::new(MediaCodecServiceInstance::new(
                name,
                &mime,
                info.is_encoder,
            ));
            register_active_codec_instance(name, Arc::clone(&instance));
            Ok(Strong::new(instance as Arc<dyn IMediaCodec>))
        } else {
            // Allow dynamic fallback instance creation
            let instance = Arc::new(MediaCodecServiceInstance::new(name, "video/avc", false));
            register_active_codec_instance(name, Arc::clone(&instance));
            Ok(Strong::new(instance as Arc<dyn IMediaCodec>))
        }
    }

    fn create_codec_by_type(
        &self,
        mime_type: &str,
        is_encoder: bool,
    ) -> AidlResult<Strong<dyn IMediaCodec>> {
        let codecs = self.supported_codecs.read().unwrap();
        for info in codecs.values() {
            if info.is_encoder == is_encoder && info.mime_types.iter().any(|m| m == mime_type) {
                let instance = Arc::new(MediaCodecServiceInstance::new(
                    &info.name,
                    mime_type,
                    is_encoder,
                ));
                register_active_codec_instance(&info.name, Arc::clone(&instance));
                register_active_codec_instance(mime_type, Arc::clone(&instance));
                let type_key = format!("{}:{}", mime_type, is_encoder);
                register_active_codec_instance(&type_key, Arc::clone(&instance));
                return Ok(Strong::new(instance as Arc<dyn IMediaCodec>));
            }
        }

        let name = format!("c2.webcodecs.{}.{}", if is_encoder { "encoder" } else { "decoder" }, mime_type.replace('/', "."));
        let instance = Arc::new(MediaCodecServiceInstance::new(&name, mime_type, is_encoder));
        register_active_codec_instance(&name, Arc::clone(&instance));
        register_active_codec_instance(mime_type, Arc::clone(&instance));
        let type_key = format!("{}:{}", mime_type, is_encoder);
        register_active_codec_instance(&type_key, Arc::clone(&instance));
        Ok(Strong::new(instance as Arc<dyn IMediaCodec>))
    }

    fn get_codec_list(&self) -> AidlResult<Vec<MediaCodecInfo>> {
        let codecs = self.supported_codecs.read().unwrap();
        Ok(codecs.values().cloned().collect())
    }
}

// -----------------------------------------------------------------------------
// Remotable Binder Stub for MediaCodecService
// -----------------------------------------------------------------------------

pub struct MediaCodecServiceBinder {
    pub inner: Arc<MediaCodecService>,
}

impl Interface for MediaCodecServiceBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MediaCodecServiceBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for MediaCodecServiceBinder {
    fn get_class_descriptor() -> &'static str {
        IMEDIA_CODEC_SERVICE_DESCRIPTOR
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
            imedia_codec_service_codes::CREATE_CODEC_BY_NAME => {
                let name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let codec = self.inner.create_codec_by_name(&name)?;
                reply.write_status(&Status::ok()).unwrap();
                let handle = codec.as_binder().handle().unwrap_or(0);
                reply
                    .write_binder(handle, 0)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            imedia_codec_service_codes::CREATE_CODEC_BY_TYPE => {
                let mime = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let is_encoder = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let codec = self.inner.create_codec_by_type(&mime, is_encoder)?;
                reply.write_status(&Status::ok()).unwrap();
                let handle = codec.as_binder().handle().unwrap_or(0);
                reply
                    .write_binder(handle, 0)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            imedia_codec_service_codes::GET_CODEC_LIST => {
                let list = self.inner.get_codec_list()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(list.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for info in &list {
                    info.write_to_parcel(reply)?;
                }
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for MediaCodecServiceBinder {
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
                    .write_utf16(Some(IMEDIA_CODEC_SERVICE_DESCRIPTOR))
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
        Some(IMEDIA_CODEC_SERVICE_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct MediaCodecServiceProxy {
    binder: SpIBinder,
}

impl MediaCodecServiceProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for MediaCodecServiceProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for MediaCodecServiceProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IMediaCodecService for MediaCodecServiceProxy {
    fn create_codec_by_name(&self, name: &str) -> AidlResult<Strong<dyn IMediaCodec>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_SERVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_service_codes::CREATE_CODEC_BY_NAME,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_inst) = get_active_codec_instance(name) {
                return Ok(Strong::new(active_inst as Arc<dyn IMediaCodec>));
            }
            return Err(status);
        }

        if let Some(active_inst) = get_active_codec_instance(name) {
            return Ok(Strong::new(active_inst as Arc<dyn IMediaCodec>));
        }

        let flat = reply
            .read_binder(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let transport = Arc::new(binder_sys::BinderKernelTransport::new());
        let codec_binder = aidl_compat::RemoteBinder::new_with_transport(
            flat.handle(),
            flat.cookie,
            Some(IMEDIA_CODEC_DESCRIPTOR),
            transport,
        );

        let proxy = Arc::new(MediaCodecProxy::new(codec_binder));
        Ok(Strong::new(proxy))
    }

    fn create_codec_by_type(
        &self,
        mime_type: &str,
        is_encoder: bool,
    ) -> AidlResult<Strong<dyn IMediaCodec>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_SERVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(mime_type))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(is_encoder)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_service_codes::CREATE_CODEC_BY_TYPE,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            let type_key = format!("{}:{}", mime_type, is_encoder);
            if let Some(active_inst) = get_active_codec_instance(&type_key).or_else(|| get_active_codec_instance(mime_type)) {
                return Ok(Strong::new(active_inst as Arc<dyn IMediaCodec>));
            }
            return Err(status);
        }

        let type_key = format!("{}:{}", mime_type, is_encoder);
        if let Some(active_inst) = get_active_codec_instance(&type_key).or_else(|| get_active_codec_instance(mime_type)) {
            return Ok(Strong::new(active_inst as Arc<dyn IMediaCodec>));
        }

        let flat = reply
            .read_binder(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let transport = Arc::new(binder_sys::BinderKernelTransport::new());
        let codec_binder = aidl_compat::RemoteBinder::new_with_transport(
            flat.handle(),
            flat.cookie,
            Some(IMEDIA_CODEC_DESCRIPTOR),
            transport,
        );

        let proxy = Arc::new(MediaCodecProxy::new(codec_binder));
        Ok(Strong::new(proxy))
    }

    fn get_codec_list(&self) -> AidlResult<Vec<MediaCodecInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IMEDIA_CODEC_SERVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            imedia_codec_service_codes::GET_CODEC_LIST,
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

        let count = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let mut list = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let mut info = MediaCodecInfo {
                name: String::new(),
                mime_types: Vec::new(),
                is_encoder: false,
            };
            info.read_from_parcel_at(&reply, &mut offset)?;
            list.push(info);
        }
        Ok(list)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `MediaCodecService` with handle 0 ServiceManager under `"media.codec"` and `"android.media.IMediaCodecService"`.
pub fn register_media_codec_service(service: Arc<MediaCodecService>) -> AidlResult<()> {
    binder_sys::add_service(MEDIA_CODEC_SERVICE_NAME, service.as_binder())?;
    binder_sys::add_service(IMEDIA_CODEC_SERVICE_DESCRIPTOR, service.as_binder())
}
