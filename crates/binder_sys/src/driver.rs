//! Abstraction over physical Linux `/dev/binder` kernel device and simulated `MockBinderDriver`.

use crate::mmap::BinderMmapRegion;
use crate::mock_driver::{DriverError, MockBinderDriver, MockClientProcess};
use crate::sys::*;
use std::sync::Arc;

/// Common interface for Binder driver backends.
pub trait BinderDriverBackend: Send + Sync {
    /// Execute `BINDER_WRITE_READ` ioctl.
    fn write_read(&self, bwr: &mut binder_write_read) -> Result<(), DriverError>;

    /// Execute `BINDER_SET_MAX_THREADS` ioctl.
    fn set_max_threads(&self, max_threads: u32) -> Result<(), DriverError>;

    /// Execute `BINDER_VERSION` ioctl.
    fn get_version(&self) -> Result<i32, DriverError>;

    /// Execute `BINDER_THREAD_EXIT` ioctl.
    fn thread_exit(&self) -> Result<(), DriverError>;

    /// Return reference to shared memory mapping region.
    fn mmap_region(&self) -> Arc<BinderMmapRegion>;

    /// Get client process ID.
    fn pid(&self) -> u32;
}

/// Simulated in-memory driver backend.
pub struct MockDriverBackend {
    driver: Arc<MockBinderDriver>,
    client: Arc<MockClientProcess>,
}

impl MockDriverBackend {
    /// Create new mock driver backend attached to a driver instance.
    pub fn new(driver: Arc<MockBinderDriver>, mmap_size: usize) -> Self {
        let client = driver.register_client(mmap_size);
        Self { driver, client }
    }

    /// Access underlying mock driver.
    pub fn driver(&self) -> &Arc<MockBinderDriver> {
        &self.driver
    }

    /// Access underlying client process.
    pub fn client(&self) -> &Arc<MockClientProcess> {
        &self.client
    }
}

impl BinderDriverBackend for MockDriverBackend {
    fn write_read(&self, bwr: &mut binder_write_read) -> Result<(), DriverError> {
        self.driver.write_read(&self.client, bwr)
    }

    fn set_max_threads(&self, max_threads: u32) -> Result<(), DriverError> {
        self.driver.set_max_threads(&self.client, max_threads)
    }

    fn get_version(&self) -> Result<i32, DriverError> {
        self.driver.get_version()
    }

    fn thread_exit(&self) -> Result<(), DriverError> {
        let mut exit_bwr = binder_write_read::new();
        let mut exit_cmd = Vec::with_capacity(4);
        exit_cmd.extend_from_slice(&BC_EXIT_LOOPER.to_ne_bytes());
        exit_bwr.write_buffer = exit_cmd.as_ptr() as u64;
        exit_bwr.write_size = exit_cmd.len() as u64;
        self.driver.write_read(&self.client, &mut exit_bwr)
    }

    fn mmap_region(&self) -> Arc<BinderMmapRegion> {
        Arc::clone(&self.client.mmap_region)
    }

    fn pid(&self) -> u32 {
        self.client.pid
    }
}

/// Linux `/dev/binder` driver backend (activated on Linux targets with physical /dev/binder).
pub struct LinuxBinderDriver {
    #[cfg(target_os = "linux")]
    fd: i32,
    mmap_region: Arc<BinderMmapRegion>,
}

impl LinuxBinderDriver {
    /// Open `/dev/binder` on Linux.
    #[cfg(target_os = "linux")]
    pub fn open(device_path: &str, mmap_size: usize) -> Result<Self, DriverError> {
        use std::ffi::CString;
        let c_path = CString::new(device_path).map_err(|e| DriverError::InvalidArgument(e.to_string()))?;
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(DriverError::IoError(format!(
                "Failed to open {}: errno {}",
                device_path,
                std::io::Error::last_os_error()
            )));
        }

        let map_size = mmap_size.clamp(
            crate::mmap::BINDER_MIN_MMAP_SIZE,
            crate::mmap::BINDER_MAX_MMAP_SIZE,
        );

        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                map_size,
                libc::PROT_READ,
                libc::MAP_PRIVATE | libc::MAP_NORESERVE,
                fd,
                0,
            )
        };

        if mapped == libc::MAP_FAILED {
            unsafe { libc::close(fd) };
            return Err(DriverError::IoError(format!(
                "mmap failed on {}: errno {}",
                device_path,
                std::io::Error::last_os_error()
            )));
        }

        let mmap_region = unsafe { BinderMmapRegion::from_raw_mmap(mapped as *mut u8, map_size) };

        Ok(Self { fd, mmap_region })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn open(_device_path: &str, _mmap_size: usize) -> Result<Self, DriverError> {
        Err(DriverError::IoError(
            "Physical /dev/binder is only supported on Linux".into(),
        ))
    }
}

impl BinderDriverBackend for LinuxBinderDriver {
    fn write_read(&self, bwr: &mut binder_write_read) -> Result<(), DriverError> {
        #[cfg(target_os = "linux")]
        {
            let res = unsafe {
                libc::ioctl(
                    self.fd,
                    BINDER_WRITE_READ as libc::c_ulong,
                    bwr as *mut binder_write_read,
                )
            };
            if res < 0 {
                return Err(DriverError::IoError(format!(
                    "BINDER_WRITE_READ ioctl failed: errno {}",
                    std::io::Error::last_os_error()
                )));
            }
            let _ = bwr;
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = bwr;
            Err(DriverError::IoError("Not supported on non-linux".into()))
        }
    }

    fn set_max_threads(&self, max_threads: u32) -> Result<(), DriverError> {
        #[cfg(target_os = "linux")]
        {
            let res = unsafe {
                libc::ioctl(
                    self.fd,
                    BINDER_SET_MAX_THREADS as libc::c_ulong,
                    &max_threads as *const u32,
                )
            };
            if res < 0 {
                return Err(DriverError::IoError(format!(
                    "BINDER_SET_MAX_THREADS failed: errno {}",
                    std::io::Error::last_os_error()
                )));
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = max_threads;
            Err(DriverError::IoError("Not supported on non-linux".into()))
        }
    }

    fn get_version(&self) -> Result<i32, DriverError> {
        #[cfg(target_os = "linux")]
        {
            let mut version = binder_version::new(0);
            let res = unsafe {
                libc::ioctl(
                    self.fd,
                    BINDER_VERSION as libc::c_ulong,
                    &mut version as *mut binder_version,
                )
            };
            if res < 0 {
                return Err(DriverError::IoError(format!(
                    "BINDER_VERSION failed: errno {}",
                    std::io::Error::last_os_error()
                )));
            }
            Ok(version.protocol_version)
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(DriverError::IoError("Not supported on non-linux".into()))
        }
    }

    fn thread_exit(&self) -> Result<(), DriverError> {
        #[cfg(target_os = "linux")]
        {
            let mut dummy: i32 = 0;
            let res = unsafe {
                libc::ioctl(
                    self.fd,
                    BINDER_THREAD_EXIT as libc::c_ulong,
                    &mut dummy as *mut i32,
                )
            };
            if res < 0 {
                return Err(DriverError::IoError(format!(
                    "BINDER_THREAD_EXIT failed: errno {}",
                    std::io::Error::last_os_error()
                )));
            }
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(DriverError::IoError("Not supported on non-linux".into()))
        }
    }

    fn mmap_region(&self) -> Arc<BinderMmapRegion> {
        Arc::clone(&self.mmap_region)
    }

    fn pid(&self) -> u32 {
        std::process::id()
    }
}
