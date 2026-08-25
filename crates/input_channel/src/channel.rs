//! Android `InputChannel` socketpair transport and memory channel abstraction.

use crate::error::{InputChannelError, Result};
use crate::message::{InputMessage, INPUT_MESSAGE_WIRE_SIZE};
use aidl_compat::{Parcel, Parcelable, Result as AidlResult, Status, STATUS_BAD_VALUE};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

#[cfg(unix)]
use std::os::unix::net::UnixDatagram;

/// Internal transport backend for `InputChannel`.
enum TransportBackend {
    #[cfg(unix)]
    Socket(UnixDatagram),
    Memory {
        send_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
        recv_queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    },
}

/// Global registry for resolving in-memory or cross-process input channel pairs by ID.
static CHANNEL_REGISTRY: RwLock<Option<HashMap<u64, InputChannel>>> = RwLock::new(None);
static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

fn ensure_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<u64, InputChannel>) -> R,
{
    let mut guard = CHANNEL_REGISTRY.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

/// Android `InputChannel` communicating binary input messages between publisher and consumer.
pub struct InputChannel {
    id: u64,
    name: String,
    backend: Arc<Mutex<TransportBackend>>,
}

impl Clone for InputChannel {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            backend: Arc::clone(&self.backend),
        }
    }
}

impl InputChannel {
    /// Create an in-memory input channel pair.
    pub fn create_memory_pair(name: &str) -> (Self, Self) {
        let id_a = NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst);
        let id_b = NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst);

        let q_a_to_b = Arc::new(Mutex::new(VecDeque::new()));
        let q_b_to_a = Arc::new(Mutex::new(VecDeque::new()));

        let chan_a = Self {
            id: id_a,
            name: format!("{}_server", name),
            backend: Arc::new(Mutex::new(TransportBackend::Memory {
                send_queue: Arc::clone(&q_a_to_b),
                recv_queue: Arc::clone(&q_b_to_a),
            })),
        };

        let chan_b = Self {
            id: id_b,
            name: format!("{}_client", name),
            backend: Arc::new(Mutex::new(TransportBackend::Memory {
                send_queue: Arc::clone(&q_b_to_a),
                recv_queue: Arc::clone(&q_a_to_b),
            })),
        };

        ensure_registry(|map| {
            map.insert(id_a, chan_a.clone());
            map.insert(id_b, chan_b.clone());
        });

        (chan_a, chan_b)
    }

    /// Open an input channel pair using Unix socketpair if on Unix, or memory pair otherwise.
    pub fn open_input_channel_pair(name: &str) -> Result<(Self, Self)> {
        #[cfg(unix)]
        {
            match UnixDatagram::pair() {
                Ok((sock_a, sock_b)) => {
                    sock_a.set_nonblocking(true)?;
                    sock_b.set_nonblocking(true)?;

                    let id_a = NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst);
                    let id_b = NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst);

                    let chan_a = Self {
                        id: id_a,
                        name: format!("{}_server", name),
                        backend: Arc::new(Mutex::new(TransportBackend::Socket(sock_a))),
                    };

                    let chan_b = Self {
                        id: id_b,
                        name: format!("{}_client", name),
                        backend: Arc::new(Mutex::new(TransportBackend::Socket(sock_b))),
                    };

                    ensure_registry(|map| {
                        map.insert(id_a, chan_a.clone());
                        map.insert(id_b, chan_b.clone());
                    });

                    Ok((chan_a, chan_b))
                }
                Err(_) => Ok(Self::create_memory_pair(name)),
            }
        }

        #[cfg(not(unix))]
        {
            Ok(Self::create_memory_pair(name))
        }
    }

    /// Access unique channel ID.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Access human-readable channel name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Send an `InputMessage` across the transport.
    pub fn send_message(&self, msg: &InputMessage) -> Result<()> {
        let mut buf = [0u8; INPUT_MESSAGE_WIRE_SIZE];
        msg.encode(&mut buf);

        let backend = self.backend.lock().unwrap();
        match &*backend {
            #[cfg(unix)]
            TransportBackend::Socket(sock) => {
                sock.send(&buf)?;
                Ok(())
            }
            TransportBackend::Memory { send_queue, .. } => {
                let mut queue = send_queue.lock().unwrap();
                queue.push_back(buf.to_vec());
                Ok(())
            }
        }
    }

    /// Receive an `InputMessage` across the transport (blocking / loop until available or timeout).
    pub fn receive_message(&self) -> Result<InputMessage> {
        let backend = self.backend.lock().unwrap();
        match &*backend {
            #[cfg(unix)]
            TransportBackend::Socket(sock) => {
                let mut buf = [0u8; INPUT_MESSAGE_WIRE_SIZE];
                // Loop with small yield if nonblocking
                loop {
                    match sock.recv(&mut buf) {
                        Ok(n) => {
                            return InputMessage::decode(&buf[..n]);
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::yield_now();
                        }
                        Err(e) => return Err(InputChannelError::Io(e)),
                    }
                }
            }
            TransportBackend::Memory { recv_queue, .. } => loop {
                {
                    let mut queue = recv_queue.lock().unwrap();
                    if let Some(bytes) = queue.pop_front() {
                        return InputMessage::decode(&bytes);
                    }
                }
                std::thread::yield_now();
            },
        }
    }

    /// Try to receive an `InputMessage` without blocking.
    pub fn try_receive_message(&self) -> Result<Option<InputMessage>> {
        let backend = self.backend.lock().unwrap();
        match &*backend {
            #[cfg(unix)]
            TransportBackend::Socket(sock) => {
                let mut buf = [0u8; INPUT_MESSAGE_WIRE_SIZE];
                match sock.recv(&mut buf) {
                    Ok(n) => {
                        let msg = InputMessage::decode(&buf[..n])?;
                        Ok(Some(msg))
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                    Err(e) => Err(InputChannelError::Io(e)),
                }
            }
            TransportBackend::Memory { recv_queue, .. } => {
                let mut queue = recv_queue.lock().unwrap();
                if let Some(bytes) = queue.pop_front() {
                    let msg = InputMessage::decode(&bytes)?;
                    Ok(Some(msg))
                } else {
                    Ok(None)
                }
            }
        }
    }
}

impl Default for InputChannel {
    fn default() -> Self {
        let (server, _client) = Self::create_memory_pair("default");
        server
    }
}

impl Parcelable for InputChannel {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u64(self.id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_utf8(Some(&self.name)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        let id = parcel.read_u64(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let name = parcel.read_utf8(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_else(|| "unnamed_channel".to_string());

        let existing = CHANNEL_REGISTRY
            .read()
            .unwrap()
            .as_ref()
            .and_then(|map| map.get(&id).cloned());

        if let Some(channel) = existing {
            *self = channel;
        } else {
            let (chan, _) = Self::create_memory_pair(&name);
            *self = chan;
            self.id = id;
            self.name = name;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{FinishedData, KeyEventData};

    #[test]
    fn test_input_channel_memory_pair() {
        let (server, client) = InputChannel::create_memory_pair("test_pair");
        assert_eq!(server.name(), "test_pair_server");
        assert_eq!(client.name(), "test_pair_client");

        let key = KeyEventData {
            seq: 1,
            key_code: 4, // KEYCODE_BACK
            ..Default::default()
        };
        server.send_message(&InputMessage::Key(key)).unwrap();

        let received = client.receive_message().unwrap();
        if let InputMessage::Key(k) = received {
            assert_eq!(k.seq, 1);
            assert_eq!(k.key_code, 4);
        } else {
            panic!("Expected Key message");
        }

        // Client replies with finished signal
        let fin = FinishedData::new(1, true);
        client.send_message(&InputMessage::Finished(fin)).unwrap();

        let ack = server.receive_message().unwrap();
        if let InputMessage::Finished(f) = ack {
            assert_eq!(f.seq, 1);
            assert!(f.handled);
        } else {
            panic!("Expected Finished message");
        }
    }

    #[test]
    fn test_input_channel_socket_pair() {
        let (server, client) = InputChannel::open_input_channel_pair("sock_test").unwrap();

        let key = KeyEventData {
            seq: 99,
            key_code: 66, // KEYCODE_ENTER
            ..Default::default()
        };
        server.send_message(&InputMessage::Key(key)).unwrap();

        let received = client.receive_message().unwrap();
        if let InputMessage::Key(k) = received {
            assert_eq!(k.seq, 99);
            assert_eq!(k.key_code, 66);
        } else {
            panic!("Expected Key message");
        }
    }

    #[test]
    fn test_input_channel_parcelable() {
        let (server, _client) = InputChannel::create_memory_pair("parcel_test");
        let mut p = Parcel::new();
        server.write_to_parcel(&mut p).unwrap();

        let mut restored = InputChannel::default();
        let mut offset = 0;
        restored.read_from_parcel_at(&p, &mut offset).unwrap();

        assert_eq!(restored.id(), server.id());
        assert_eq!(restored.name(), server.name());
    }
}
