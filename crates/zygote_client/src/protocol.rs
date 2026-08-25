//! Zygote socket argument wire format encoding and PID response decoding.

use crate::error::{ZygoteError, ZygoteResult};
use serde::{Deserialize, Serialize};

/// Default UID and GID for sandboxed Android app processes (APP_ID baseline).
pub const DEFAULT_APP_UID: u32 = 10000;
pub const DEFAULT_APP_GID: u32 = 10000;

/// Default target SDK version for Android 13 (Tiramisu).
pub const DEFAULT_TARGET_SDK_VERSION: u32 = 33;

/// Default Java entry point for Android application processes.
pub const DEFAULT_ENTRY_POINT: &str = "android.app.ActivityThread";

/// Arguments formatted for Zygote process spawning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZygoteSpawnArgs {
    /// Package name of the application (e.g. `"com.example.game"`).
    pub package_name: String,

    /// Nice name / process name displayed in ps / /proc (e.g. `"com.example.game"`).
    pub nice_name: String,

    /// Linux user ID to assign to the child process.
    pub uid: u32,

    /// Linux primary group ID to assign to the child process.
    pub gid: u32,

    /// Target Android SDK version (API level, e.g. 33).
    pub target_sdk_version: u32,

    /// Entry point class name (defaults to `"android.app.ActivityThread"`).
    pub entry_point: String,

    /// Additional supplementary group IDs (e.g. `[1015, 1028]` for storage/network).
    pub gids: Vec<u32>,

    /// SELinux security context information (e.g. `"default:targetSdkVersion=33"`).
    pub se_info: Option<String>,

    /// ABI / instruction set architecture (e.g. `"arm64-v8a"` or `"x86_64"`).
    pub instruction_set: Option<String>,

    /// App data sandbox directory path (e.g. `"/data/user/0/com.example.game"`).
    pub app_data_dir: Option<String>,

    /// Additional raw arguments appended to the spawn command.
    pub extra_args: Vec<String>,
}

impl ZygoteSpawnArgs {
    /// Create new spawn arguments with default Android 13 parameters.
    pub fn new(package_name: impl Into<String>, nice_name: impl Into<String>) -> Self {
        Self {
            package_name: package_name.into(),
            nice_name: nice_name.into(),
            uid: DEFAULT_APP_UID,
            gid: DEFAULT_APP_GID,
            target_sdk_version: DEFAULT_TARGET_SDK_VERSION,
            entry_point: DEFAULT_ENTRY_POINT.to_string(),
            gids: Vec::new(),
            se_info: None,
            instruction_set: None,
            app_data_dir: None,
            extra_args: Vec::new(),
        }
    }

    /// Builder method to specify UID.
    pub fn with_uid(mut self, uid: u32) -> Self {
        self.uid = uid;
        self
    }

    /// Builder method to specify GID.
    pub fn with_gid(mut self, gid: u32) -> Self {
        self.gid = gid;
        self
    }

    /// Builder method to specify target SDK version.
    pub fn with_target_sdk_version(mut self, sdk: u32) -> Self {
        self.target_sdk_version = sdk;
        self
    }

    /// Builder method to specify entry point class.
    pub fn with_entry_point(mut self, entry: impl Into<String>) -> Self {
        self.entry_point = entry.into();
        self
    }

    /// Builder method to append supplementary group IDs.
    pub fn with_gids(mut self, gids: Vec<u32>) -> Self {
        self.gids = gids;
        self
    }

    /// Builder method to specify SELinux context info.
    pub fn with_se_info(mut self, se_info: impl Into<String>) -> Self {
        self.se_info = Some(se_info.into());
        self
    }

    /// Builder method to specify app data directory.
    pub fn with_app_data_dir(mut self, data_dir: impl Into<String>) -> Self {
        self.app_data_dir = Some(data_dir.into());
        self
    }

    /// Format argument lines into standard Zygote command arguments array.
    pub fn format_command_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();

        lines.push(format!("--setuid={}", self.uid));
        lines.push(format!("--setgid={}", self.gid));

        if !self.gids.is_empty() {
            let gids_str: Vec<String> = self.gids.iter().map(|g| g.to_string()).collect();
            lines.push(format!("--setgroups={}", gids_str.join(",")));
        }

        lines.push(format!("--target-sdk-version={}", self.target_sdk_version));
        lines.push(format!("--package-name={}", self.package_name));
        lines.push(format!("--nice-name={}", self.nice_name));

        if let Some(ref se_info) = self.se_info {
            lines.push(format!("--seinfo={}", se_info));
        }

        if let Some(ref isa) = self.instruction_set {
            lines.push(format!("--instruction-set={}", isa));
        }

        if let Some(ref data_dir) = self.app_data_dir {
            lines.push(format!("--app-data-dir={}", data_dir));
        }

        for extra in &self.extra_args {
            lines.push(extra.clone());
        }

        // The entry point (e.g. android.app.ActivityThread) is the trailing positional argument
        lines.push(self.entry_point.clone());

        lines
    }

    /// Serialize into the Zygote wire protocol byte stream:
    /// `<argument_count>\n<arg1>\n<arg2>\n...<argN>\n`
    pub fn encode_wire_bytes(&self) -> Vec<u8> {
        let lines = self.format_command_lines();
        let mut buffer = Vec::new();

        // Write line count followed by newline
        let count_header = format!("{}\n", lines.len());
        buffer.extend_from_slice(count_header.as_bytes());

        // Write each argument line
        for line in lines {
            buffer.extend_from_slice(line.as_bytes());
            buffer.push(b'\n');
        }

        buffer
    }

    /// Parse Zygote wire bytes into a `ZygoteSpawnArgs` struct.
    pub fn parse_wire_bytes(bytes: &[u8]) -> ZygoteResult<Self> {
        let s = std::str::from_utf8(bytes)
            .map_err(|e| ZygoteError::ProtocolViolation(format!("Invalid UTF-8 in payload: {e}")))?;

        let mut lines = s.lines();
        let first_line = lines.next().ok_or_else(|| {
            ZygoteError::ProtocolViolation("Empty Zygote command payload".to_string())
        })?;

        let expected_count: usize = first_line.trim().parse().map_err(|e| {
            ZygoteError::ProtocolViolation(format!("Invalid argument count header '{first_line}': {e}"))
        })?;

        let parsed_lines: Vec<&str> = lines.collect();
        if parsed_lines.len() != expected_count {
            return Err(ZygoteError::ProtocolViolation(format!(
                "Argument count mismatch: header specified {}, but found {}",
                expected_count,
                parsed_lines.len()
            )));
        }

        let mut package_name = String::new();
        let mut nice_name = String::new();
        let mut uid = DEFAULT_APP_UID;
        let mut gid = DEFAULT_APP_GID;
        let mut target_sdk_version = DEFAULT_TARGET_SDK_VERSION;
        let mut entry_point = DEFAULT_ENTRY_POINT.to_string();
        let mut gids = Vec::new();
        let mut se_info = None;
        let mut instruction_set = None;
        let mut app_data_dir = None;
        let mut extra_args = Vec::new();

        for line in &parsed_lines {
            if let Some(val) = line.strip_prefix("--setuid=") {
                uid = val.parse().unwrap_or(DEFAULT_APP_UID);
            } else if let Some(val) = line.strip_prefix("--setgid=") {
                gid = val.parse().unwrap_or(DEFAULT_APP_GID);
            } else if let Some(val) = line.strip_prefix("--setgroups=") {
                gids = val
                    .split(',')
                    .filter_map(|g| g.trim().parse::<u32>().ok())
                    .collect();
            } else if let Some(val) = line.strip_prefix("--target-sdk-version=") {
                target_sdk_version = val.parse().unwrap_or(DEFAULT_TARGET_SDK_VERSION);
            } else if let Some(val) = line.strip_prefix("--package-name=") {
                package_name = val.to_string();
            } else if let Some(val) = line.strip_prefix("--nice-name=") {
                nice_name = val.to_string();
            } else if let Some(val) = line.strip_prefix("--seinfo=") {
                se_info = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("--instruction-set=") {
                instruction_set = Some(val.to_string());
            } else if let Some(val) = line.strip_prefix("--app-data-dir=") {
                app_data_dir = Some(val.to_string());
            } else if !line.starts_with("--") {
                entry_point = line.to_string();
            } else {
                extra_args.push(line.to_string());
            }
        }

        Ok(Self {
            package_name,
            nice_name,
            uid,
            gid,
            target_sdk_version,
            entry_point,
            gids,
            se_info,
            instruction_set,
            app_data_dir,
            extra_args,
        })
    }
}

/// Format 4-byte little-endian PID response buffer.
pub fn format_pid_response(pid: i32) -> [u8; 4] {
    pid.to_le_bytes()
}

/// Parse 4-byte little-endian PID response from Zygote daemon.
///
/// Zygote returns a 4-byte signed integer PID:
/// - `pid > 0`: Successful fork; returns child PID.
/// - `pid <= 0`: Fork failure; returns `ZygoteError::ForkFailed`.
pub fn parse_pid_response(bytes: &[u8]) -> ZygoteResult<u32> {
    if bytes.len() != 4 {
        return Err(ZygoteError::InvalidPidResponse(bytes.len()));
    }

    let buf: [u8; 4] = [bytes[0], bytes[1], bytes[2], bytes[3]];
    let pid = i32::from_le_bytes(buf);

    if pid <= 0 {
        return Err(ZygoteError::ForkFailed {
            pid,
            message: format!("Zygote returned failure code/invalid PID {pid}"),
        });
    }

    Ok(pid as u32)
}
