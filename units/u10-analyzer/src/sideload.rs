//! U10 sideload — ADB wire protocol client and APK installer (QUARANTINED).
//!
//! Implements the ADB framing protocol and sync-service transfer protocol over
//! the frozen [`NetSocket`] contract trait.
//!
//! Input: `&ApkMeta`, raw `&[u8]` APK bytes, and an [`AdbChannel`] connected to
//! an ADB socket.
//! Output: [`InstallResult`] recording package name, guest target path, and bytes installed.
//!
//! Purity: quarantine boundary. No OS network or filesystem I/O. All communication
//! routes through [`NetSocket`].

use pathn_contracts::adapters::NetSocket;
use pathn_contracts::machine::{ApkError, ApkMeta};

// ---------------------------------------------------------------------------
// ADB Wire Protocol Constants
// ---------------------------------------------------------------------------

pub const A_SYNC: u32 = 0x434e5953;
pub const A_CNXN: u32 = 0x4e584e43;
pub const A_OPEN: u32 = 0x4e45504f;
pub const A_OKAY: u32 = 0x59414b4f;
pub const A_CLSE: u32 = 0x45534c43;
pub const A_WRTE: u32 = 0x45545257;

pub const A_VERSION: u32 = 0x01000001;
pub const MAX_ADB_PAYLOAD: u32 = 256 * 1024; // 256 KiB chunk ceiling

// Sync sub-protocol IDs (4-byte identifiers)
pub const ID_SEND: [u8; 4] = *b"SEND";
pub const ID_DATA: [u8; 4] = *b"DATA";
pub const ID_DONE: [u8; 4] = *b"DONE";
pub const ID_OKAY: [u8; 4] = *b"OKAY";
pub const ID_FAIL: [u8; 4] = *b"FAIL";

/// Size of the standard ADB message header.
pub const ADB_HEADER_LEN: usize = 24;

// ---------------------------------------------------------------------------
// Error and Result Types
// ---------------------------------------------------------------------------

/// Sideload failure modes. Data, never panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideloadError {
    AnalyzerError(ApkError),
    ProtocolError(String),
    InstallFailed(String),
    ConnectionClosed,
    InvalidPath(String),
}

impl std::fmt::Display for SideloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SideloadError::AnalyzerError(e) => write!(f, "analyzer error: {e:?}"),
            SideloadError::ProtocolError(m) => write!(f, "ADB protocol error: {m}"),
            SideloadError::InstallFailed(m) => write!(f, "installation failed: {m}"),
            SideloadError::ConnectionClosed => write!(f, "ADB connection closed unexpectedly"),
            SideloadError::InvalidPath(p) => write!(f, "invalid target path: {p}"),
        }
    }
}

impl std::error::Error for SideloadError {}

/// Sideload installation summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallResult {
    pub package: String,
    pub apk_path: String,
    pub bytes_installed: usize,
}

// ---------------------------------------------------------------------------
// ADB Message Framing
// ---------------------------------------------------------------------------

/// One 24-byte header + variable payload ADB wire packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbMessage {
    pub command: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub data_length: u32,
    pub data_crc32: u32,
    pub magic: u32,
    pub payload: Vec<u8>,
}

impl AdbMessage {
    /// Construct a valid ADB message, computing length, magic, and crc32.
    pub fn new(command: u32, arg0: u32, arg1: u32, payload: Vec<u8>) -> Self {
        let data_length = payload.len() as u32;
        let data_crc32 = adb_crc32(&payload);
        let magic = command ^ 0xFFFF_FFFF;
        Self {
            command,
            arg0,
            arg1,
            data_length,
            data_crc32,
            magic,
            payload,
        }
    }

    /// Serialize message to wire bytes (24 bytes header + payload).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ADB_HEADER_LEN + self.payload.len());
        out.extend_from_slice(&self.command.to_le_bytes());
        out.extend_from_slice(&self.arg0.to_le_bytes());
        out.extend_from_slice(&self.arg1.to_le_bytes());
        out.extend_from_slice(&self.data_length.to_le_bytes());
        out.extend_from_slice(&self.data_crc32.to_le_bytes());
        out.extend_from_slice(&self.magic.to_le_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Attempt to decode one message from a buffer.
    /// Returns `Ok(Some((message, bytes_consumed)))` if a complete message was decoded,
    /// `Ok(None)` if more bytes are needed, or `Err` if the header is corrupt.
    pub fn decode(buf: &[u8]) -> Result<Option<(Self, usize)>, SideloadError> {
        if buf.len() < ADB_HEADER_LEN {
            return Ok(None);
        }
        let command = u32::from_le_bytes(buf[0..4].try_into().unwrap());
        let arg0 = u32::from_le_bytes(buf[4..8].try_into().unwrap());
        let arg1 = u32::from_le_bytes(buf[8..12].try_into().unwrap());
        let data_length = u32::from_le_bytes(buf[12..16].try_into().unwrap());
        let data_crc32 = u32::from_le_bytes(buf[16..20].try_into().unwrap());
        let magic = u32::from_le_bytes(buf[20..24].try_into().unwrap());

        if magic != (command ^ 0xFFFF_FFFF) {
            return Err(SideloadError::ProtocolError(format!(
                "invalid ADB magic: cmd=0x{command:08x}, magic=0x{magic:08x}"
            )));
        }

        let needed = ADB_HEADER_LEN + data_length as usize;
        if buf.len() < needed {
            return Ok(None);
        }

        let payload = buf[ADB_HEADER_LEN..needed].to_vec();
        let computed_crc = adb_crc32(&payload);
        if computed_crc != data_crc32 {
            return Err(SideloadError::ProtocolError(format!(
                "CRC32 mismatch: expected 0x{data_crc32:08x}, got 0x{computed_crc:08x}"
            )));
        }

        Ok(Some((
            Self {
                command,
                arg0,
                arg1,
                data_length,
                data_crc32,
                magic,
                payload,
            },
            needed,
        )))
    }
}

/// In ADB protocol, crc32 is a simple unsigned sum of all bytes in the payload.
pub fn adb_crc32(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for &b in data {
        sum = sum.wrapping_add(b as u32);
    }
    sum
}

// ---------------------------------------------------------------------------
// AdbChannel — Client side
// ---------------------------------------------------------------------------

/// Manages an ADB client session over an abstract [`NetSocket`].
pub struct AdbChannel<'a> {
    socket: &'a mut dyn NetSocket,
    rx_buf: Vec<u8>,
    connected: bool,
    next_local_id: u32,
    peer_stepper: Option<Box<dyn FnMut() + 'a>>,
}

impl<'a> AdbChannel<'a> {
    pub fn new(socket: &'a mut dyn NetSocket) -> Self {
        Self {
            socket,
            rx_buf: Vec::new(),
            connected: false,
            next_local_id: 1,
            peer_stepper: None,
        }
    }

    /// Set an optional peer stepper callback to pump mock servers synchronously.
    pub fn set_peer_stepper<F: FnMut() + 'a>(&mut self, stepper: F) {
        self.peer_stepper = Some(Box::new(stepper));
    }

    /// Read incoming socket datagrams into `rx_buf`.
    pub fn pump_rx(&mut self) {
        while let Some(chunk) = self.socket.recv() {
            self.rx_buf.extend_from_slice(&chunk);
        }
    }

    /// Send an ADB message immediately over the socket.
    pub fn send_message(&mut self, msg: &AdbMessage) {
        self.socket.send(&msg.encode());
    }

    /// Try to read the next complete message from `rx_buf`.
    pub fn next_message(&mut self) -> Result<Option<AdbMessage>, SideloadError> {
        self.pump_rx();
        if let Some((msg, consumed)) = AdbMessage::decode(&self.rx_buf)? {
            self.rx_buf.drain(0..consumed);
            Ok(Some(msg))
        } else {
            Ok(None)
        }
    }

    /// Read next message, polling peer_stepper if waiting on a mock peer.
    pub fn expect_message(&mut self) -> Result<AdbMessage, SideloadError> {
        for _ in 0..100 {
            if let Some(msg) = self.next_message()? {
                return Ok(msg);
            }
            if let Some(stepper) = &mut self.peer_stepper {
                stepper();
            } else {
                break;
            }
        }
        self.next_message()?
            .ok_or(SideloadError::ConnectionClosed)
    }

    /// Perform initial ADB connection handshake (`CNXN`).
    pub fn connect(&mut self, banner: &str) -> Result<String, SideloadError> {
        let msg = AdbMessage::new(
            A_CNXN,
            A_VERSION,
            MAX_ADB_PAYLOAD,
            banner.as_bytes().to_vec(),
        );
        self.send_message(&msg);
        let resp = self.expect_message()?;
        if resp.command != A_CNXN {
            return Err(SideloadError::ProtocolError(format!(
                "expected CNXN, got 0x{:08X}",
                resp.command
            )));
        }
        self.connected = true;
        let peer_banner = String::from_utf8_lossy(&resp.payload).to_string();
        Ok(peer_banner)
    }

    /// Open a service stream (e.g. `"sync:"`). Returns `(local_id, remote_id)`.
    pub fn open_service(&mut self, service: &str) -> Result<(u32, u32), SideloadError> {
        let local_id = self.next_local_id;
        self.next_local_id = self.next_local_id.wrapping_add(1);

        let mut payload = service.as_bytes().to_vec();
        if !payload.ends_with(&[0]) {
            payload.push(0);
        }

        let msg = AdbMessage::new(A_OPEN, local_id, 0, payload);
        self.send_message(&msg);

        let resp = self.expect_message()?;
        if resp.command != A_OKAY || resp.arg1 != local_id {
            return Err(SideloadError::ProtocolError(format!(
                "open service failed: expected OKAY for id {local_id}, got 0x{:08X}",
                resp.command
            )));
        }
        let remote_id = resp.arg0;
        Ok((local_id, remote_id))
    }

    /// Write data payload into an open stream.
    pub fn write_stream(
        &mut self,
        local_id: u32,
        remote_id: u32,
        payload: Vec<u8>,
    ) -> Result<(), SideloadError> {
        let msg = AdbMessage::new(A_WRTE, local_id, remote_id, payload);
        self.send_message(&msg);
        let resp = self.expect_message()?;
        if resp.command != A_OKAY || resp.arg1 != local_id {
            return Err(SideloadError::ProtocolError(format!(
                "expected OKAY after WRTE, got 0x{:08X}",
                resp.command
            )));
        }
        Ok(())
    }

    /// Close an open stream.
    pub fn close_stream(
        &mut self,
        local_id: u32,
        remote_id: u32,
    ) -> Result<(), SideloadError> {
        let msg = AdbMessage::new(A_CLSE, local_id, remote_id, Vec::new());
        self.send_message(&msg);
        let resp = self.expect_message()?;
        if resp.command != A_CLSE && resp.command != A_OKAY {
            return Err(SideloadError::ProtocolError(format!(
                "expected CLSE/OKAY, got 0x{:08X}",
                resp.command
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// High-Level Sideload Flow
// ---------------------------------------------------------------------------

/// Sideload an analyzed APK into the guest rootfs via an active ADB channel.
///
/// Steps:
/// 1. Connects with ADB handshake (`CNXN`).
/// 2. Opens the `sync:` stream service.
/// 3. Sends sync `SEND` command pointing to `/data/app/<package>/base.apk,0100644`.
/// 4. Chunks and streams `DATA` blocks (up to 64 KiB per block).
/// 5. Sends sync `DONE` command and reads device acknowledgment.
/// 6. Closes the sync stream (`CLSE`).
/// 7. Returns [`InstallResult`].
pub fn sideload_apk(
    meta: &ApkMeta,
    apk_bytes: &[u8],
    channel: &mut AdbChannel<'_>,
) -> Result<InstallResult, SideloadError> {
    if apk_bytes.is_empty() {
        return Err(SideloadError::InstallFailed("APK bytes are empty".to_string()));
    }
    if meta.package.is_empty() {
        return Err(SideloadError::InstallFailed(
            "package name is empty".to_string(),
        ));
    }

    // Handshake
    channel.connect("host::pathn-sideload-client")?;

    // Open sync service
    let (local_id, remote_id) = channel.open_service("sync:")?;

    // Target path in guest rootfs
    let target_path = format!("/data/app/{}/base.apk", meta.package);
    let send_spec = format!("{target_path},0100644");
    let send_spec_bytes = send_spec.as_bytes();

    // 1. Send SYNC SEND header
    let mut send_cmd = Vec::with_capacity(8 + send_spec_bytes.len());
    send_cmd.extend_from_slice(&ID_SEND);
    send_cmd.extend_from_slice(&(send_spec_bytes.len() as u32).to_le_bytes());
    send_cmd.extend_from_slice(send_spec_bytes);
    channel.write_stream(local_id, remote_id, send_cmd)?;

    // 2. Stream DATA chunks (64 KiB chunks)
    const CHUNK_SIZE: usize = 64 * 1024;
    for chunk in apk_bytes.chunks(CHUNK_SIZE) {
        let mut data_cmd = Vec::with_capacity(8 + chunk.len());
        data_cmd.extend_from_slice(&ID_DATA);
        data_cmd.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        data_cmd.extend_from_slice(chunk);
        channel.write_stream(local_id, remote_id, data_cmd)?;
    }

    // 3. Send DONE command
    let mut done_cmd = Vec::with_capacity(8);
    done_cmd.extend_from_slice(&ID_DONE);
    done_cmd.extend_from_slice(&0u32.to_le_bytes()); // mtime 0
    channel.write_stream(local_id, remote_id, done_cmd)?;

    // 4. Expect SYNC result from device
    let resp = channel.expect_message()?;
    if resp.command != A_WRTE {
        return Err(SideloadError::ProtocolError(format!(
            "expected WRTE sync response, got 0x{:08X}",
            resp.command
        )));
    }
    // Acknowledge WRTE with OKAY
    channel.send_message(&AdbMessage::new(A_OKAY, local_id, remote_id, Vec::new()));

    if resp.payload.len() < 4 {
        return Err(SideloadError::ProtocolError(
            "sync response too short".to_string(),
        ));
    }
    let status_id = &resp.payload[0..4];
    if status_id == &ID_FAIL {
        let msg_len = if resp.payload.len() >= 8 {
            u32::from_le_bytes(resp.payload[4..8].try_into().unwrap()) as usize
        } else {
            resp.payload.len() - 4
        };
        let start = 8.min(resp.payload.len());
        let end = (start + msg_len).min(resp.payload.len());
        let err_msg = String::from_utf8_lossy(&resp.payload[start..end]).to_string();
        return Err(SideloadError::InstallFailed(err_msg));
    } else if status_id != &ID_OKAY {
        return Err(SideloadError::ProtocolError(format!(
            "unknown sync status: {:?}",
            String::from_utf8_lossy(status_id)
        )));
    }

    // Close stream
    channel.close_stream(local_id, remote_id)?;

    Ok(InstallResult {
        package: meta.package.clone(),
        apk_path: target_path,
        bytes_installed: apk_bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_message_encode_decode_roundtrip() {
        let original = AdbMessage::new(
            A_CNXN,
            A_VERSION,
            MAX_ADB_PAYLOAD,
            b"host::antigravity".to_vec(),
        );
        let wire = original.encode();
        assert_eq!(wire.len(), ADB_HEADER_LEN + original.payload.len());

        let (decoded, consumed) = AdbMessage::decode(&wire)
            .expect("decode must succeed")
            .expect("complete message");
        assert_eq!(consumed, wire.len());
        assert_eq!(decoded, original);
    }

    #[test]
    fn adb_message_decode_rejects_corrupt_magic() {
        let original = AdbMessage::new(A_CNXN, 1, 2, vec![1, 2, 3]);
        let mut wire = original.encode();
        wire[20] ^= 0xFF; // corrupt magic
        assert!(AdbMessage::decode(&wire).is_err());
    }

    #[test]
    fn adb_message_decode_rejects_crc_mismatch() {
        let original = AdbMessage::new(A_CNXN, 1, 2, vec![1, 2, 3]);
        let mut wire = original.encode();
        wire[16] ^= 0xFF; // corrupt crc32
        assert!(AdbMessage::decode(&wire).is_err());
    }

    #[test]
    fn adb_message_partial_decode_returns_none() {
        let original = AdbMessage::new(A_CNXN, 1, 2, vec![1, 2, 3]);
        let wire = original.encode();
        // Truncate to less than header
        assert_eq!(AdbMessage::decode(&wire[..10]).unwrap(), None);
        // Truncate payload
        assert_eq!(AdbMessage::decode(&wire[..wire.len() - 1]).unwrap(), None);
    }
}
