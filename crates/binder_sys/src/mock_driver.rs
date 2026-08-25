//! High-fidelity in-memory simulated `/dev/binder` driver for cross-platform execution.
//!
//! Simulates kernel Binder ioctl commands (`BINDER_WRITE_READ`, `BINDER_SET_MAX_THREADS`,
//! `BINDER_VERSION`, `BINDER_THREAD_EXIT`), mmap buffer management, handle table routing,
//! looper dispatch, threadpool expansion (`BR_SPAWN_LOOPER`), and death notifications.

use crate::mmap::BinderMmapRegion;
use crate::sys::*;
use bytemuck::{bytes_of, pod_read_unaligned};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DriverError {
    #[error("I/O error during Binder ioctl: {0}")]
    IoError(String),
    #[error("Invalid Binder command opcode: 0x{0:x}")]
    InvalidCommand(u32),
    #[error("Target handle {0} not found")]
    HandleNotFound(u32),
    #[error("Transaction buffer memory allocation failed")]
    NoMemory,
    #[error("Dead object error")]
    DeadObject,
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Timeout waiting for Binder reply")]
    Timeout,
}

/// Transaction pending reply coordination.
struct PendingReply {
    ready: Mutex<bool>,
    cond: Condvar,
    reply_tr: Mutex<Option<BinderTransactionData>>,
}

impl PendingReply {
    fn new() -> Self {
        Self {
            ready: Mutex::new(false),
            cond: Condvar::new(),
            reply_tr: Mutex::new(None),
        }
    }

    fn wait_reply(&self) -> Result<BinderTransactionData, DriverError> {
        let mut ready = self.ready.lock().unwrap();
        while !*ready {
            ready = self
                .cond
                .wait(ready)
                .map_err(|e| DriverError::IoError(e.to_string()))?;
        }
        let tr = self.reply_tr.lock().unwrap().take();
        tr.ok_or(DriverError::DeadObject)
    }

    fn set_reply(&self, tr: BinderTransactionData) {
        {
            let mut reply = self.reply_tr.lock().unwrap();
            *reply = Some(tr);
            let mut ready = self.ready.lock().unwrap();
            *ready = true;
        }
        self.cond.notify_all();
    }
}

/// Handle descriptor pointing to a target process and object.
#[derive(Debug, Clone)]
pub struct RegisteredHandle {
    pub target_pid: u32,
    pub target_ptr: u64,
    pub target_cookie: u64,
    pub strong_refs: u32,
}

/// Death notification registration.
#[derive(Debug, Clone)]
struct DeathWatcher {
    #[allow(dead_code)]
    watcher_pid: u32,
    handle: u32,
    cookie: u64,
}

/// State of a single client process connected to the simulated Binder driver.
pub struct MockClientProcess {
    pub pid: u32,
    pub mmap_region: Arc<BinderMmapRegion>,
    pub max_threads: AtomicU32,
    pub registered_loopers: AtomicU32,
    pub waiting_loopers: AtomicU32,
    pub looper_active: AtomicBool,

    // Command queues for asynchronous and incoming transactions
    read_queue: Mutex<VecDeque<Vec<u8>>>,
    queue_cond: Condvar,

    // Active synchronous transaction waiters (key: transaction id / call sequence)
    pending_replies: Mutex<HashMap<u64, Arc<PendingReply>>>,

    // Registered handles table for this process: handle ID -> RegisteredHandle
    handle_table: Mutex<HashMap<u32, RegisteredHandle>>,

    // Allocated buffers mapped in this process (ptr -> size)
    allocated_buffers: Mutex<HashMap<u64, usize>>,

    // Death notifications registered by this client
    death_watchers: Mutex<Vec<DeathWatcher>>,
}

impl MockClientProcess {
    pub fn new(pid: u32, mmap_region: Arc<BinderMmapRegion>) -> Self {
        Self {
            pid,
            mmap_region,
            max_threads: AtomicU32::new(15),
            registered_loopers: AtomicU32::new(0),
            waiting_loopers: AtomicU32::new(0),
            looper_active: AtomicBool::new(false),
            read_queue: Mutex::new(VecDeque::new()),
            queue_cond: Condvar::new(),
            pending_replies: Mutex::new(HashMap::new()),
            handle_table: Mutex::new(HashMap::new()),
            allocated_buffers: Mutex::new(HashMap::new()),
            death_watchers: Mutex::new(Vec::new()),
        }
    }

    /// Push command bytes into client read queue and wake waiting loopers.
    pub fn push_read_command(&self, cmd_bytes: &[u8]) {
        {
            let mut q = self.read_queue.lock().unwrap();
            q.push_back(cmd_bytes.to_vec());
        }
        self.queue_cond.notify_one();
    }
}

/// In-memory simulated Linux Kernel Binder Driver.
pub struct MockBinderDriver {
    clients: Mutex<HashMap<u32, Arc<MockClientProcess>>>,
    context_manager: Mutex<Option<(u32, u64, u64)>>, // (pid, ptr, cookie)
    next_pid: AtomicU32,
    next_handle: AtomicU32,
    next_tx_id: AtomicU64,
}

impl Default for MockBinderDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBinderDriver {
    /// Create new mock driver instance.
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            context_manager: Mutex::new(None),
            next_pid: AtomicU32::new(100),
            next_handle: AtomicU32::new(1),
            next_tx_id: AtomicU64::new(1),
        }
    }

    /// Register a new client process with the driver.
    pub fn register_client(&self, mmap_size: usize) -> Arc<MockClientProcess> {
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
        let mmap = BinderMmapRegion::new_simulated(mmap_size);
        let client = Arc::new(MockClientProcess::new(pid, mmap));
        let mut clients = self.clients.lock().unwrap();
        clients.insert(pid, Arc::clone(&client));
        client
    }

    /// Set context manager (handle 0 / ServiceManager).
    pub fn set_context_manager(&self, pid: u32, ptr: u64, cookie: u64) {
        let mut cm = self.context_manager.lock().unwrap();
        *cm = Some((pid, ptr, cookie));
    }

    /// Get client process by PID.
    pub fn get_client(&self, pid: u32) -> Option<Arc<MockClientProcess>> {
        self.clients.lock().unwrap().get(&pid).cloned()
    }

    /// Generate unique transaction identifier.
    pub fn next_transaction_id(&self) -> u64 {
        self.next_tx_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Process `ioctl(BINDER_SET_MAX_THREADS)`
    pub fn set_max_threads(&self, client: &MockClientProcess, max: u32) -> Result<(), DriverError> {
        client.max_threads.store(max, Ordering::SeqCst);
        Ok(())
    }

    /// Process `ioctl(BINDER_VERSION)`
    pub fn get_version(&self) -> Result<i32, DriverError> {
        Ok(BINDER_CURRENT_PROTOCOL_VERSION)
    }

    /// Process `ioctl(BINDER_WRITE_READ)`
    pub fn write_read(
        &self,
        client: &MockClientProcess,
        bwr: &mut binder_write_read,
    ) -> Result<(), DriverError> {
        let mut sync_reply_tr: Option<BinderTransactionData> = None;

        // 1. Process userspace write buffer if write_size > 0
        if bwr.write_size > 0 && bwr.write_buffer != 0 {
            let write_data = unsafe {
                std::slice::from_raw_parts(
                    bwr.write_buffer as *const u8,
                    bwr.write_size as usize,
                )
            };
            let (consumed, reply_opt) = self.process_write_buffer(client, write_data)?;
            bwr.write_consumed = consumed as u64;
            sync_reply_tr = reply_opt;
        }

        // 2. Fill userspace read buffer if read_size > 0
        if bwr.read_size > 0 && bwr.read_buffer != 0 {
            let read_dest = unsafe {
                std::slice::from_raw_parts_mut(
                    bwr.read_buffer as *mut u8,
                    bwr.read_size as usize,
                )
            };

            if let Some(reply_tr) = sync_reply_tr {
                let mut written = 0;
                let tr_size = std::mem::size_of::<BinderTransactionData>();
                if read_dest.len() >= 4 + tr_size {
                    read_dest[..4].copy_from_slice(&BR_REPLY.to_ne_bytes());
                    read_dest[4..4 + tr_size].copy_from_slice(bytes_of(&reply_tr));
                    written = 4 + tr_size;
                }
                bwr.read_consumed = written as u64;
            } else {
                let read_bytes = self.fill_read_buffer(client, read_dest, bwr.write_size == 0)?;
                bwr.read_consumed = read_bytes as u64;
            }
        }

        Ok(())
    }

    /// Parse commands from write buffer.
    fn process_write_buffer(
        &self,
        client: &MockClientProcess,
        mut buffer: &[u8],
    ) -> Result<(usize, Option<BinderTransactionData>), DriverError> {
        let original_len = buffer.len();
        let mut sync_reply_out: Option<BinderTransactionData> = None;

        while buffer.len() >= 4 {
            let mut cmd_bytes = [0u8; 4];
            cmd_bytes.copy_from_slice(&buffer[..4]);
            let cmd = u32::from_ne_bytes(cmd_bytes);
            buffer = &buffer[4..];

            match cmd {
                BC_ENTER_LOOPER => {
                    client.registered_loopers.fetch_add(1, Ordering::SeqCst);
                    client.looper_active.store(true, Ordering::SeqCst);
                }
                BC_REGISTER_LOOPER => {
                    client.registered_loopers.fetch_add(1, Ordering::SeqCst);
                }
                BC_EXIT_LOOPER => {
                    client.registered_loopers.fetch_sub(1, Ordering::SeqCst);
                }
                BC_FREE_BUFFER => {
                    if buffer.len() < 8 {
                        break;
                    }
                    let mut ptr_bytes = [0u8; 8];
                    ptr_bytes.copy_from_slice(&buffer[..8]);
                    let ptr = u64::from_ne_bytes(ptr_bytes);
                    buffer = &buffer[8..];

                    let size_opt = {
                        let mut allocs = client.allocated_buffers.lock().unwrap();
                        allocs.remove(&ptr)
                    };
                    if let Some(size) = size_opt {
                        let _ = client.mmap_region.free_buffer(ptr, size);
                    }
                }
                BC_TRANSACTION => {
                    let tr_size = std::mem::size_of::<BinderTransactionData>();
                    if buffer.len() < tr_size {
                        return Err(DriverError::InvalidArgument(
                            "Incomplete BinderTransactionData in write buffer".into(),
                        ));
                    }
                    let tr = BinderTransactionData::from_bytes(&buffer[..tr_size]).ok_or_else(
                        || DriverError::InvalidArgument("Invalid BinderTransactionData".into()),
                    )?;
                    buffer = &buffer[tr_size..];

                    let reply_opt = self.handle_bc_transaction(client, tr)?;
                    if reply_opt.is_some() {
                        sync_reply_out = reply_opt;
                    }
                }
                BC_REPLY => {
                    let tr_size = std::mem::size_of::<BinderTransactionData>();
                    if buffer.len() < tr_size {
                        return Err(DriverError::InvalidArgument(
                            "Incomplete BinderTransactionData in BC_REPLY".into(),
                        ));
                    }
                    let tr = BinderTransactionData::from_bytes(&buffer[..tr_size]).ok_or_else(
                        || DriverError::InvalidArgument("Invalid BinderTransactionData".into()),
                    )?;
                    buffer = &buffer[tr_size..];

                    self.handle_bc_reply(client, tr)?;
                }
                BC_ACQUIRE | BC_INCREFS => {
                    if buffer.len() < 4 {
                        break;
                    }
                    let mut h_bytes = [0u8; 4];
                    h_bytes.copy_from_slice(&buffer[..4]);
                    let handle = u32::from_ne_bytes(h_bytes);
                    buffer = &buffer[4..];

                    let mut ht = client.handle_table.lock().unwrap();
                    if let Some(reg) = ht.get_mut(&handle) {
                        reg.strong_refs += 1;
                    }
                }
                BC_RELEASE | BC_DECREFS => {
                    if buffer.len() < 4 {
                        break;
                    }
                    let mut h_bytes = [0u8; 4];
                    h_bytes.copy_from_slice(&buffer[..4]);
                    let handle = u32::from_ne_bytes(h_bytes);
                    buffer = &buffer[4..];

                    let mut ht = client.handle_table.lock().unwrap();
                    if let Some(reg) = ht.get_mut(&handle) {
                        if reg.strong_refs > 0 {
                            reg.strong_refs -= 1;
                        }
                    }
                }
                BC_REQUEST_DEATH_NOTIFICATION => {
                    if buffer.len() < 16 {
                        break;
                    }
                    let hc = pod_read_unaligned::<BinderHandleCookie>(&buffer[..16]);
                    buffer = &buffer[16..];

                    let mut watchers = client.death_watchers.lock().unwrap();
                    watchers.push(DeathWatcher {
                        watcher_pid: client.pid,
                        handle: hc.handle,
                        cookie: hc.cookie,
                    });
                }
                BC_CLEAR_DEATH_NOTIFICATION => {
                    if buffer.len() < 16 {
                        break;
                    }
                    let hc = pod_read_unaligned::<BinderHandleCookie>(&buffer[..16]);
                    buffer = &buffer[16..];

                    let mut watchers = client.death_watchers.lock().unwrap();
                    watchers.retain(|w| w.cookie != hc.cookie);

                    // Push BR_CLEAR_DEATH_NOTIFICATION_DONE
                    let mut resp = Vec::with_capacity(12);
                    resp.extend_from_slice(&BR_CLEAR_DEATH_NOTIFICATION_DONE.to_ne_bytes());
                    resp.extend_from_slice(&hc.cookie.to_ne_bytes());
                    client.push_read_command(&resp);
                }
                BC_DEAD_BINDER_DONE => {
                    if buffer.len() < 8 {
                        break;
                    }
                    let _cookie = u64::from_ne_bytes(buffer[..8].try_into().unwrap());
                    buffer = &buffer[8..];
                }
                BC_ACQUIRE_DONE | BC_INCREFS_DONE => {
                    if buffer.len() < 16 {
                        break;
                    }
                    buffer = &buffer[16..];
                }
                _ => {
                    log::warn!("MockDriver: unhandled BC command: 0x{:x}", cmd);
                }
            }
        }

        Ok((original_len - buffer.len(), sync_reply_out))
    }

    /// Handle `BC_TRANSACTION` from sender client.
    fn handle_bc_transaction(
        &self,
        sender: &MockClientProcess,
        mut tr: BinderTransactionData,
    ) -> Result<Option<BinderTransactionData>, DriverError> {
        let (target_pid, target_ptr, target_cookie) = self.resolve_target(sender, &tr)?;

        let target_client = self
            .get_client(target_pid)
            .ok_or(DriverError::DeadObject)?;

        // Read sender's payload bytes
        let payload_bytes = if tr.data_size > 0 && tr.data_buffer != 0 {
            unsafe {
                std::slice::from_raw_parts(
                    tr.data_buffer as *const u8,
                    tr.data_size as usize,
                )
            }
            .to_vec()
        } else {
            Vec::new()
        };

        // Read sender's offsets bytes
        let offsets_bytes = if tr.offsets_size > 0 && tr.offsets_buffer != 0 {
            unsafe {
                std::slice::from_raw_parts(
                    tr.offsets_buffer as *const u8,
                    tr.offsets_size as usize,
                )
            }
            .to_vec()
        } else {
            Vec::new()
        };

        // Allocate buffer in target process's mmap region
        let target_data_ptr = if !payload_bytes.is_empty() {
            let ptr = target_client
                .mmap_region
                .allocate_buffer(payload_bytes.len())
                .map_err(|_| DriverError::NoMemory)?;
            target_client
                .mmap_region
                .write_bytes(ptr, &payload_bytes)
                .map_err(|_| DriverError::NoMemory)?;
            let mut allocs = target_client.allocated_buffers.lock().unwrap();
            allocs.insert(ptr, payload_bytes.len());
            ptr
        } else {
            0
        };

        let target_offsets_ptr = if !offsets_bytes.is_empty() {
            let ptr = target_client
                .mmap_region
                .allocate_buffer(offsets_bytes.len())
                .map_err(|_| DriverError::NoMemory)?;
            target_client
                .mmap_region
                .write_bytes(ptr, &offsets_bytes)
                .map_err(|_| DriverError::NoMemory)?;
            let mut allocs = target_client.allocated_buffers.lock().unwrap();
            allocs.insert(ptr, offsets_bytes.len());
            ptr
        } else {
            0
        };

        let tx_id = self.next_transaction_id();

        // Update target transaction descriptor
        tr.target = if target_ptr != 0 { target_ptr } else { target_cookie };
        tr.cookie = tx_id;
        tr.sender_pid = sender.pid as i32;
        tr.sender_euid = 1000;
        tr.data_buffer = target_data_ptr;
        tr.offsets_buffer = target_offsets_ptr;

        let is_oneway = tr.is_one_way();

        // If synchronous, register pending reply channel using tx_id as tracking
        let reply_chan = if !is_oneway {
            let chan = Arc::new(PendingReply::new());
            let mut replies = sender.pending_replies.lock().unwrap();
            replies.insert(tx_id, Arc::clone(&chan));
            Some((tx_id, chan))
        } else {
            None
        };

        let mut target_cmd = Vec::with_capacity(4 + std::mem::size_of::<BinderTransactionData>());
        target_cmd.extend_from_slice(&BR_TRANSACTION.to_ne_bytes());
        target_cmd.extend_from_slice(bytes_of(&tr));

        // Check if target needs threadpool expansion (BR_SPAWN_LOOPER)
        let waiting = target_client.waiting_loopers.load(Ordering::SeqCst);
        let registered = target_client.registered_loopers.load(Ordering::SeqCst);
        let max = target_client.max_threads.load(Ordering::SeqCst);
        if waiting == 0 && registered < max {
            let mut spawn_cmd = Vec::with_capacity(4);
            spawn_cmd.extend_from_slice(&BR_SPAWN_LOOPER.to_ne_bytes());
            target_client.push_read_command(&spawn_cmd);
        }

        // Deliver to target read queue
        target_client.push_read_command(&target_cmd);

        if is_oneway {
            // Push BR_TRANSACTION_COMPLETE to sender
            let mut comp_cmd = Vec::with_capacity(4);
            comp_cmd.extend_from_slice(&BR_TRANSACTION_COMPLETE.to_ne_bytes());
            sender.push_read_command(&comp_cmd);
            Ok(None)
        } else {
            // Synchronously wait for reply
            let (id, chan) = reply_chan.unwrap();
            let reply_tr = chan.wait_reply()?;
            {
                let mut replies = sender.pending_replies.lock().unwrap();
                replies.remove(&id);
            }
            Ok(Some(reply_tr))
        }
    }

    /// Handle `BC_REPLY` from recipient service process.
    fn handle_bc_reply(
        &self,
        service_client: &MockClientProcess,
        tr: BinderTransactionData,
    ) -> Result<(), DriverError> {
        let tx_id = if tr.cookie != 0 {
            tr.cookie
        } else {
            return Err(DriverError::DeadObject);
        };

        // Find which client process is waiting for this tx_id
        let mut target_sender: Option<(Arc<MockClientProcess>, Arc<PendingReply>)> = None;
        {
            let clients = self.clients.lock().unwrap();
            for client in clients.values() {
                let replies = client.pending_replies.lock().unwrap();
                if let Some(chan) = replies.get(&tx_id) {
                    target_sender = Some((Arc::clone(client), Arc::clone(chan)));
                    break;
                }
            }
        }

        let (sender_client, reply_chan) = target_sender.ok_or(DriverError::DeadObject)?;

        // Read service reply payload bytes
        let reply_payload = if tr.data_size > 0 && tr.data_buffer != 0 {
            unsafe {
                std::slice::from_raw_parts(
                    tr.data_buffer as *const u8,
                    tr.data_size as usize,
                )
            }
            .to_vec()
        } else {
            Vec::new()
        };

        // Allocate reply buffer in sender process's mmap region
        let sender_data_ptr = if !reply_payload.is_empty() {
            let ptr = sender_client
                .mmap_region
                .allocate_buffer(reply_payload.len())
                .map_err(|_| DriverError::NoMemory)?;
            sender_client
                .mmap_region
                .write_bytes(ptr, &reply_payload)
                .map_err(|_| DriverError::NoMemory)?;
            let mut allocs = sender_client.allocated_buffers.lock().unwrap();
            allocs.insert(ptr, reply_payload.len());
            ptr
        } else {
            0
        };

        let mut final_reply_tr = tr;
        final_reply_tr.data_buffer = sender_data_ptr;
        final_reply_tr.offsets_buffer = 0;
        final_reply_tr.offsets_size = 0;

        // Wake sender waiting on reply_chan
        reply_chan.set_reply(final_reply_tr);

        // Notify service process that transaction completed
        let mut comp_cmd = Vec::with_capacity(4);
        comp_cmd.extend_from_slice(&BR_TRANSACTION_COMPLETE.to_ne_bytes());
        service_client.push_read_command(&comp_cmd);

        Ok(())
    }

    /// Resolve target process PID, ptr, and cookie for a transaction.
    fn resolve_target(
        &self,
        sender: &MockClientProcess,
        tr: &BinderTransactionData,
    ) -> Result<(u32, u64, u64), DriverError> {
        let target_handle = tr.target_handle();

        // Handle 0 = Context Manager (ServiceManager)
        if target_handle == 0 {
            let cm = self.context_manager.lock().unwrap();
            let (pid, ptr, cookie) = cm.ok_or(DriverError::HandleNotFound(0))?;
            return Ok((pid, ptr, cookie));
        }

        // Handle N = lookup in sender's handle table
        let ht = sender.handle_table.lock().unwrap();
        if let Some(reg) = ht.get(&target_handle) {
            Ok((reg.target_pid, reg.target_ptr, reg.target_cookie))
        } else {
            Err(DriverError::HandleNotFound(target_handle))
        }
    }

    /// Register a handle descriptor in a client's handle table.
    pub fn add_handle_for_client(
        &self,
        client: &MockClientProcess,
        target_pid: u32,
        target_ptr: u64,
        target_cookie: u64,
    ) -> u32 {
        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let mut ht = client.handle_table.lock().unwrap();
        ht.insert(
            handle,
            RegisteredHandle {
                target_pid,
                target_ptr,
                target_cookie,
                strong_refs: 1,
            },
        );
        handle
    }

    /// Drain queued read commands into userspace read buffer.
    fn fill_read_buffer(
        &self,
        client: &MockClientProcess,
        dest: &mut [u8],
        block_if_empty: bool,
    ) -> Result<usize, DriverError> {
        let mut written = 0;

        let mut queue = client.read_queue.lock().unwrap();
        if queue.is_empty() && block_if_empty {
            client.waiting_loopers.fetch_add(1, Ordering::SeqCst);
            while queue.is_empty() {
                queue = client
                    .queue_cond
                    .wait(queue)
                    .map_err(|e| DriverError::IoError(e.to_string()))?;
            }
            client.waiting_loopers.fetch_sub(1, Ordering::SeqCst);
        }

        while let Some(front) = queue.front() {
            if written + front.len() <= dest.len() {
                let cmd = queue.pop_front().unwrap();
                dest[written..written + cmd.len()].copy_from_slice(&cmd);
                written += cmd.len();
            } else {
                break;
            }
        }

        // If nothing was written and read_size >= 4, emit BR_NOOP
        if written == 0 && dest.len() >= 4 {
            dest[..4].copy_from_slice(&BR_NOOP.to_ne_bytes());
            written = 4;
        }

        Ok(written)
    }

    /// Trigger process death and send `BR_DEAD_BINDER` to all registered death watchers.
    pub fn trigger_process_death(&self, dead_pid: u32) {
        let clients = self.clients.lock().unwrap();
        for client in clients.values() {
            let watchers = client.death_watchers.lock().unwrap().clone();
            for w in watchers {
                let mut is_match = false;
                {
                    let ht = client.handle_table.lock().unwrap();
                    if let Some(h) = ht.get(&w.handle) {
                        if h.target_pid == dead_pid {
                            is_match = true;
                        }
                    }
                }

                if is_match {
                    let mut dead_cmd = Vec::with_capacity(12);
                    dead_cmd.extend_from_slice(&BR_DEAD_BINDER.to_ne_bytes());
                    dead_cmd.extend_from_slice(&w.cookie.to_ne_bytes());
                    client.push_read_command(&dead_cmd);
                }
            }
        }
    }
}
