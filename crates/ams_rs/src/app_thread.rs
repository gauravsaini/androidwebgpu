//! AIDL `IApplicationThread` Interface, Client Proxy, and Mock/Stub Server.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use pms_rs::types::ApplicationInfo;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub const IAPPLICATION_THREAD_DESCRIPTOR: &str = "android.app.IApplicationThread";

// -----------------------------------------------------------------------------
// Transaction Codes
// -----------------------------------------------------------------------------

pub mod application_thread_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const SCHEDULE_TRANSACTION: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const BIND_APPLICATION: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const SCHEDULE_RESUME_ACTIVITY: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const SCHEDULE_PAUSE_ACTIVITY: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const SCHEDULE_STOP_ACTIVITY: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const SCHEDULE_DESTROY_ACTIVITY: u32 = FIRST_CALL_TRANSACTION + 5; // 6
}

// -----------------------------------------------------------------------------
// Client Transaction Models
// -----------------------------------------------------------------------------

/// Item representing a specific lifecycle callback in a client transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientTransactionItem {
    ResumeActivity { is_forward: bool },
    PauseActivity { is_finishing: bool, user_leaving: bool },
    StopActivity,
    DestroyActivity { is_finishing: bool },
}

/// Transaction container sent to client `ActivityThread` to drive lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientTransaction {
    pub activity_token_id: u32,
    pub items: Vec<ClientTransactionItem>,
}

impl Parcelable for ClientTransaction {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_u32(self.activity_token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.items.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        for item in &self.items {
            match item {
                ClientTransactionItem::ResumeActivity { is_forward } => {
                    parcel
                        .write_i32(1)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    parcel
                        .write_bool(*is_forward)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                ClientTransactionItem::PauseActivity {
                    is_finishing,
                    user_leaving,
                } => {
                    parcel
                        .write_i32(2)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    parcel
                        .write_bool(*is_finishing)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    parcel
                        .write_bool(*user_leaving)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                ClientTransactionItem::StopActivity => {
                    parcel
                        .write_i32(3)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                ClientTransactionItem::DestroyActivity { is_finishing } => {
                    parcel
                        .write_i32(4)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    parcel
                        .write_bool(*is_finishing)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
            }
        }
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> AidlResult<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.activity_token_id = parcel
            .read_u32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        self.items.clear();
        for _ in 0..count.max(0) {
            let item_type = parcel
                .read_i32(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            match item_type {
                1 => {
                    let is_forward = parcel
                        .read_bool(offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    self.items
                        .push(ClientTransactionItem::ResumeActivity { is_forward });
                }
                2 => {
                    let is_finishing = parcel
                        .read_bool(offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    let user_leaving = parcel
                        .read_bool(offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    self.items.push(ClientTransactionItem::PauseActivity {
                        is_finishing,
                        user_leaving,
                    });
                }
                3 => {
                    self.items.push(ClientTransactionItem::StopActivity);
                }
                4 => {
                    let is_finishing = parcel
                        .read_bool(offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    self.items
                        .push(ClientTransactionItem::DestroyActivity { is_finishing });
                }
                _ => return Err(Status::from_status(STATUS_BAD_VALUE)),
            }
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// IApplicationThread Trait
// -----------------------------------------------------------------------------

/// Trait defining the `IApplicationThread` AIDL contract.
pub trait IApplicationThread: Send + Sync + 'static {
    /// Bind application package metadata to client process.
    fn bind_application(
        &self,
        package_name: &str,
        app_info: &ApplicationInfo,
        process_name: &str,
    ) -> AidlResult<()>;

    /// Schedule an activity transition to RESUMED.
    fn schedule_resume_activity(&self, token: &SpIBinder, is_forward: bool) -> AidlResult<()>;

    /// Schedule an activity transition to PAUSED.
    fn schedule_pause_activity(
        &self,
        token: &SpIBinder,
        is_finishing: bool,
        user_leaving: bool,
    ) -> AidlResult<()>;

    /// Schedule an activity transition to STOPPED.
    fn schedule_stop_activity(&self, token: &SpIBinder) -> AidlResult<()>;

    /// Schedule an activity transition to DESTROYED.
    fn schedule_destroy_activity(&self, token: &SpIBinder, is_finishing: bool) -> AidlResult<()>;

    /// Schedule a batched client transaction.
    fn schedule_transaction(&self, transaction: &ClientTransaction) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// Anonymous Binder Token Helper
// -----------------------------------------------------------------------------

/// A simple anonymous local Binder token object used for activity identification.
pub struct ActivityTokenBinder {
    id: u32,
}

impl ActivityTokenBinder {
    pub fn new(id: u32) -> SpIBinder {
        SpIBinder::new(Self { id })
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

impl IBinder for ActivityTokenBinder {
    fn transact(
        &self,
        _code: TransactionCode,
        _flags: TransactionFlags,
        _data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        let _ = reply.write_status(&Status::ok());
        Ok(())
    }

    fn handle(&self) -> Option<u32> {
        Some(self.id)
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ApplicationThreadProxy (Client Proxy)
// -----------------------------------------------------------------------------

/// Proxy implementation calling `android.app.IApplicationThread` over Binder IPC.
pub struct ApplicationThreadProxy {
    binder: SpIBinder,
}

impl ApplicationThreadProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Proxy for ApplicationThreadProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl Interface for ApplicationThreadProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl IApplicationThread for ApplicationThreadProxy {
    fn bind_application(
        &self,
        package_name: &str,
        app_info: &ApplicationInfo,
        process_name: &str,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        app_info.write_to_parcel(&mut data)?;
        data.write_utf8(Some(process_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::BIND_APPLICATION,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn schedule_resume_activity(&self, token: &SpIBinder, is_forward: bool) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle_id = token.handle().unwrap_or(1);
        data.write_u32(handle_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(is_forward)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::SCHEDULE_RESUME_ACTIVITY,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn schedule_pause_activity(
        &self,
        token: &SpIBinder,
        is_finishing: bool,
        user_leaving: bool,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle_id = token.handle().unwrap_or(1);
        data.write_u32(handle_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(is_finishing)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(user_leaving)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::SCHEDULE_PAUSE_ACTIVITY,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn schedule_stop_activity(&self, token: &SpIBinder) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle_id = token.handle().unwrap_or(1);
        data.write_u32(handle_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::SCHEDULE_STOP_ACTIVITY,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn schedule_destroy_activity(&self, token: &SpIBinder, is_finishing: bool) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let handle_id = token.handle().unwrap_or(1);
        data.write_u32(handle_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(is_finishing)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::SCHEDULE_DESTROY_ACTIVITY,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }

    fn schedule_transaction(&self, transaction: &ClientTransaction) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        transaction.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder.transact(
            application_thread_codes::SCHEDULE_TRANSACTION,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if status.is_ok() {
            Ok(())
        } else {
            Err(status)
        }
    }
}

// -----------------------------------------------------------------------------
// MockApplicationThread (In-Memory & Remotable Stub for Tests)
// -----------------------------------------------------------------------------

/// In-memory mock and stub for `IApplicationThread` recording lifecycle invocations.
#[derive(Clone, Default)]
pub struct MockApplicationThread {
    pub bound_applications: Arc<RwLock<Vec<(String, ApplicationInfo, String)>>>,
    pub resumed_activities: Arc<RwLock<Vec<(u32, bool)>>>,
    pub paused_activities: Arc<RwLock<Vec<(u32, bool, bool)>>>,
    pub stopped_activities: Arc<RwLock<Vec<u32>>>,
    pub destroyed_activities: Arc<RwLock<Vec<(u32, bool)>>>,
    pub scheduled_transactions: Arc<RwLock<Vec<ClientTransaction>>>,
}

impl MockApplicationThread {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IApplicationThread for MockApplicationThread {
    fn bind_application(
        &self,
        package_name: &str,
        app_info: &ApplicationInfo,
        process_name: &str,
    ) -> AidlResult<()> {
        let mut lock = self.bound_applications.write().unwrap();
        lock.push((
            package_name.to_string(),
            app_info.clone(),
            process_name.to_string(),
        ));
        Ok(())
    }

    fn schedule_resume_activity(&self, token: &SpIBinder, is_forward: bool) -> AidlResult<()> {
        let handle_id = token.handle().unwrap_or(1);
        let mut lock = self.resumed_activities.write().unwrap();
        lock.push((handle_id, is_forward));
        Ok(())
    }

    fn schedule_pause_activity(
        &self,
        token: &SpIBinder,
        is_finishing: bool,
        user_leaving: bool,
    ) -> AidlResult<()> {
        let handle_id = token.handle().unwrap_or(1);
        let mut lock = self.paused_activities.write().unwrap();
        lock.push((handle_id, is_finishing, user_leaving));
        Ok(())
    }

    fn schedule_stop_activity(&self, token: &SpIBinder) -> AidlResult<()> {
        let handle_id = token.handle().unwrap_or(1);
        let mut lock = self.stopped_activities.write().unwrap();
        lock.push(handle_id);
        Ok(())
    }

    fn schedule_destroy_activity(&self, token: &SpIBinder, is_finishing: bool) -> AidlResult<()> {
        let handle_id = token.handle().unwrap_or(1);
        let mut lock = self.destroyed_activities.write().unwrap();
        lock.push((handle_id, is_finishing));
        Ok(())
    }

    fn schedule_transaction(&self, transaction: &ClientTransaction) -> AidlResult<()> {
        let mut lock = self.scheduled_transactions.write().unwrap();
        lock.push(transaction.clone());
        Ok(())
    }
}

impl Remotable for MockApplicationThread {
    fn get_class_descriptor() -> &'static str {
        IAPPLICATION_THREAD_DESCRIPTOR
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
            application_thread_codes::BIND_APPLICATION => {
                let pkg = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let mut app_info = ApplicationInfo::default();
                app_info.read_from_parcel_at(data, &mut offset)?;
                let proc = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();

                self.bind_application(&pkg, &app_info, &proc)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            application_thread_codes::SCHEDULE_RESUME_ACTIVITY => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let is_forward = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.schedule_resume_activity(&token, is_forward)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            application_thread_codes::SCHEDULE_PAUSE_ACTIVITY => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let is_finishing = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_leaving = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.schedule_pause_activity(&token, is_finishing, user_leaving)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            application_thread_codes::SCHEDULE_STOP_ACTIVITY => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.schedule_stop_activity(&token)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            application_thread_codes::SCHEDULE_DESTROY_ACTIVITY => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let is_finishing = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.schedule_destroy_activity(&token, is_finishing)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            application_thread_codes::SCHEDULE_TRANSACTION => {
                let mut tx = ClientTransaction {
                    activity_token_id: 0,
                    items: Vec::new(),
                };
                tx.read_from_parcel_at(data, &mut offset)?;
                self.schedule_transaction(&tx)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for MockApplicationThread {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => Ok(()),
            INTERFACE_TRANSACTION => {
                reply
                    .write_utf16(Some(IAPPLICATION_THREAD_DESCRIPTOR))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(IAPPLICATION_THREAD_DESCRIPTOR)
    }
}
