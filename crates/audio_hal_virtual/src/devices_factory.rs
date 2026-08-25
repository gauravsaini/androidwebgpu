//! AIDL `IDevicesFactory` Interface and Service.

use crate::audio_module::AudioModuleService;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::{IBinder, Interface, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::sync::Arc;

pub const IDEVICES_FACTORY_DESCRIPTOR: &str = "android.hardware.audio.core.IDevicesFactory";

pub mod idevices_factory_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const OPEN_DEVICE: u32 = FIRST_CALL_TRANSACTION; // 1
}

pub trait IDevicesFactory: Interface + Send + Sync {
    fn open_device(&self, name: &str) -> AidlResult<SpIBinder>;
}

pub struct DevicesFactoryService {
    primary_module: Arc<AudioModuleService>,
}

impl DevicesFactoryService {
    pub fn new(primary_module: Arc<AudioModuleService>) -> Self {
        Self { primary_module }
    }
}

impl Interface for DevicesFactoryService {
    fn as_binder(&self) -> SpIBinder {
        let binder: Arc<dyn IBinder> = Arc::new(Self {
            primary_module: Arc::clone(&self.primary_module),
        });
        SpIBinder::from_arc(binder)
    }
}

impl IDevicesFactory for DevicesFactoryService {
    fn open_device(&self, _name: &str) -> AidlResult<SpIBinder> {
        let binder: Arc<dyn IBinder> = Arc::clone(&self.primary_module) as Arc<dyn IBinder>;
        Ok(SpIBinder::from_arc(binder))
    }
}

impl Remotable for DevicesFactoryService {
    fn get_class_descriptor() -> &'static str {
        IDEVICES_FACTORY_DESCRIPTOR
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
                reply.write_utf8(Some(IDEVICES_FACTORY_DESCRIPTOR)).unwrap();
                Ok(())
            }
            idevices_factory_codes::OPEN_DEVICE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IDEVICES_FACTORY_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let name = data
                    .read_utf16(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();

                let res = self.open_device(&name);
                match res {
                    Ok(binder) => {
                        reply.write_status(&Status::ok()).unwrap();
                        let handle = binder.handle().unwrap_or(1);
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
            _ => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
        }
    }
}

impl IBinder for DevicesFactoryService {
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
                    .write_utf16(Some(IDEVICES_FACTORY_DESCRIPTOR))
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
        Some(IDEVICES_FACTORY_DESCRIPTOR)
    }
}
