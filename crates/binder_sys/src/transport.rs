//! `RemoteTransport` implementation connecting `aidl_compat` proxies with kernel `binder_sys`.

use crate::ipc_thread_state::IPCThreadState;
use crate::process_state::ProcessState;
use aidl_compat::status::Result;
use aidl_compat::stub::RemoteTransport;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use std::sync::Arc;

/// Kernel Binder Userspace Transport implementing `aidl_compat::RemoteTransport`.
pub struct BinderKernelTransport {
    process: Option<Arc<ProcessState>>,
}

impl BinderKernelTransport {
    /// Construct default transport bound to global singleton `ProcessState`.
    pub fn new() -> Self {
        Self { process: None }
    }

    /// Construct transport bound to specific `ProcessState` instance.
    pub fn with_process(process: Arc<ProcessState>) -> Self {
        Self {
            process: Some(process),
        }
    }
}

impl Default for BinderKernelTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteTransport for BinderKernelTransport {
    fn transact(
        &self,
        handle: u32,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        if let Some(proc) = &self.process {
            IPCThreadState::current_with_process(proc, |state| {
                state.transact(handle, code, flags, data, reply)
            })
        } else {
            IPCThreadState::current(|state| state.transact(handle, code, flags, data, reply))
        }
    }
}
