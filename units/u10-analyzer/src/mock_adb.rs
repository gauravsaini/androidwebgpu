//! Mock ADB server implementation (device side).
//!
//! Provides a pure in-memory ADB daemon state machine that terminates ADB client
//! connections over [`NetSocket`], executes the file sync sub-protocol, and writes
//! files into an [`AdbTargetFs`] (such as `guest_image::Rootfs`).
//!
//! Purity: quarantine boundary. Pure state machine over abstract socket and filesystem.
//! Zero threads, zero clock calls, zero OS network sockets.

use crate::sideload::{
    AdbMessage, SideloadError, A_CLSE, A_CNXN, A_OKAY, A_OPEN, A_VERSION, A_WRTE, ID_DATA, ID_DONE,
    ID_FAIL, ID_OKAY, ID_SEND, MAX_ADB_PAYLOAD,
};
use pathn_contracts::adapters::NetSocket;

/// Abstract target filesystem interface consumed by the mock ADB server.
///
/// Decouples ADB protocol handling from any specific filesystem crate.
pub trait AdbTargetFs {
    fn write_file(&mut self, path: &str, data: Vec<u8>, mode: u32) -> Result<(), String>;
    fn file_exists(&self, path: &str) -> bool;
    fn read_file(&self, path: &str) -> Option<&[u8]>;
}

/// In-memory active sync session state.
#[derive(Debug, Default)]
struct SyncSession {
    local_id: u32,
    remote_id: u32,
    dest_path: String,
    dest_mode: u32,
    data_buffer: Vec<u8>,
}

/// In-memory mock ADB server / daemon.
pub struct MockAdbServer<F: AdbTargetFs> {
    target_fs: F,
    banner: String,
    connected: bool,
    rx_buf: Vec<u8>,
    session: Option<SyncSession>,
    next_stream_id: u32,
}

impl<F: AdbTargetFs> MockAdbServer<F> {
    /// Create a new mock ADB daemon wrapping a target filesystem.
    pub fn new(target_fs: F) -> Self {
        Self {
            target_fs,
            banner: "device::ro.product.model=PathN_ARM64;ro.product.device=pathn".to_string(),
            connected: false,
            rx_buf: Vec::new(),
            session: None,
            next_stream_id: 100,
        }
    }

    /// Access the underlying target filesystem.
    pub fn target_fs(&self) -> &F {
        &self.target_fs
    }

    /// Mutably access the underlying target filesystem.
    pub fn target_fs_mut(&mut self) -> &mut F {
        &mut self.target_fs
    }

    /// True if an ADB client has connected.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Pump messages from `socket`, process all pending requests, and send responses.
    /// Returns the number of ADB messages processed this step.
    pub fn step(&mut self, socket: &mut dyn NetSocket) -> Result<usize, SideloadError> {
        while let Some(chunk) = socket.recv() {
            self.rx_buf.extend_from_slice(&chunk);
        }

        let mut processed = 0;
        while let Some((msg, consumed)) = AdbMessage::decode(&self.rx_buf)? {
            self.rx_buf.drain(0..consumed);
            self.handle_message(socket, msg)?;
            processed += 1;
        }

        Ok(processed)
    }

    fn handle_message(
        &mut self,
        socket: &mut dyn NetSocket,
        msg: AdbMessage,
    ) -> Result<(), SideloadError> {
        match msg.command {
            A_CNXN => {
                self.connected = true;
                let reply = AdbMessage::new(
                    A_CNXN,
                    A_VERSION,
                    MAX_ADB_PAYLOAD,
                    self.banner.as_bytes().to_vec(),
                );
                socket.send(&reply.encode());
            }
            A_OPEN => {
                let service = String::from_utf8_lossy(&msg.payload)
                    .trim_matches('\0')
                    .to_string();
                if service != "sync:" {
                    // Reject unsupported service with CLSE
                    let reply = AdbMessage::new(A_CLSE, 0, msg.arg0, Vec::new());
                    socket.send(&reply.encode());
                    return Ok(());
                }

                let local_id = self.next_stream_id;
                self.next_stream_id = self.next_stream_id.wrapping_add(1);
                let remote_id = msg.arg0;

                self.session = Some(SyncSession {
                    local_id,
                    remote_id,
                    dest_path: String::new(),
                    dest_mode: 0o100644,
                    data_buffer: Vec::new(),
                });

                let reply = AdbMessage::new(A_OKAY, local_id, remote_id, Vec::new());
                socket.send(&reply.encode());
            }
            A_WRTE => {
                let remote_id = msg.arg0;
                let local_id = msg.arg1;

                if let Some(session) = &mut self.session {
                    if session.local_id == local_id && session.remote_id == remote_id {
                        let payload = &msg.payload;
                        let mut sync_resp: Option<Vec<u8>> = None;

                        if payload.len() >= 8 {
                            let tag = &payload[0..4];
                            let length =
                                u32::from_le_bytes(payload[4..8].try_into().unwrap()) as usize;

                            if tag == &ID_SEND {
                                let spec_bytes = &payload[8..8 + length.min(payload.len() - 8)];
                                let spec = String::from_utf8_lossy(spec_bytes).to_string();
                                let mut parts = spec.split(',');
                                session.dest_path = parts.next().unwrap_or("").to_string();
                                if let Some(mode_str) = parts.next() {
                                    session.dest_mode = u32::from_str_radix(mode_str, 8)
                                        .unwrap_or(0o100644);
                                }
                            } else if tag == &ID_DATA {
                                let data_start = 8;
                                let data_end = 8 + length.min(payload.len() - 8);
                                session
                                    .data_buffer
                                    .extend_from_slice(&payload[data_start..data_end]);
                            } else if tag == &ID_DONE {
                                // Finalize transfer: write to target filesystem
                                let write_res = self.target_fs.write_file(
                                    &session.dest_path,
                                    std::mem::take(&mut session.data_buffer),
                                    session.dest_mode,
                                );
                                match write_res {
                                    Ok(()) => {
                                        let mut ok = Vec::with_capacity(8);
                                        ok.extend_from_slice(&ID_OKAY);
                                        ok.extend_from_slice(&0u32.to_le_bytes());
                                        sync_resp = Some(ok);
                                    }
                                    Err(e) => {
                                        let mut fail = Vec::with_capacity(8 + e.len());
                                        fail.extend_from_slice(&ID_FAIL);
                                        fail.extend_from_slice(&(e.len() as u32).to_le_bytes());
                                        fail.extend_from_slice(e.as_bytes());
                                        sync_resp = Some(fail);
                                    }
                                }
                            }
                        }

                        // Flow control: acknowledge client WRTE
                        let ack = AdbMessage::new(A_OKAY, local_id, remote_id, Vec::new());
                        socket.send(&ack.encode());

                        // Send sync protocol reply if generated (on DONE)
                        if let Some(resp_payload) = sync_resp {
                            let resp_msg =
                                AdbMessage::new(A_WRTE, local_id, remote_id, resp_payload);
                            socket.send(&resp_msg.encode());
                        }
                    }
                }
            }
            A_CLSE => {
                let remote_id = msg.arg0;
                let local_id = msg.arg1;
                if let Some(session) = &self.session {
                    if session.local_id == local_id && session.remote_id == remote_id {
                        self.session = None;
                    }
                }
                let reply = AdbMessage::new(A_CLSE, local_id, remote_id, Vec::new());
                socket.send(&reply.encode());
            }
            _ => {}
        }
        Ok(())
    }
}
