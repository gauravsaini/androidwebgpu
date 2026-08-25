//! Zygote abstract socket transport, client implementation, and mock server.

use crate::error::{ZygoteError, ZygoteResult};
use crate::process::{ProcessRecord, ProcessTracker};
use crate::protocol::{parse_pid_response, ZygoteSpawnArgs};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// Default abstract socket name for Zygote primary 64-bit daemon.
pub const DEFAULT_ZYGOTE_SOCKET_ABSTRACT: &str = "@zygote";

/// Default filesystem socket path for Zygote daemon on Android.
pub const DEFAULT_ZYGOTE_SOCKET_PATH: &str = "/dev/socket/zygote";

/// Trait for mocking or intercepting Zygote fork requests in test environments.
pub trait ZygoteMockHandler: Send + Sync + 'static {
    /// Handle a spawn request and return the assigned child PID or an error.
    fn handle_spawn(&self, args: &ZygoteSpawnArgs) -> ZygoteResult<u32>;
}

/// A configurable in-memory mock handler for automated unit and integration tests.
#[derive(Debug)]
pub struct MockZygoteHandler {
    next_pid: AtomicU32,
    fail_next: Mutex<Option<ZygoteError>>,
    received_requests: Arc<RwLock<Vec<ZygoteSpawnArgs>>>,
}

impl MockZygoteHandler {
    /// Create a new mock handler starting PID sequence at `start_pid`.
    pub fn new(start_pid: u32) -> Self {
        Self {
            next_pid: AtomicU32::new(start_pid),
            fail_next: Mutex::new(None),
            received_requests: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set an error to be returned on the next fork call.
    pub fn set_fail_next(&self, error: ZygoteError) {
        let mut lock = self.fail_next.lock().unwrap();
        *lock = Some(error);
    }

    /// Retrieve clone of all received spawn arguments.
    pub fn get_received_requests(&self) -> Vec<ZygoteSpawnArgs> {
        let lock = self.received_requests.read().unwrap();
        lock.clone()
    }

    /// Clear recorded spawn requests.
    pub fn clear_received_requests(&self) {
        let mut lock = self.received_requests.write().unwrap();
        lock.clear();
    }
}

impl Default for MockZygoteHandler {
    fn default() -> Self {
        Self::new(10001)
    }
}

impl ZygoteMockHandler for MockZygoteHandler {
    fn handle_spawn(&self, args: &ZygoteSpawnArgs) -> ZygoteResult<u32> {
        if let Some(err) = self.fail_next.lock().unwrap().take() {
            return Err(err);
        }

        // Validate basic parameters
        if args.package_name.is_empty() {
            return Err(ZygoteError::ProtocolViolation(
                "Package name cannot be empty".to_string(),
            ));
        }

        // Record request
        {
            let mut lock = self.received_requests.write().unwrap();
            lock.push(args.clone());
        }

        // Allocate PID
        let pid = self.next_pid.fetch_add(1, Ordering::SeqCst);
        Ok(pid)
    }
}

/// Endpoint target for Zygote client connections.
#[derive(Clone)]
pub enum ZygoteEndpoint {
    /// Abstract Unix Domain Socket (e.g. `"@zygote"` or `"zygote"`).
    Abstract(String),
    /// Filesystem Unix Domain Socket Path (e.g. `"/dev/socket/zygote"`).
    Path(PathBuf),
    /// In-memory mock transport for host tests.
    Mock(Arc<dyn ZygoteMockHandler>),
}

/// Zygote client providing process spawning and lifecycle tracking.
#[derive(Clone)]
pub struct ZygoteClient {
    endpoint: ZygoteEndpoint,
    tracker: ProcessTracker,
}

impl ZygoteClient {
    /// Create a new Zygote client for the specified endpoint with a fresh tracker.
    pub fn new(endpoint: ZygoteEndpoint) -> Self {
        Self {
            endpoint,
            tracker: ProcessTracker::new(),
        }
    }

    /// Create a client connecting to an abstract unix domain socket.
    pub fn new_abstract(name: impl Into<String>) -> Self {
        Self::new(ZygoteEndpoint::Abstract(name.into()))
    }

    /// Create a client connecting to a filesystem socket path.
    pub fn new_path(path: impl Into<PathBuf>) -> Self {
        Self::new(ZygoteEndpoint::Path(path.into()))
    }

    /// Create a client backed by an in-memory mock handler.
    pub fn new_mock(handler: Arc<dyn ZygoteMockHandler>) -> Self {
        Self::new(ZygoteEndpoint::Mock(handler))
    }

    /// Create a default mock client for tests.
    pub fn new_mock_default() -> (Self, Arc<MockZygoteHandler>) {
        let handler = Arc::new(MockZygoteHandler::default());
        let client = Self::new_mock(handler.clone());
        (client, handler)
    }

    /// Reference to the internal process tracker registry.
    pub fn tracker(&self) -> &ProcessTracker {
        &self.tracker
    }

    /// Connect to the configured Unix domain socket.
    fn connect_socket(&self) -> ZygoteResult<UnixStream> {
        match &self.endpoint {
            ZygoteEndpoint::Path(path) => {
                UnixStream::connect(path).map_err(|e| {
                    ZygoteError::ConnectionFailed(path.display().to_string(), e.to_string())
                })
            }
            ZygoteEndpoint::Abstract(name) => {
                #[cfg(target_os = "linux")]
                {
                    use std::os::linux::net::SocketAddrExt;
                    let clean_name = name.trim_start_matches('@');
                    let addr = std::os::unix::net::SocketAddr::from_abstract_name(clean_name)
                        .map_err(|e| {
                            ZygoteError::ConnectionFailed(name.clone(), e.to_string())
                        })?;
                    UnixStream::connect_addr(&addr).map_err(|e| {
                        ZygoteError::ConnectionFailed(name.clone(), e.to_string())
                    })
                }
                #[cfg(not(target_os = "linux"))]
                {
                    // Fallback on non-Linux systems: attempt filesystem socket if path exists,
                    // otherwise return error informing to use mock transport in host tests.
                    let fallback_path = PathBuf::from(DEFAULT_ZYGOTE_SOCKET_PATH);
                    if fallback_path.exists() {
                        UnixStream::connect(&fallback_path).map_err(|e| {
                            ZygoteError::ConnectionFailed(
                                fallback_path.display().to_string(),
                                e.to_string(),
                            )
                        })
                    } else {
                        Err(ZygoteError::ConnectionFailed(
                            name.clone(),
                            "Abstract namespace sockets are only supported on Linux. Use Mock transport for host unit tests."
                                .to_string(),
                        ))
                    }
                }
            }
            ZygoteEndpoint::Mock(_) => Err(ZygoteError::MockFailure(
                "Cannot open raw socket on mock endpoint".to_string(),
            )),
        }
    }

    /// Fork a new application process with full `ZygoteSpawnArgs`.
    pub fn fork_app(&self, args: &ZygoteSpawnArgs) -> ZygoteResult<u32> {
        let pid = match &self.endpoint {
            ZygoteEndpoint::Mock(handler) => handler.handle_spawn(args)?,
            _ => {
                let mut stream = self.connect_socket()?;
                let payload = args.encode_wire_bytes();

                // Send spawn arguments
                stream.write_all(&payload).map_err(ZygoteError::Io)?;
                stream.flush().map_err(ZygoteError::Io)?;

                // Read 4-byte PID response
                let mut pid_buf = [0u8; 4];
                stream.read_exact(&mut pid_buf).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        ZygoteError::InvalidPidResponse(0)
                    } else {
                        ZygoteError::Io(e)
                    }
                })?;

                parse_pid_response(&pid_buf)?
            }
        };

        // Register in process tracker
        let record = ProcessRecord::from_spawn_args(pid, args);
        let _ = self.tracker.register_process(record);

        Ok(pid)
    }

    /// Convenience wrapper to fork an application with standard parameters.
    pub fn fork_app_simple(
        &self,
        package_name: &str,
        nice_name: &str,
        uid: u32,
        gid: u32,
        target_sdk: u32,
    ) -> ZygoteResult<u32> {
        let args = ZygoteSpawnArgs::new(package_name, nice_name)
            .with_uid(uid)
            .with_gid(gid)
            .with_target_sdk_version(target_sdk);

        self.fork_app(&args)
    }
}
