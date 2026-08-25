//! AIDL `IPackageManager` Interface, Remotable Server Stub, and Client Proxy.

use crate::package_manager::PackageManagerService;
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_NAME_NOT_FOUND,
    STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use binder_sys::IServiceManager;
use std::sync::Arc;

pub const IPACKAGE_MANAGER_DESCRIPTOR: &str = "android.content.pm.IPackageManager";

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod ipackage_manager_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const GET_PACKAGE_INFO: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_APPLICATION_INFO: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_ACTIVITY_INFO: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const RESOLVE_INTENT: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const QUERY_INTENT_ACTIVITIES: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const CHECK_PERMISSION: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const GET_INSTALLED_PACKAGES: u32 = FIRST_CALL_TRANSACTION + 6; // 7
    pub const GET_INSTALLED_APPLICATIONS: u32 = FIRST_CALL_TRANSACTION + 7; // 8
}

// -----------------------------------------------------------------------------
// IPackageManager Trait Definition
// -----------------------------------------------------------------------------

pub trait IPackageManager: Interface + Send + Sync {
    fn get_package_info(
        &self,
        package_name: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<PackageInfo>>;

    fn get_application_info(
        &self,
        package_name: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ApplicationInfo>>;

    fn get_activity_info(
        &self,
        component: &ComponentName,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ActivityInfo>>;

    fn resolve_intent(
        &self,
        intent: &Intent,
        resolved_type: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ResolveInfo>>;

    fn query_intent_activities(
        &self,
        intent: &Intent,
        resolved_type: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<ResolveInfo>>;

    fn check_permission(
        &self,
        perm_name: &str,
        pkg_name: &str,
        user_id: i32,
    ) -> AidlResult<i32>;

    fn get_installed_packages(
        &self,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<PackageInfo>>;

    fn get_installed_applications(
        &self,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<ApplicationInfo>>;
}

// -----------------------------------------------------------------------------
// Server Remotable Dispatcher Implementation for PackageManagerService
// -----------------------------------------------------------------------------

impl Remotable for PackageManagerService {
    fn get_class_descriptor() -> &'static str {
        IPACKAGE_MANAGER_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        let mut offset = 0;
        // Optionally read descriptor token if present
        let _ = data.read_utf16(&mut offset);

        match code {
            ipackage_manager_codes::GET_PACKAGE_INFO => {
                let pkg_name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.get_package_info(&pkg_name, flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                if let Some(info) = res {
                    reply
                        .write_bool(true)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    info.write_to_parcel(reply)?;
                } else {
                    reply
                        .write_bool(false)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            ipackage_manager_codes::GET_APPLICATION_INFO => {
                let pkg_name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.get_application_info(&pkg_name, flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                if let Some(app) = res {
                    reply
                        .write_bool(true)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    app.write_to_parcel(reply)?;
                } else {
                    reply
                        .write_bool(false)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            ipackage_manager_codes::GET_ACTIVITY_INFO => {
                let mut comp = ComponentName::default();
                comp.read_from_parcel_at(data, &mut offset)?;
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.get_activity_info(&comp, flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                if let Some(act) = res {
                    reply
                        .write_bool(true)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    act.write_to_parcel(reply)?;
                } else {
                    reply
                        .write_bool(false)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            ipackage_manager_codes::RESOLVE_INTENT => {
                let mut intent = Intent::default();
                intent.read_from_parcel_at(data, &mut offset)?;
                let resolved_type = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.resolve_intent(&intent, &resolved_type, flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                if let Some(info) = res {
                    reply
                        .write_bool(true)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    info.write_to_parcel(reply)?;
                } else {
                    reply
                        .write_bool(false)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            ipackage_manager_codes::QUERY_INTENT_ACTIVITIES => {
                let mut intent = Intent::default();
                intent.read_from_parcel_at(data, &mut offset)?;
                let resolved_type = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let results = self.query_intent_activities(&intent, &resolved_type, flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                reply
                    .write_i32(results.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for item in &results {
                    item.write_to_parcel(reply)?;
                }
                Ok(())
            }
            ipackage_manager_codes::CHECK_PERMISSION => {
                let perm_name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let pkg_name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.check_permission(&perm_name, &pkg_name, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            ipackage_manager_codes::GET_INSTALLED_PACKAGES => {
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let pkgs = self.get_installed_packages(flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(pkgs.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for p in &pkgs {
                    p.write_to_parcel(reply)?;
                }
                Ok(())
            }
            ipackage_manager_codes::GET_INSTALLED_APPLICATIONS => {
                let flags = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let user_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let apps = self.get_installed_applications(flags, user_id);
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(apps.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for a in &apps {
                    a.write_to_parcel(reply)?;
                }
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for PackageManagerService {
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
                    .write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn is_binder_alive(&self) -> bool {
        true
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
        Some(IPACKAGE_MANAGER_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy Implementation (`PackageManagerClient`)
// -----------------------------------------------------------------------------

pub struct PackageManagerClient {
    binder: SpIBinder,
}

impl PackageManagerClient {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }

    pub fn from_service_manager() -> AidlResult<Self> {
        let sm = binder_sys::default_service_manager();
        let binder = sm
            .get_service("package")?
            .ok_or_else(|| Status::from_status(STATUS_NAME_NOT_FOUND))?;
        Ok(Self { binder })
    }
}

impl Interface for PackageManagerClient {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for PackageManagerClient {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IPackageManager for PackageManagerClient {
    fn get_package_info(
        &self,
        package_name: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<PackageInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::GET_PACKAGE_INFO,
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

        let has_info = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_info {
            let mut info = PackageInfo::default();
            info.read_from_parcel_at(&reply, &mut offset)?;
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    fn get_application_info(
        &self,
        package_name: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ApplicationInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(package_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::GET_APPLICATION_INFO,
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

        let has_app = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_app {
            let mut app = ApplicationInfo::default();
            app.read_from_parcel_at(&reply, &mut offset)?;
            Ok(Some(app))
        } else {
            Ok(None)
        }
    }

    fn get_activity_info(
        &self,
        component: &ComponentName,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ActivityInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        component.write_to_parcel(&mut data)?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::GET_ACTIVITY_INFO,
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

        let has_act = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_act {
            let mut act = ActivityInfo::default();
            act.read_from_parcel_at(&reply, &mut offset)?;
            Ok(Some(act))
        } else {
            Ok(None)
        }
    }

    fn resolve_intent(
        &self,
        intent: &Intent,
        resolved_type: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Option<ResolveInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        intent.write_to_parcel(&mut data)?;
        data.write_utf8(Some(resolved_type))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::RESOLVE_INTENT,
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

        let has_res = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_res {
            let mut res = ResolveInfo::default();
            res.read_from_parcel_at(&reply, &mut offset)?;
            Ok(Some(res))
        } else {
            Ok(None)
        }
    }

    fn query_intent_activities(
        &self,
        intent: &Intent,
        resolved_type: &str,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<ResolveInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        intent.write_to_parcel(&mut data)?;
        data.write_utf8(Some(resolved_type))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::QUERY_INTENT_ACTIVITIES,
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
        let mut results = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count.max(0) {
            let mut item = ResolveInfo::default();
            item.read_from_parcel_at(&reply, &mut offset)?;
            results.push(item);
        }
        Ok(results)
    }

    fn check_permission(
        &self,
        perm_name: &str,
        pkg_name: &str,
        user_id: i32,
    ) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(perm_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(pkg_name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::CHECK_PERMISSION,
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
        Ok(res)
    }

    fn get_installed_packages(
        &self,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<PackageInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::GET_INSTALLED_PACKAGES,
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
        let mut results = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count.max(0) {
            let mut item = PackageInfo::default();
            item.read_from_parcel_at(&reply, &mut offset)?;
            results.push(item);
        }
        Ok(results)
    }

    fn get_installed_applications(
        &self,
        flags: i64,
        user_id: i32,
    ) -> AidlResult<Vec<ApplicationInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(user_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            ipackage_manager_codes::GET_INSTALLED_APPLICATIONS,
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
        let mut results = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count.max(0) {
            let mut item = ApplicationInfo::default();
            item.read_from_parcel_at(&reply, &mut offset)?;
            results.push(item);
        }
        Ok(results)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `PackageManagerService` with handle 0 ServiceManager as `"package"`.
pub fn register_package_service(service: Arc<PackageManagerService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service("package", SpIBinder::from_arc(binder))
}
