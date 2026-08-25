//! Process-wide Binder singleton managing driver context, memory mapping, and threadpool.

#[cfg(target_os = "linux")]
use crate::driver::LinuxBinderDriver;
use crate::driver::{BinderDriverBackend, MockDriverBackend};
use crate::mmap::{BinderMmapRegion, BINDER_DEFAULT_MMAP_SIZE};
use crate::mock_driver::MockBinderDriver;
use aidl_compat::death::DeathRecipient;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

static GLOBAL_PROCESS_STATE: OnceLock<Arc<ProcessState>> = OnceLock::new();

/// Process-wide Binder subsystem state.
pub struct ProcessState {
    driver: Arc<dyn BinderDriverBackend>,
    max_threads: AtomicU32,
    active_workers: AtomicU32,
    threadpool_started: AtomicBool,
    // Local service objects: cookie/ptr -> Arc<dyn IBinder>
    local_objects: RwLock<HashMap<u64, Arc<dyn IBinder>>>,
    // Registered death recipients: cookie -> DeathRecipient
    death_recipients: RwLock<HashMap<u64, Arc<dyn DeathRecipient>>>,
}

impl ProcessState {
    /// Initialize with a custom driver backend.
    pub fn new(driver: Arc<dyn BinderDriverBackend>, max_threads: u32) -> Arc<Self> {
        let _ = driver.set_max_threads(max_threads);
        Arc::new(Self {
            driver,
            max_threads: AtomicU32::new(max_threads),
            active_workers: AtomicU32::new(0),
            threadpool_started: AtomicBool::new(false),
            local_objects: RwLock::new(HashMap::new()),
            death_recipients: RwLock::new(HashMap::new()),
        })
    }

    /// Access the global singleton `ProcessState`, initializing with default driver if unset.
    pub fn self_or_init() -> Arc<Self> {
        GLOBAL_PROCESS_STATE
            .get_or_init(|| {
                #[cfg(target_os = "linux")]
                {
                    if let Ok(linux_driver) =
                        LinuxBinderDriver::open("/dev/binder", BINDER_DEFAULT_MMAP_SIZE)
                    {
                        return Self::new(Arc::new(linux_driver), 15);
                    }
                }
                // Fallback / standard cross-platform mock driver
                let mock_driver = Arc::new(MockBinderDriver::new());
                let backend = MockDriverBackend::new(mock_driver, BINDER_DEFAULT_MMAP_SIZE);
                Self::new(Arc::new(backend), 15)
            })
            .clone()
    }

    /// Initialize singleton with custom mock driver (useful for tests).
    pub fn init_mock(driver: Arc<MockBinderDriver>) -> Arc<Self> {
        let backend = MockDriverBackend::new(driver, BINDER_DEFAULT_MMAP_SIZE);
        Self::new(Arc::new(backend), 15)
    }

    /// Access driver backend.
    pub fn driver(&self) -> &Arc<dyn BinderDriverBackend> {
        &self.driver
    }

    /// Access shared memory mapping region.
    pub fn mmap_region(&self) -> Arc<BinderMmapRegion> {
        self.driver.mmap_region()
    }

    /// Return process ID.
    pub fn pid(&self) -> u32 {
        self.driver.pid()
    }

    /// Register a local service stub object in this process's object table.
    pub fn register_service_object(&self, cookie: u64, object: SpIBinder) {
        let mut map = self.local_objects.write().unwrap();
        map.insert(cookie, object.into_arc());
    }

    /// Unregister a local service stub object.
    pub fn unregister_service_object(&self, cookie: u64) {
        let mut map = self.local_objects.write().unwrap();
        map.remove(&cookie);
    }

    /// Lookup a local service stub by cookie.
    pub fn get_service_object(&self, cookie: u64) -> Option<SpIBinder> {
        let map = self.local_objects.read().unwrap();
        map.get(&cookie).cloned().map(SpIBinder::from_arc)
    }

    /// Register a death recipient callback for a remote binder cookie.
    pub fn register_death_recipient(&self, cookie: u64, recipient: Arc<dyn DeathRecipient>) {
        let mut map = self.death_recipients.write().unwrap();
        map.insert(cookie, recipient);
    }

    /// Unregister death recipient.
    pub fn unregister_death_recipient(&self, cookie: u64) {
        let mut map = self.death_recipients.write().unwrap();
        map.remove(&cookie);
    }

    /// Trigger death notification for a cookie.
    pub fn notify_death(&self, cookie: u64) {
        let recipient = {
            let map = self.death_recipients.read().unwrap();
            map.get(&cookie).cloned()
        };
        if let Some(r) = recipient {
            r.binder_died();
        }
    }

    /// Start the worker threadpool.
    pub fn start_thread_pool(self: &Arc<Self>) {
        if !self.threadpool_started.swap(true, Ordering::SeqCst) {
            self.spawn_worker_thread_if_needed();
        }
    }

    /// Spawn a replacement worker thread if current active workers < max_threads.
    ///
    /// This is the key "spawn-before-block" mechanism required to prevent deadlocks
    /// during nested synchronous Binder transactions and handle `BR_SPAWN_LOOPER`.
    pub fn spawn_worker_thread_if_needed(self: &Arc<Self>) -> bool {
        let max = self.max_threads.load(Ordering::SeqCst);
        loop {
            let active = self.active_workers.load(Ordering::SeqCst);
            if active >= max {
                return false;
            }
            if self
                .active_workers
                .compare_exchange_weak(active, active + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                break;
            }
        }

        let this = Arc::clone(self);
        let builder = std::thread::Builder::new().name("BinderWorker".into());
        let spawn_res = builder.spawn(move || {
            let mut thread_state =
                crate::ipc_thread_state::IPCThreadState::with_process(Arc::clone(&this));
            let _ = thread_state.join_thread_pool();
            this.active_workers.fetch_sub(1, Ordering::SeqCst);
        });

        match spawn_res {
            Ok(_) => true,
            Err(_) => {
                self.active_workers.fetch_sub(1, Ordering::SeqCst);
                false
            }
        }
    }

    /// Return active worker count.
    pub fn active_worker_count(&self) -> u32 {
        self.active_workers.load(Ordering::SeqCst)
    }

    /// Return max threads.
    pub fn max_threads(&self) -> u32 {
        self.max_threads.load(Ordering::SeqCst)
    }
}
