//! In-memory Android rootfs and deterministic CPIO (initramfs) archive builder.
//!
//! Provides the guest filesystem hierarchy expected by Android (`/data/app`,
//! `/data/local/tmp`, `/system/bin`), in-memory file/directory staging, and
//! bit-reproducible newc CPIO encoding/decoding.
//!
//! Purity: 100% in-memory, deterministic, zero OS filesystem calls, zero hidden state.

use std::collections::{BTreeMap, BTreeSet};

/// Standard POSIX / Linux file mode flags.
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFREG: u32 = 0o100000;
pub const DEFAULT_DIR_MODE: u32 = S_IFDIR | 0o755;
pub const DEFAULT_FILE_MODE: u32 = S_IFREG | 0o644;
pub const DEFAULT_EXEC_MODE: u32 = S_IFREG | 0o755;

/// Errors produced during rootfs operations or CPIO parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootfsError {
    InvalidPath(String),
    DirectoryNotFound(String),
    FileNotFound(String),
    AlreadyExists(String),
    CorruptCpio(String),
}

impl std::fmt::Display for RootfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RootfsError::InvalidPath(p) => write!(f, "invalid path: {p}"),
            RootfsError::DirectoryNotFound(p) => write!(f, "directory not found: {p}"),
            RootfsError::FileNotFound(p) => write!(f, "file not found: {p}"),
            RootfsError::AlreadyExists(p) => write!(f, "entry already exists: {p}"),
            RootfsError::CorruptCpio(r) => write!(f, "corrupt CPIO archive: {r}"),
        }
    }
}

impl std::error::Error for RootfsError {}

/// A single regular file in the guest rootfs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub mode: u32,
    pub mtime: u32,
    pub data: Vec<u8>,
}

/// An in-memory, deterministic guest root filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rootfs {
    directories: BTreeSet<String>,
    files: BTreeMap<String, FileEntry>,
}

impl Default for Rootfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Rootfs {
    /// Create an empty rootfs with only the root directory `"/"`.
    pub fn new() -> Self {
        let mut directories = BTreeSet::new();
        directories.insert("/".to_string());
        Self {
            directories,
            files: BTreeMap::new(),
        }
    }

    /// Create a minimal Android guest rootfs hierarchy.
    ///
    /// Establishes the canonical directory layout used by AOSP:
    /// - `/data`
    /// - `/data/app` (sideloaded and installed application APKs)
    /// - `/data/local`
    /// - `/data/local/tmp` (adb push staging area)
    /// - `/system`
    /// - `/system/bin`
    /// - `/dev`, `/proc`, `/sys`, `/mnt`
    pub fn new_minimal() -> Self {
        let mut fs = Self::new();
        let dirs = [
            "/dev",
            "/proc",
            "/sys",
            "/mnt",
            "/system",
            "/system/bin",
            "/data",
            "/data/app",
            "/data/local",
            "/data/local/tmp",
        ];
        for d in dirs {
            fs.create_dir_all(d).expect("minimal directories are valid");
        }
        fs
    }

    /// Normalize a filesystem path:
    /// - Prepends leading `'/'` if missing.
    /// - Collapses multiple consecutive slashes into a single slash.
    /// - Removes trailing slashes (except for root `"/"`).
    /// - Rejects path traversal (`..` components).
    pub fn normalize_path(path: &str) -> Result<String, RootfsError> {
        if path.is_empty() {
            return Err(RootfsError::InvalidPath("empty path".to_string()));
        }
        let parts: Vec<&str> = path.split('/').collect();
        let mut clean_parts = Vec::new();
        for part in parts {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                return Err(RootfsError::InvalidPath(format!(
                    "path traversal forbidden: {path}"
                )));
            }
            clean_parts.push(part);
        }
        if clean_parts.is_empty() {
            return Ok("/".to_string());
        }
        Ok(format!("/{}", clean_parts.join("/")))
    }

    /// Create a directory and all missing parent directories.
    pub fn create_dir_all(&mut self, path: &str) -> Result<(), RootfsError> {
        let clean = Self::normalize_path(path)?;
        if clean == "/" {
            return Ok(());
        }
        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for part in parts {
            current.push('/');
            current.push_str(part);
            if self.files.contains_key(&current) {
                return Err(RootfsError::AlreadyExists(format!(
                    "file exists in path: {current}"
                )));
            }
            self.directories.insert(current.clone());
        }
        Ok(())
    }

    /// Write or overwrite a regular file at `path`. Parent directories will be
    /// created automatically if they do not yet exist.
    pub fn write_file(
        &mut self,
        path: &str,
        data: Vec<u8>,
        mode: u32,
    ) -> Result<(), RootfsError> {
        let clean = Self::normalize_path(path)?;
        if clean == "/" {
            return Err(RootfsError::InvalidPath(
                "cannot write file to root directory path".to_string(),
            ));
        }
        if self.directories.contains(&clean) {
            return Err(RootfsError::AlreadyExists(format!(
                "directory exists at path: {clean}"
            )));
        }
        if let Some(parent) = parent_dir(&clean) {
            self.create_dir_all(&parent)?;
        }
        let file_mode = if mode & S_IFREG == 0 && mode & S_IFDIR == 0 {
            mode | S_IFREG
        } else {
            mode
        };
        self.files.insert(
            clean.clone(),
            FileEntry {
                path: clean,
                mode: file_mode,
                mtime: 0,
                data,
            },
        );
        Ok(())
    }

    /// Get read access to a file's data.
    pub fn get_file(&self, path: &str) -> Option<&[u8]> {
        let clean = Self::normalize_path(path).ok()?;
        self.files.get(&clean).map(|e| e.data.as_slice())
    }

    /// True if a file exists at `path`.
    pub fn has_file(&self, path: &str) -> bool {
        Self::normalize_path(path)
            .ok()
            .map(|p| self.files.contains_key(&p))
            .unwrap_or(false)
    }

    /// True if a directory exists at `path`.
    pub fn has_dir(&self, path: &str) -> bool {
        Self::normalize_path(path)
            .ok()
            .map(|p| self.directories.contains(&p))
            .unwrap_or(false)
    }

    /// Total number of regular files in the rootfs.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total number of directories in the rootfs.
    pub fn dir_count(&self) -> usize {
        self.directories.len()
    }

    /// List immediate child names (files and directories) inside `dir`.
    pub fn list_dir(&self, dir: &str) -> Result<Vec<String>, RootfsError> {
        let clean = Self::normalize_path(dir)?;
        if !self.directories.contains(&clean) {
            return Err(RootfsError::DirectoryNotFound(clean));
        }
        let prefix = if clean == "/" {
            "/".to_string()
        } else {
            format!("{clean}/")
        };
        let mut results = BTreeSet::new();
        for d in &self.directories {
            if d.starts_with(&prefix) && d != &clean {
                let rel = &d[prefix.len()..];
                let item = rel.split('/').next().unwrap_or("");
                if !item.is_empty() {
                    results.insert(item.to_string());
                }
            }
        }
        for f in self.files.keys() {
            if f.starts_with(&prefix) {
                let rel = &f[prefix.len()..];
                let item = rel.split('/').next().unwrap_or("");
                if !item.is_empty() {
                    results.insert(item.to_string());
                }
            }
        }
        Ok(results.into_iter().collect())
    }

    /// Sideload an APK directly into the canonical Android location:
    /// `/data/app/<package>/base.apk`.
    ///
    /// Validates the package identifier before writing.
    pub fn sideload_apk(
        &mut self,
        package: &str,
        apk_bytes: &[u8],
    ) -> Result<String, RootfsError> {
        validate_package_name(package)?;
        let dest = format!("/data/app/{package}/base.apk");
        self.write_file(&dest, apk_bytes.to_vec(), DEFAULT_FILE_MODE)?;
        Ok(dest)
    }

    /// Encode the entire rootfs into a deterministic newc CPIO archive.
    ///
    /// The archive is sorted deterministically (directories first, then regular
    /// files). Suitable for feeding as an initramfs to Linux kernels.
    pub fn to_cpio(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut ino: u32 = 1;

        // Write directories (excluding root "/")
        for dir in &self.directories {
            if dir == "/" {
                continue;
            }
            let cpio_name = dir.trim_start_matches('/');
            write_cpio_entry(
                &mut out,
                ino,
                DEFAULT_DIR_MODE,
                0,
                cpio_name.as_bytes(),
                &[],
            );
            ino = ino.wrapping_add(1);
        }

        // Write regular files
        for (path, entry) in &self.files {
            let cpio_name = path.trim_start_matches('/');
            write_cpio_entry(
                &mut out,
                ino,
                entry.mode,
                entry.mtime,
                cpio_name.as_bytes(),
                &entry.data,
            );
            ino = ino.wrapping_add(1);
        }

        // CPIO trailer record
        write_cpio_entry(&mut out, 0, 0, 0, b"TRAILER!!!", &[]);

        out
    }

    /// Decode a newc CPIO archive into a [`Rootfs`].
    pub fn from_cpio(bytes: &[u8]) -> Result<Self, RootfsError> {
        let mut fs = Self::new();
        let mut cursor = 0;

        while cursor + 110 <= bytes.len() {
            let magic = &bytes[cursor..cursor + 6];
            if magic != b"070701" {
                return Err(RootfsError::CorruptCpio(format!(
                    "unexpected magic at offset {cursor}: {:?}",
                    String::from_utf8_lossy(magic)
                )));
            }

            let mode = parse_hex_field(&bytes[cursor + 14..cursor + 22])?;
            let mtime = parse_hex_field(&bytes[cursor + 46..cursor + 54])?;
            let filesize = parse_hex_field(&bytes[cursor + 54..cursor + 62])? as usize;
            let namesize = parse_hex_field(&bytes[cursor + 94..cursor + 102])? as usize;

            cursor += 110;
            if cursor + namesize > bytes.len() {
                return Err(RootfsError::CorruptCpio(
                    "namesize overflows archive".to_string(),
                ));
            }

            let name_bytes = &bytes[cursor..cursor + namesize];
            let name_len = if !name_bytes.is_empty() && name_bytes[namesize - 1] == 0 {
                namesize - 1
            } else {
                namesize
            };
            let name_str = std::str::from_utf8(&name_bytes[..name_len]).map_err(|e| {
                RootfsError::CorruptCpio(format!("invalid UTF-8 entry name: {e}"))
            })?;

            if name_str == "TRAILER!!!" {
                break;
            }

            // Align cursor after name to 4 bytes
            let name_pad = (4 - (namesize % 4)) % 4;
            cursor += namesize + name_pad;

            if cursor + filesize > bytes.len() {
                return Err(RootfsError::CorruptCpio(
                    "filesize overflows archive".to_string(),
                ));
            }

            let file_data = bytes[cursor..cursor + filesize].to_vec();
            let data_pad = (4 - (filesize % 4)) % 4;
            cursor += filesize + data_pad;

            let clean_path = format!("/{name_str}");
            if (mode & S_IFDIR) != 0 {
                fs.create_dir_all(&clean_path)?;
            } else {
                if let Some(parent) = parent_dir(&clean_path) {
                    fs.create_dir_all(&parent)?;
                }
                fs.files.insert(
                    clean_path.clone(),
                    FileEntry {
                        path: clean_path,
                        mode,
                        mtime,
                        data: file_data,
                    },
                );
            }
        }

        Ok(fs)
    }
}

fn parent_dir(path: &str) -> Option<String> {
    let clean = path.trim_end_matches('/');
    clean.rfind('/').map(|idx| {
        if idx == 0 {
            "/".to_string()
        } else {
            clean[..idx].to_string()
        }
    })
}

fn validate_package_name(package: &str) -> Result<(), RootfsError> {
    if package.is_empty() || package.len() > 255 {
        return Err(RootfsError::InvalidPath(
            "package name length invalid".to_string(),
        ));
    }
    if !package
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(RootfsError::InvalidPath(format!(
            "invalid characters in package name: {package}"
        )));
    }
    if !package.contains('.') {
        return Err(RootfsError::InvalidPath(format!(
            "package name must contain at least one dot: {package}"
        )));
    }
    Ok(())
}

fn parse_hex_field(bytes: &[u8]) -> Result<u32, RootfsError> {
    let s = std::str::from_utf8(bytes).map_err(|e| {
        RootfsError::CorruptCpio(format!("invalid hex field: {e}"))
    })?;
    u32::from_str_radix(s, 16).map_err(|e| {
        RootfsError::CorruptCpio(format!("cannot parse hex string '{s}': {e}"))
    })
}

fn write_cpio_entry(
    out: &mut Vec<u8>,
    ino: u32,
    mode: u32,
    mtime: u32,
    name: &[u8],
    data: &[u8],
) {
    let namesize = name.len() + 1; // includes null terminator
    let filesize = data.len();

    // 110-byte newc header
    out.extend_from_slice(b"070701");
    out.extend_from_slice(format!("{ino:08X}").as_bytes());
    out.extend_from_slice(format!("{mode:08X}").as_bytes());
    out.extend_from_slice(b"00000000"); // uid
    out.extend_from_slice(b"00000000"); // gid
    out.extend_from_slice(b"00000001"); // nlink
    out.extend_from_slice(format!("{mtime:08X}").as_bytes());
    out.extend_from_slice(format!("{filesize:08X}").as_bytes());
    out.extend_from_slice(b"00000000"); // devmajor
    out.extend_from_slice(b"00000000"); // devminor
    out.extend_from_slice(b"00000000"); // rdevmajor
    out.extend_from_slice(b"00000000"); // rdevminor
    out.extend_from_slice(format!("{namesize:08X}").as_bytes());
    out.extend_from_slice(b"00000000"); // chksum

    // Name + null terminator
    out.extend_from_slice(name);
    out.push(0);
    let name_pad = (4 - (namesize % 4)) % 4;
    out.resize(out.len() + name_pad, 0);

    // Data
    if filesize > 0 {
        out.extend_from_slice(data);
        let data_pad = (4 - (filesize % 4)) % 4;
        out.resize(out.len() + data_pad, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rootfs_minimal_creates_expected_hierarchy() {
        let fs = Rootfs::new_minimal();
        assert!(fs.has_dir("/data"));
        assert!(fs.has_dir("/data/app"));
        assert!(fs.has_dir("/data/local/tmp"));
        assert!(fs.has_dir("/system/bin"));
        assert_eq!(fs.file_count(), 0);
    }

    #[test]
    fn rootfs_normalize_path_sanitizes_and_rejects_traversal() {
        assert_eq!(Rootfs::normalize_path("data/app").unwrap(), "/data/app");
        assert_eq!(Rootfs::normalize_path("/data//app/").unwrap(), "/data/app");
        assert_eq!(Rootfs::normalize_path("/").unwrap(), "/");
        assert!(Rootfs::normalize_path("../data").is_err());
        assert!(Rootfs::normalize_path("/data/../etc").is_err());
    }

    #[test]
    fn rootfs_file_write_read_and_listing() {
        let mut fs = Rootfs::new_minimal();
        fs.write_file("/data/hello.txt", b"hello world".to_vec(), DEFAULT_FILE_MODE)
            .unwrap();
        assert!(fs.has_file("/data/hello.txt"));
        assert_eq!(fs.get_file("/data/hello.txt"), Some(b"hello world".as_ref()));

        let children = fs.list_dir("/data").unwrap();
        assert!(children.contains(&"app".to_string()));
        assert!(children.contains(&"local".to_string()));
        assert!(children.contains(&"hello.txt".to_string()));
    }

    #[test]
    fn rootfs_sideload_apk_creates_canonical_path() {
        let mut fs = Rootfs::new_minimal();
        let apk_data = vec![0x50, 0x4B, 0x03, 0x04, 0x00];
        let path = fs
            .sideload_apk("com.example.cube", &apk_data)
            .expect("sideload should succeed");
        assert_eq!(path, "/data/app/com.example.cube/base.apk");
        assert!(fs.has_file(&path));
        assert_eq!(fs.get_file(&path), Some(apk_data.as_slice()));
    }

    #[test]
    fn rootfs_sideload_apk_rejects_bad_package_name() {
        let mut fs = Rootfs::new_minimal();
        assert!(fs.sideload_apk("nodots", b"bytes").is_err());
        assert!(fs.sideload_apk("../evil.pkg", b"bytes").is_err());
        assert!(fs.sideload_apk("pkg with spaces.app", b"bytes").is_err());
    }

    #[test]
    fn rootfs_cpio_roundtrip_is_lossless() {
        let mut fs = Rootfs::new_minimal();
        fs.write_file("/system/bin/sh", b"#!/bin/sh\n".to_vec(), DEFAULT_EXEC_MODE)
            .unwrap();
        fs.sideload_apk("com.example.app", b"sample apk payload")
            .unwrap();

        let cpio_bytes = fs.to_cpio();
        assert!(!cpio_bytes.is_empty());

        let restored = Rootfs::from_cpio(&cpio_bytes).expect("CPIO should restore cleanly");
        assert!(restored.has_dir("/data/app"));
        assert!(restored.has_file("/system/bin/sh"));
        assert!(restored.has_file("/data/app/com.example.app/base.apk"));
        assert_eq!(
            restored.get_file("/data/app/com.example.app/base.apk"),
            Some(b"sample apk payload".as_ref())
        );
    }

    #[test]
    fn rootfs_cpio_deterministic() {
        let build_fs = || {
            let mut fs = Rootfs::new_minimal();
            fs.write_file("/file1", b"content1".to_vec(), DEFAULT_FILE_MODE)
                .unwrap();
            fs.write_file("/file2", b"content2".to_vec(), DEFAULT_FILE_MODE)
                .unwrap();
            fs.to_cpio()
        };
        let c1 = build_fs();
        let c2 = build_fs();
        assert_eq!(c1, c2, "CPIO archive generation must be bit-identical");
    }
}
