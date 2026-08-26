use crate::app_thread::{ActivityTokenBinder, ApplicationThreadProxy, IApplicationThread};
use crate::lifecycle::LifecycleManager;
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
use pms_rs::service::IPackageManager;
use pms_rs::types::{ActivityInfo, ApplicationInfo, Intent};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use zygote_client::socket::ZygoteClient;
use zygote_client::ProcessState;

pub const IACTIVITY_MANAGER_DESCRIPTOR: &str = "android.app.IActivityManager";

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod iactivity_manager_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const START_ACTIVITY: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const ATTACH_APPLICATION: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const ACTIVITY_RESUMED: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const ACTIVITY_PAUSED: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const ACTIVITY_STOPPED: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const FINISH_ACTIVITY: u32 = FIRST_CALL_TRANSACTION + 5; // 6
}

// -----------------------------------------------------------------------------
// IActivityManager Trait Definition
// -----------------------------------------------------------------------------

pub trait IActivityManager: Send + Sync + 'static {
    fn start_activity(
        &self,
        caller: Option<SpIBinder>,
        calling_package: Option<String>,
        intent: &Intent,
        resolved_type: Option<&str>,
        result_to: Option<SpIBinder>,
        result_who: Option<&str>,
        request_code: i32,
        flags: i32,
        profiler_info: Option<&str>,
        options: Option<&[u8]>,
    ) -> AidlResult<i32>;

    fn attach_application(&self, thread: SpIBinder, start_seq: i64) -> AidlResult<()>;

    fn activity_resumed(&self, token: SpIBinder) -> AidlResult<()>;

    fn activity_paused(&self, token: SpIBinder) -> AidlResult<()>;

    fn activity_stopped(&self, token: SpIBinder, state: Option<&[u8]>) -> AidlResult<()>;

    fn finish_activity(
        &self,
        token: SpIBinder,
        result_code: i32,
        result_data: Option<&Intent>,
        finish_task: i32,
    ) -> AidlResult<bool>;
}

// -----------------------------------------------------------------------------
// ActivityManagerService Implementation
// -----------------------------------------------------------------------------

pub struct ActivityManagerService {
    lifecycle: Arc<LifecycleManager>,
    pms: Arc<dyn IPackageManager>,
    zygote: Arc<ZygoteClient>,
    pending_launches: Arc<RwLock<VecDeque<PendingLaunch>>>,
    process_threads: Arc<RwLock<HashMap<u32, Arc<dyn IApplicationThread>>>>,
    next_token_id: AtomicU32,
}

impl ActivityManagerService {
    /// Create a new native ActivityManagerService with specified PMS and Zygote clients.
    pub fn new(pms: Arc<dyn IPackageManager>, zygote: Arc<ZygoteClient>) -> Self {
        Self {
            lifecycle: Arc::new(LifecycleManager::new()),
            pms,
            zygote,
            pending_launches: Arc::new(RwLock::new(VecDeque::new())),
            process_threads: Arc::new(RwLock::new(HashMap::new())),
            next_token_id: AtomicU32::new(100),
        }
    }

    /// Access reference to the internal lifecycle manager.
    pub fn lifecycle(&self) -> &Arc<LifecycleManager> {
        &self.lifecycle
    }

    /// Access reference to the internal Zygote client.
    pub fn zygote(&self) -> &Arc<ZygoteClient> {
        &self.zygote
    }

    /// Access reference to the PMS client.
    pub fn pms(&self) -> &Arc<dyn IPackageManager> {
        &self.pms
    }

    /// Resolve an activity and its parent application via PMS.
    fn resolve_target_activity(
        &self,
        intent: &Intent,
        resolved_type: Option<&str>,
    ) -> AidlResult<(ActivityInfo, ApplicationInfo)> {
        // First try direct component if specified
        if let Some(ref comp) = intent.component {
            if let Some(act) = self.pms.get_activity_info(comp, 0, 0)? {
                let app = self
                    .pms
                    .get_application_info(&act.package_name, 0, 0)?
                    .unwrap_or_else(|| ApplicationInfo {
                        package_name: act.package_name.clone(),
                        ..Default::default()
                    });
                return Ok((act, app));
            }
        }

        // Second: try Intent resolution
        let r_type = resolved_type.unwrap_or("");
        if let Some(resolve_info) = self.pms.resolve_intent(intent, r_type, 0, 0)? {
            if let Some(act) = resolve_info.activity_info {
                let app = self
                    .pms
                    .get_application_info(&act.package_name, 0, 0)?
                    .unwrap_or_else(|| ApplicationInfo {
                        package_name: act.package_name.clone(),
                        ..Default::default()
                    });
                return Ok((act, app));
            }
        }

        // Third: try package lookup if package is set
        if let Some(ref pkg) = intent.package {
            if let Some(pkg_info) = self.pms.get_package_info(pkg, 0, 0)? {
                if let Some(first_act) = pkg_info.activities.into_iter().next() {
                    let app = pkg_info.application_info.unwrap_or_else(|| ApplicationInfo {
                        package_name: pkg.clone(),
                        ..Default::default()
                    });
                    return Ok((first_act, app));
                }
            }
        }

        Err(Status::from_status(STATUS_NAME_NOT_FOUND))
    }
}

impl IActivityManager for ActivityManagerService {
    fn start_activity(
        &self,
        _caller: Option<SpIBinder>,
        _calling_package: Option<String>,
        intent: &Intent,
        resolved_type: Option<&str>,
        result_to: Option<SpIBinder>,
        result_who: Option<&str>,
        request_code: i32,
        _flags: i32,
        _profiler_info: Option<&str>,
        _options: Option<&[u8]>,
    ) -> AidlResult<i32> {
        let (act_info, app_info) = match self.resolve_target_activity(intent, resolved_type) {
            Ok(pair) => pair,
            Err(_) => return Ok(START_INTENT_NOT_RESOLVED),
        };

        let token_id = self.next_token_id.fetch_add(1, Ordering::SeqCst);
        let token = ActivityTokenBinder::new(token_id);

        let record_arc = self.lifecycle.create_activity(
            token.clone(),
            intent.clone(),
            act_info.clone(),
            app_info.clone(),
        );

        {
            let mut record = record_arc.write().unwrap();
            record.result_to = result_to;
            record.result_who = result_who.map(|s| s.to_string());
            record.request_code = request_code;
        }

        let package_name = app_info.package_name.clone();
        let process_name = app_info.name.clone().unwrap_or_else(|| package_name.clone());

        // Check if process is already running and attached
        let existing_proc = self.zygote.tracker().get_process_by_package(&package_name);
        if let Some(proc_record) = existing_proc {
            let pid = proc_record.pid;
            let thread_opt = {
                let threads = self.process_threads.read().unwrap();
                threads.get(&pid).cloned()
            };

            if let Some(app_thread) = thread_opt {
                {
                    let mut record = record_arc.write().unwrap();
                    record.pid = Some(pid);
                }
                // Transition: INITIALIZING -> CREATED -> STARTED
                let _ = self
                    .lifecycle
                    .transition_activity(&token, ActivityState::CREATED);
                let _ = self
                    .lifecycle
                    .transition_activity(&token, ActivityState::STARTED);
                // Schedule resume on client
                app_thread.schedule_resume_activity(&token, true)?;
                return Ok(START_SUCCESS);
            }
        }

        // Cold Start: Fork application process via Zygote
        let uid = app_info.uid.max(0) as u32;
        let gid = app_info.uid.max(0) as u32;
        let target_sdk = app_info.target_sdk_version.max(0) as u32;

        let pid = self
            .zygote
            .fork_app_simple(&package_name, &process_name, uid, gid, target_sdk)
            .map_err(|e| {
                log::error!("Zygote fork failed: {e}");
                Status::from_status(aidl_compat::status::STATUS_UNKNOWN_ERROR)
            })?;

        {
            let mut record = record_arc.write().unwrap();
            record.pid = Some(pid);
        }

        // Register pending launch waiting for attachApplication
        {
            let mut pending = self.pending_launches.write().unwrap();
            pending.push_back(PendingLaunch {
                pid,
                package_name,
                process_name,
                activity_record: record_arc,
            });
        }

        Ok(START_SUCCESS)
    }

    fn attach_application(&self, thread: SpIBinder, start_seq: i64) -> AidlResult<()> {
        let app_thread: Arc<dyn IApplicationThread> =
            Arc::new(ApplicationThreadProxy::new(thread));

        // Match pending launch by PID or fall back to FIFO head
        let pending = {
            let mut lock = self.pending_launches.write().unwrap();
            if start_seq > 0 {
                if let Some(pos) = lock.iter().position(|p| p.pid == start_seq as u32) {
                    lock.remove(pos)
                } else {
                    lock.pop_front()
                }
            } else {
                lock.pop_front()
            }
        };

        if let Some(pending) = pending {
            let pid = pending.pid;
            let package_name = pending.package_name;
            let process_name = pending.process_name;
            let record_arc = pending.activity_record;

            // Save thread
            {
                let mut threads = self.process_threads.write().unwrap();
                threads.insert(pid, app_thread.clone());
            }

            // Update process tracker state
            let _ = self
                .zygote
                .tracker()
                .update_process_state(pid, ProcessState::Running);

            let (app_info, token) = {
                let rec = record_arc.read().unwrap();
                (rec.app_info.clone(), rec.token.clone())
            };

            // 1. Bind Application metadata to client
            app_thread.bind_application(&package_name, &app_info, &process_name)?;

            // 2. Drive Activity Lifecycle: CREATED -> STARTED
            let _ = self
                .lifecycle
                .transition_activity(&token, ActivityState::CREATED);
            let _ = self
                .lifecycle
                .transition_activity(&token, ActivityState::STARTED);

            // 3. Schedule Activity Resume
            app_thread.schedule_resume_activity(&token, true)?;
        }

        Ok(())
    }

    fn activity_resumed(&self, token: SpIBinder) -> AidlResult<()> {
        self.lifecycle
            .record_activity_resumed(&token)
            .map_err(|e| Status::from(e))
    }

    fn activity_paused(&self, token: SpIBinder) -> AidlResult<()> {
        self.lifecycle
            .record_activity_paused(&token)
            .map_err(|e| Status::from(e))
    }

    fn activity_stopped(&self, token: SpIBinder, _state: Option<&[u8]>) -> AidlResult<()> {
        self.lifecycle
            .record_activity_stopped(&token)
            .map_err(|e| Status::from(e))
    }

    fn finish_activity(
        &self,
        token: SpIBinder,
        _result_code: i32,
        _result_data: Option<&Intent>,
        _finish_task: i32,
    ) -> AidlResult<bool> {
        let act_arc = match self.lifecycle.find_activity(&token) {
            Some(a) => a,
            None => return Ok(false),
        };

        let pid_opt = {
            let act = act_arc.read().unwrap();
            act.pid
        };

        if let Some(pid) = pid_opt {
            let thread_opt = {
                let threads = self.process_threads.read().unwrap();
                threads.get(&pid).cloned()
            };
            if let Some(thread) = thread_opt {
                let _ = thread.schedule_destroy_activity(&token, true);
            }
        }

        self.lifecycle
            .finish_activity(&token)
            .map_err(|e| Status::from(e))
    }
}

// -----------------------------------------------------------------------------
// Remotable Server Dispatcher for ActivityManagerService
// -----------------------------------------------------------------------------

impl Remotable for ActivityManagerService {
    fn get_class_descriptor() -> &'static str {
        IACTIVITY_MANAGER_DESCRIPTOR
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
            iactivity_manager_codes::START_ACTIVITY => {
                // Read calling package
                let has_pkg = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let calling_pkg = if has_pkg {
                    data.read_utf8(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                } else {
                    None
                };

                // Read Intent
                let mut intent = Intent::default();
                intent.read_from_parcel_at(data, &mut offset)?;

                // Read resolvedType
                let has_res_type = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let resolved_type = if has_res_type {
                    data.read_utf8(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                } else {
                    None
                };

                // Read resultWho
                let has_result_who = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let result_who = if has_result_who {
                    data.read_utf8(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                } else {
                    None
                };

                // Read requestCode & flags
                let request_code = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let flags = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.start_activity(
                    None,
                    calling_pkg,
                    &intent,
                    resolved_type.as_deref(),
                    None,
                    result_who.as_deref(),
                    request_code,
                    flags,
                    None,
                    None,
                )?;

                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(res)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iactivity_manager_codes::ATTACH_APPLICATION => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let start_seq = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let thread = ActivityTokenBinder::new(token_id);

                self.attach_application(thread, start_seq)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iactivity_manager_codes::ACTIVITY_RESUMED => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.activity_resumed(token)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iactivity_manager_codes::ACTIVITY_PAUSED => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.activity_paused(token)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iactivity_manager_codes::ACTIVITY_STOPPED => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);
                self.activity_stopped(token, None)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iactivity_manager_codes::FINISH_ACTIVITY => {
                let token_id = data
                    .read_u32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let result_code = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let finish_task = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let token = ActivityTokenBinder::new(token_id);

                let finished = self.finish_activity(token, result_code, None, finish_task)?;
                reply
                    .write_status(&Status::ok())
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_bool(finished)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for ActivityManagerService {
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
                    .write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
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
        Some(IACTIVITY_MANAGER_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// ActivityManagerClient (Proxy)
// -----------------------------------------------------------------------------

pub struct ActivityManagerClient {
    binder: SpIBinder,
}

impl ActivityManagerClient {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Proxy for ActivityManagerClient {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl Interface for ActivityManagerClient {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl IActivityManager for ActivityManagerClient {
    fn start_activity(
        &self,
        _caller: Option<SpIBinder>,
        calling_package: Option<String>,
        intent: &Intent,
        resolved_type: Option<&str>,
        _result_to: Option<SpIBinder>,
        result_who: Option<&str>,
        request_code: i32,
        flags: i32,
        _profiler_info: Option<&str>,
        _options: Option<&[u8]>,
    ) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(ref pkg) = calling_package {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            data.write_utf8(Some(pkg))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        intent.write_to_parcel(&mut data)?;

        if let Some(r_type) = resolved_type {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            data.write_utf8(Some(r_type))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        if let Some(r_who) = result_who {
            data.write_bool(true)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
            data.write_utf8(Some(r_who))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_bool(false)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        data.write_i32(request_code)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::START_ACTIVITY,
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

    fn attach_application(&self, thread: SpIBinder, start_seq: i64) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let token_id = thread.handle().unwrap_or(1);
        data.write_u32(token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(start_seq)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::ATTACH_APPLICATION,
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

    fn activity_resumed(&self, token: SpIBinder) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let token_id = token.handle().unwrap_or(1);
        data.write_u32(token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::ACTIVITY_RESUMED,
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

    fn activity_paused(&self, token: SpIBinder) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let token_id = token.handle().unwrap_or(1);
        data.write_u32(token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::ACTIVITY_PAUSED,
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

    fn activity_stopped(&self, token: SpIBinder, _state: Option<&[u8]>) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let token_id = token.handle().unwrap_or(1);
        data.write_u32(token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::ACTIVITY_STOPPED,
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

    fn finish_activity(
        &self,
        token: SpIBinder,
        result_code: i32,
        _result_data: Option<&Intent>,
        finish_task: i32,
    ) -> AidlResult<bool> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IACTIVITY_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let token_id = token.handle().unwrap_or(1);
        data.write_u32(token_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(result_code)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(finish_task)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            iactivity_manager_codes::FINISH_ACTIVITY,
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

        let finished = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(finished)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `ActivityManagerService` with handle 0 ServiceManager as `"activity"`.
pub fn register_activity_service(service: Arc<ActivityManagerService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service("activity", SpIBinder::from_arc(binder))
}
