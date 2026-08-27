//! Per-thread looper state machine and kernel driver communication engine.
//!
//! Implements `talk_with_driver`, transaction encoding/decoding, looper loops,
//! and the spawn-before-block concurrency mechanism.

use crate::process_state::ProcessState;
use crate::sys::*;
use aidl_compat::status::{
    Result, Status, STATUS_DEAD_OBJECT, STATUS_FAILED_TRANSACTION, STATUS_UNKNOWN_TRANSACTION,
};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use bytemuck::{bytes_of, pod_read_unaligned};
use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static CURRENT_THREAD_STATE: RefCell<Option<IPCThreadState>> = const { RefCell::new(None) };
    static IS_SERVING_TRANSACTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CALLING_PID: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    static CALLING_UID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Per-thread Binder IPC state.
pub struct IPCThreadState {
    process: Arc<ProcessState>,
    write_buf: Vec<u8>,
    read_buf: Vec<u8>,
    is_serving_transaction: bool,
    is_looper_active: bool,
    calling_pid: i32,
    calling_uid: u32,
}

impl IPCThreadState {
    /// Create new thread state bound to a `ProcessState`.
    pub fn with_process(process: Arc<ProcessState>) -> Self {
        Self {
            process,
            write_buf: Vec::with_capacity(512),
            read_buf: vec![0u8; 1024],
            is_serving_transaction: false,
            is_looper_active: false,
            calling_pid: 0,
            calling_uid: 0,
        }
    }

    /// Access or initialize thread-local `IPCThreadState` bound to a specific `ProcessState`.
    pub fn current_with_process<F, R>(process: &Arc<ProcessState>, f: F) -> R
    where
        F: FnOnce(&mut IPCThreadState) -> R,
    {
        CURRENT_THREAD_STATE.with(|cell| {
            let mut opt = cell.borrow_mut();
            let need_init = match opt.as_ref() {
                Some(state) => state.process.pid() != process.pid(),
                None => true,
            };
            if need_init {
                *opt = Some(Self::with_process(Arc::clone(process)));
            }
            let state = opt.as_mut().unwrap();
            f(state)
        })
    }

    /// Access thread-local `IPCThreadState` bound to singleton `ProcessState::self_or_init()`.
    pub fn current<F, R>(f: F) -> R
    where
        F: FnOnce(&mut IPCThreadState) -> R,
    {
        let ps = ProcessState::self_or_init();
        Self::current_with_process(&ps, f)
    }

    /// Return calling process ID for currently serviced incoming transaction.
    pub fn calling_pid(&self) -> i32 {
        if self.calling_pid != 0 {
            self.calling_pid
        } else {
            CALLING_PID.with(|c| c.get())
        }
    }

    /// Return calling effective UID for currently serviced incoming transaction.
    pub fn calling_uid(&self) -> u32 {
        if self.calling_uid != 0 {
            self.calling_uid
        } else {
            CALLING_UID.with(|c| c.get())
        }
    }

    /// Return true if this thread is currently serving an incoming transaction.
    pub fn is_serving_transaction(&self) -> bool {
        self.is_serving_transaction || IS_SERVING_TRANSACTION.with(|c| c.get())
    }

    /// Send and receive raw buffers with Binder driver.
    pub fn talk_with_driver(&mut self, do_receive: bool) -> Result<usize> {
        let mut bwr = binder_write_read::new();

        if !self.write_buf.is_empty() {
            bwr.write_buffer = self.write_buf.as_ptr() as u64;
            bwr.write_size = self.write_buf.len() as u64;
        }

        if do_receive {
            bwr.read_buffer = self.read_buf.as_mut_ptr() as u64;
            bwr.read_size = self.read_buf.len() as u64;
        }

        self.process
            .driver()
            .write_read(&mut bwr)
            .map_err(|_e| Status::from_status(STATUS_FAILED_TRANSACTION))?;

        if bwr.write_consumed > 0 {
            let consumed = bwr.write_consumed as usize;
            if consumed >= self.write_buf.len() {
                self.write_buf.clear();
            } else {
                self.write_buf.drain(0..consumed);
            }
        }

        Ok(bwr.read_consumed as usize)
    }

    /// Send PING_TRANSACTION to a given handle to check if remote binder is alive.
    pub fn ping(&mut self, handle: u32) -> Result<()> {
        let data = Parcel::new();
        let mut reply = Parcel::new();
        self.transact(handle, PING_TRANSACTION, 0, &data, &mut reply)
    }

    /// Perform a synchronous or asynchronous Binder transaction across the driver.
    pub fn transact(
        &mut self,
        handle: u32,
        code: TransactionCode,
        flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        let is_oneway = (flags & TF_ONE_WAY) != 0;

        // --- SPAWN-BEFORE-BLOCK CONCURRENCY HANDLING ---
        // If this worker thread is currently serving an incoming transaction and is about
        // to block on a nested synchronous Binder call, spawn a replacement thread
        // to prevent deadlock under concurrent re-entrant app load!
        if (self.is_serving_transaction || IS_SERVING_TRANSACTION.with(|c| c.get())) && !is_oneway {
            self.process.spawn_worker_thread_if_needed();
        }

        let data_slice = data.data();
        let offsets_slice = data.offsets();

        let tr = BinderTransactionData::new(
            handle as u64,
            0,
            code,
            flags,
            0,
            0,
            data_slice.len() as u64,
            (offsets_slice.len() * 8) as u64,
            if !data_slice.is_empty() {
                data_slice.as_ptr() as u64
            } else {
                0
            },
            if !offsets_slice.is_empty() {
                offsets_slice.as_ptr() as u64
            } else {
                0
            },
        );

        self.write_buf.extend_from_slice(&BC_TRANSACTION.to_ne_bytes());
        self.write_buf.extend_from_slice(bytes_of(&tr));

        if is_oneway {
            let _ = self.talk_with_driver(false)?;
            return Ok(());
        }

        // Synchronous call: loop until we receive BR_REPLY
        loop {
            let read_consumed = self.talk_with_driver(true)?;
            let read_copy = self.read_buf[..read_consumed].to_vec();
            let mut read_data = &read_copy[..];

            while read_data.len() >= 4 {
                let mut cmd_bytes = [0u8; 4];
                cmd_bytes.copy_from_slice(&read_data[..4]);
                let cmd = u32::from_ne_bytes(cmd_bytes);
                read_data = &read_data[4..];

                match cmd {
                    BR_REPLY => {
                        let tr_size = std::mem::size_of::<BinderTransactionData>();
                        if read_data.len() < tr_size {
                            return Err(Status::from_status(STATUS_FAILED_TRANSACTION));
                        }
                        let reply_tr = BinderTransactionData::from_bytes(&read_data[..tr_size])
                            .ok_or_else(|| Status::from_status(STATUS_FAILED_TRANSACTION))?;

                        // Unpack reply payload from mmap region
                        if reply_tr.data_size > 0 && reply_tr.data_buffer != 0 {
                            let payload = self
                                .process
                                .mmap_region()
                                .read_bytes(reply_tr.data_buffer, reply_tr.data_size as usize)
                                .map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;

                            let reply_parcel = Parcel::from_slice(&payload);

                            // Emit BC_FREE_BUFFER
                            self.write_buf
                                .extend_from_slice(&BC_FREE_BUFFER.to_ne_bytes());
                            self.write_buf
                                .extend_from_slice(&reply_tr.data_buffer.to_ne_bytes());
                            let _ = self.talk_with_driver(false);

                            if (reply_tr.flags & TF_STATUS_CODE) != 0 {
                                let mut offset = 0;
                                let raw_status = reply_parcel
                                    .read_i32(&mut offset)
                                    .unwrap_or(STATUS_FAILED_TRANSACTION);
                                return Err(Status::from_status(raw_status));
                            }

                            *reply = reply_parcel;
                        } else {
                            *reply = Parcel::new();
                            if (reply_tr.flags & TF_STATUS_CODE) != 0 {
                                return Err(Status::from_status(STATUS_FAILED_TRANSACTION));
                            }
                        }

                        return Ok(());
                    }
                    BR_FAILED_REPLY => {
                        return Err(Status::from_status(STATUS_FAILED_TRANSACTION));
                    }
                    BR_DEAD_REPLY => {
                        return Err(Status::from_status(STATUS_DEAD_OBJECT));
                    }
                    BR_TRANSACTION_COMPLETE => {
                        // Handshake acknowledgment, continue loop
                    }
                    _ => {
                        self.execute_command(cmd, &mut read_data)?;
                    }
                }
            }
        }
    }

    /// Read incoming driver buffer and process all pending commands.
    pub fn process_pending_commands(&mut self) -> Result<()> {
        let read_consumed = self.talk_with_driver(true)?;
        let read_copy = self.read_buf[..read_consumed].to_vec();
        let mut read_data = &read_copy[..];

        while read_data.len() >= 4 {
            let mut cmd_bytes = [0u8; 4];
            cmd_bytes.copy_from_slice(&read_data[..4]);
            let cmd = u32::from_ne_bytes(cmd_bytes);
            read_data = &read_data[4..];

            if cmd == BR_FINISHED {
                self.is_looper_active = false;
                break;
            }

            self.execute_command(cmd, &mut read_data)?;
        }
        Ok(())
    }

    /// Enter looper as the main thread.
    pub fn enter_looper(&mut self) -> Result<()> {
        self.write_buf
            .extend_from_slice(&BC_ENTER_LOOPER.to_ne_bytes());
        let _ = self.talk_with_driver(false)?;
        self.is_looper_active = true;
        self.run_looper()
    }

    /// Join thread pool as a worker thread.
    pub fn join_thread_pool(&mut self) -> Result<()> {
        self.write_buf
            .extend_from_slice(&BC_REGISTER_LOOPER.to_ne_bytes());
        let _ = self.talk_with_driver(false)?;
        self.is_looper_active = true;
        self.run_looper()
    }

    /// Execute looper loop until exit.
    fn run_looper(&mut self) -> Result<()> {
        while self.is_looper_active {
            self.process_pending_commands()?;
        }

        // Thread exit ioctl
        let _ = self.process.driver().thread_exit();
        Ok(())
    }

    /// Execute a single driver return command.
    fn execute_command(&mut self, cmd: u32, read_cursor: &mut &[u8]) -> Result<()> {
        match cmd {
            BR_TRANSACTION => {
                let tr_size = std::mem::size_of::<BinderTransactionData>();
                if read_cursor.len() < tr_size {
                    return Err(Status::from_status(STATUS_FAILED_TRANSACTION));
                }
                let tr = BinderTransactionData::from_bytes(&read_cursor[..tr_size])
                    .ok_or_else(|| Status::from_status(STATUS_FAILED_TRANSACTION))?;
                *read_cursor = &read_cursor[tr_size..];

                let _ = self.handle_incoming_transaction(tr);
            }
            BR_SPAWN_LOOPER => {
                self.process.spawn_worker_thread_if_needed();
            }
            BR_DEAD_BINDER => {
                if read_cursor.len() >= 8 {
                    let mut c_bytes = [0u8; 8];
                    c_bytes.copy_from_slice(&read_cursor[..8]);
                    let cookie = u64::from_ne_bytes(c_bytes);
                    *read_cursor = &read_cursor[8..];

                    self.process.notify_death(cookie);

                    // Send BC_DEAD_BINDER_DONE
                    self.write_buf
                        .extend_from_slice(&BC_DEAD_BINDER_DONE.to_ne_bytes());
                    self.write_buf.extend_from_slice(&cookie.to_ne_bytes());
                    let _ = self.talk_with_driver(false);
                }
            }
            BR_CLEAR_DEATH_NOTIFICATION_DONE => {
                if read_cursor.len() >= 8 {
                    *read_cursor = &read_cursor[8..];
                }
            }
            BR_ACQUIRE | BR_INCREFS => {
                if read_cursor.len() >= 16 {
                    let ptr_cookie = pod_read_unaligned::<BinderPtrCookie>(&read_cursor[..16]);
                    *read_cursor = &read_cursor[16..];

                    let done_cmd = if cmd == BR_ACQUIRE {
                        BC_ACQUIRE_DONE
                    } else {
                        BC_INCREFS_DONE
                    };
                    self.write_buf.extend_from_slice(&done_cmd.to_ne_bytes());
                    self.write_buf.extend_from_slice(bytes_of(&ptr_cookie));
                    let _ = self.talk_with_driver(false);
                }
            }
            BR_RELEASE | BR_DECREFS => {
                if read_cursor.len() >= 16 {
                    *read_cursor = &read_cursor[16..];
                }
            }
            BR_NOOP | BR_OK | BR_TRANSACTION_COMPLETE => {}
            _ => {
                log::warn!("IPCThreadState: unhandled BR command: 0x{:x}", cmd);
            }
        }

        Ok(())
    }

    /// Process incoming `BR_TRANSACTION` from driver and send `BC_REPLY`.
    fn handle_incoming_transaction(&mut self, tr: BinderTransactionData) -> Result<()> {
        let prev_serving = self.is_serving_transaction;
        let prev_pid = self.calling_pid;
        let prev_uid = self.calling_uid;
        let prev_tls_serving = IS_SERVING_TRANSACTION.with(|c| c.get());
        let prev_tls_pid = CALLING_PID.with(|c| c.get());
        let prev_tls_uid = CALLING_UID.with(|c| c.get());

        self.is_serving_transaction = true;
        self.calling_pid = tr.sender_pid;
        self.calling_uid = tr.sender_euid;

        IS_SERVING_TRANSACTION.with(|c| c.set(true));
        CALLING_PID.with(|c| c.set(tr.sender_pid));
        CALLING_UID.with(|c| c.set(tr.sender_euid));

        // Unpack incoming data payload from mmap
        let mut data_parcel = Parcel::new();
        if tr.data_size > 0 && tr.data_buffer != 0 {
            if let Ok(bytes) = self
                .process
                .mmap_region()
                .read_bytes(tr.data_buffer, tr.data_size as usize)
            {
                data_parcel = Parcel::from_slice(&bytes);
            }
        }

        // Find service stub
        let target_stub = self
            .process
            .get_service_object(tr.target)
            .or_else(|| self.process.get_service_object(tr.cookie));

        let mut reply_parcel = Parcel::new();
        let transact_res = if let Some(stub) = target_stub {
            stub.transact(tr.code, tr.flags, &data_parcel, &mut reply_parcel)
        } else {
            Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION))
        };

        self.is_serving_transaction = prev_serving;
        self.calling_pid = prev_pid;
        self.calling_uid = prev_uid;

        IS_SERVING_TRANSACTION.with(|c| c.set(prev_tls_serving));
        CALLING_PID.with(|c| c.set(prev_tls_pid));
        CALLING_UID.with(|c| c.set(prev_tls_uid));

        // Free incoming transaction buffer in driver
        if tr.data_buffer != 0 {
            self.write_buf
                .extend_from_slice(&BC_FREE_BUFFER.to_ne_bytes());
            self.write_buf
                .extend_from_slice(&tr.data_buffer.to_ne_bytes());
        }

        // If not one-way, send BC_REPLY
        if !tr.is_one_way() {
            let (reply_parcel, reply_flags) = match &transact_res {
                Ok(()) => (reply_parcel, 0u32),
                Err(status) => {
                    let mut err_parcel = Parcel::new();
                    let code = status.status_code() as i32;
                    let _ = err_parcel.write_i32(code);
                    (err_parcel, TF_STATUS_CODE)
                }
            };

            let reply_slice = reply_parcel.data();
            let reply_offsets = reply_parcel.offsets();

            let reply_tr = BinderTransactionData::new(
                0,
                tr.cookie, // Correlating transaction ID / cookie
                0,
                reply_flags,
                0,
                0,
                reply_slice.len() as u64,
                (reply_offsets.len() * 8) as u64,
                if !reply_slice.is_empty() {
                    reply_slice.as_ptr() as u64
                } else {
                    0
                },
                if !reply_offsets.is_empty() {
                    reply_offsets.as_ptr() as u64
                } else {
                    0
                },
            );

            self.write_buf.extend_from_slice(&BC_REPLY.to_ne_bytes());
            self.write_buf.extend_from_slice(bytes_of(&reply_tr));

            // Flush write buffer while reply_parcel data is still alive
            let _ = self.talk_with_driver(false);
        } else {
            // Flush write buffer (e.g. BC_FREE_BUFFER)
            let _ = self.talk_with_driver(false);
        }

        transact_res
    }
}
