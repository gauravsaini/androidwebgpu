//! U18 `virtio-console` — OASIS VirtIO Console Device Model (MMIO transport).
//!
//! Purity: EXPLICIT-STATE. All state lives inside [`VirtioConsole`].
//! Performs no I/O, no threading, no time reads — fully deterministic.
//!
//! # Architecture Reference
//! - OASIS Virtual I/O Device (VIRTIO) Version 1.2, Section 5.3 (Console Device)
//! - Device ID: 3 (`VIRTIO_ID_CONSOLE`)
//! - Queue 0: Receive (RX) queue (host -> guest)
//! - Queue 1: Transmit (TX) queue (guest -> host)
//!
//! Default QEMU virt MMIO slot: `0x0A00_0000`, size `0x200`.

use std::collections::VecDeque;
use u6_virtio_transport::{
    pop_chain, push_used, MAGIC_VALUE, MMIO_DEVICE_CONFIG, MMIO_DEVICE_FEATURES,
    MMIO_DEVICE_ID, MMIO_DRIVER_FEATURES, MMIO_MAGIC,
    MMIO_QUEUE_AVAIL_HI, MMIO_QUEUE_AVAIL_LO, MMIO_QUEUE_DESC_HI, MMIO_QUEUE_DESC_LO,
    MMIO_QUEUE_NOTIFY, MMIO_QUEUE_NUM, MMIO_QUEUE_NUM_MAX, MMIO_QUEUE_READY, MMIO_QUEUE_SEL,
    MMIO_QUEUE_USED_HI, MMIO_QUEUE_USED_LO, MMIO_STATUS, MMIO_VENDOR_ID, MMIO_VERSION,
    MMIO_VERSION_VALUE, VENDOR_ID_VALUE, VIRTIO_F_VERSION_1, VIRTIO_STATUS_DRIVER_OK,
    VIRTQ_MAX_SIZE,
};

pub const MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const MMIO_INTERRUPT_ACK: u64 = 0x064;

pub const VIRTIO_CONSOLE_BASE: u64 = 0x0A00_0000;
pub const VIRTIO_CONSOLE_SIZE: u64 = 0x200;
pub const VIRTIO_CONSOLE_DEVICE_ID: u32 = 3;
pub const VIRTIO_CONSOLE_IRQ: u32 = 48; // SPI 16 = 32 + 16

pub const NUM_QUEUES: usize = 2;
pub const QUEUE_RX: usize = 0;
pub const QUEUE_TX: usize = 1;

/// Interrupt status bits (OASIS §4.2.2.2)
pub const VIRTIO_MMIO_INT_VRING: u32 = 1 << 0;
pub const VIRTIO_MMIO_INT_CONFIG: u32 = 1 << 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueState {
    pub num_max: u32,
    pub num: u32,
    pub ready: bool,
    pub desc_addr: u64,
    pub avail_addr: u64,
    pub used_addr: u64,
    pub last_avail_idx: u16,
    pub last_used_idx: u16,
}

impl Default for QueueState {
    fn default() -> Self {
        Self {
            num_max: VIRTQ_MAX_SIZE as u32,
            num: 0,
            ready: false,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            last_avail_idx: 0,
            last_used_idx: 0,
        }
    }
}

/// Explicit state of the VirtIO Console device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioConsole {
    pub status: u8,
    pub queue_sel: u32,
    pub queues: [QueueState; NUM_QUEUES],
    pub driver_features: u64,
    pub interrupt_status: u32,
    pub irq_line: bool,
    pub irq_num: u32,

    // Config space
    pub cols: u16,
    pub rows: u16,
    pub max_nr_ports: u32,

    // Console buffers
    pub tx_log: Vec<u8>,
    pub rx_fifo: VecDeque<u8>,
}

impl Default for VirtioConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtioConsole {
    pub fn new() -> Self {
        Self {
            status: 0,
            queue_sel: 0,
            queues: [QueueState::default(), QueueState::default()],
            driver_features: 0,
            interrupt_status: 0,
            irq_line: false,
            irq_num: VIRTIO_CONSOLE_IRQ,

            cols: 80,
            rows: 24,
            max_nr_ports: 1,

            tx_log: Vec::new(),
            rx_fifo: VecDeque::new(),
        }
    }

    // -----------------------------------------------------------------------
    // MMIO Registers Interface
    // -----------------------------------------------------------------------

    pub fn mmio_read(&self, offset: u64, size: usize) -> u32 {
        let val32 = match offset {
            MMIO_MAGIC => MAGIC_VALUE as u32,
            MMIO_VERSION => MMIO_VERSION_VALUE as u32,
            MMIO_DEVICE_ID => VIRTIO_CONSOLE_DEVICE_ID,
            MMIO_VENDOR_ID => VENDOR_ID_VALUE as u32,
            MMIO_DEVICE_FEATURES => {
                // Return lower or upper 32 bits based on feature selector (default 0)
                (VIRTIO_F_VERSION_1 & 0xFFFF_FFFF) as u32
            }
            0x014 => (VIRTIO_F_VERSION_1 >> 32) as u32,
            MMIO_QUEUE_NUM_MAX => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].num_max
                } else {
                    0
                }
            }
            MMIO_QUEUE_READY => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES && self.queues[q].ready {
                    1
                } else {
                    0
                }
            }
            MMIO_INTERRUPT_STATUS => self.interrupt_status,
            MMIO_STATUS => self.status as u32,

            // Device configuration space (0x100..)
            off if off >= MMIO_DEVICE_CONFIG => {
                let cfg_off = off - MMIO_DEVICE_CONFIG;
                match cfg_off {
                    0x00 => (self.cols as u32) | ((self.rows as u32) << 16),
                    0x04 => self.max_nr_ports,
                    _ => 0,
                }
            }
            _ => 0,
        };

        match size {
            1 => val32 & 0xFF,
            2 => val32 & 0xFFFF,
            4 => val32,
            _ => 0,
        }
    }

    pub fn mmio_write(&mut self, offset: u64, val: u32, _size: usize, ram: &mut [u8]) {
        match offset {
            MMIO_DEVICE_FEATURES => {} // read-only
            MMIO_DRIVER_FEATURES => {
                self.driver_features = (self.driver_features & 0xFFFF_FFFF_0000_0000) | (val as u64);
            }
            0x024 => {
                self.driver_features = (self.driver_features & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
            }
            MMIO_QUEUE_SEL => {
                self.queue_sel = val;
            }
            MMIO_QUEUE_NUM => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].num = val;
                }
            }
            MMIO_QUEUE_READY => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].ready = (val & 1) != 0;
                }
            }
            MMIO_QUEUE_NOTIFY => {
                let q = val as usize;
                if q < NUM_QUEUES {
                    self.process_queue(q, ram);
                }
            }
            MMIO_INTERRUPT_ACK => {
                self.interrupt_status &= !val;
                if self.interrupt_status == 0 {
                    self.irq_line = false;
                }
            }
            MMIO_STATUS => {
                self.status = val as u8;
                if self.status == 0 {
                    // Reset device
                    self.reset();
                }
            }
            MMIO_QUEUE_DESC_LO => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].desc_addr = (self.queues[q].desc_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_DESC_HI => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].desc_addr = (self.queues[q].desc_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            MMIO_QUEUE_AVAIL_LO => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].avail_addr = (self.queues[q].avail_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_AVAIL_HI => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].avail_addr = (self.queues[q].avail_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            MMIO_QUEUE_USED_LO => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].used_addr = (self.queues[q].used_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_USED_HI => {
                let q = self.queue_sel as usize;
                if q < NUM_QUEUES {
                    self.queues[q].used_addr = (self.queues[q].used_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            // Device config writes
            off if off >= MMIO_DEVICE_CONFIG => {
                let cfg_off = off - MMIO_DEVICE_CONFIG;
                if cfg_off == 0x14 {
                    // emerg_wr: guest writes a single character directly
                    self.tx_log.push(val as u8);
                }
            }
            _ => {}
        }
    }

    /// Reset the device state.
    pub fn reset(&mut self) {
        self.status = 0;
        self.queue_sel = 0;
        self.queues = [QueueState::default(), QueueState::default()];
        self.driver_features = 0;
        self.interrupt_status = 0;
        self.irq_line = false;
        self.rx_fifo.clear();
    }

    // -----------------------------------------------------------------------
    // Virtqueue Processing (TX & RX)
    // -----------------------------------------------------------------------

    /// Process a notified virtqueue.
    pub fn process_queue(&mut self, q_idx: usize, ram: &mut [u8]) {
        if (self.status & VIRTIO_STATUS_DRIVER_OK) == 0 {
            return;
        }
        if q_idx >= NUM_QUEUES || !self.queues[q_idx].ready {
            return;
        }

        match q_idx {
            QUEUE_TX => self.process_tx(ram),
            QUEUE_RX => self.process_rx(ram),
            _ => {}
        }
    }

    /// Drain TX virtqueue: read characters from guest and append to tx_log.
    fn process_tx(&mut self, ram: &mut [u8]) {
        let q = &mut self.queues[QUEUE_TX];
        let mut processed = false;

        loop {
            match pop_chain(ram, q.desc_addr, q.avail_addr, q.num as u16, q.last_avail_idx) {
                Ok(Some(pop)) => {
                    q.last_avail_idx = pop.next_avail;

                    // Read bytes from readable buffers in chain
                    let mut bytes_read: u32 = 0;
                    for buf in &pop.chain.readable {
                        let start = buf.addr as usize;
                        let end = start + buf.len as usize;
                        if end <= ram.len() {
                            self.tx_log.extend_from_slice(&ram[start..end]);
                            bytes_read += buf.len;
                        }
                    }

                    // Push to used ring
                    if let Ok(next_used) = push_used(
                        ram,
                        q.used_addr,
                        q.num as u16,
                        q.last_used_idx,
                        pop.chain.head,
                        bytes_read,
                    ) {
                        q.last_used_idx = next_used;
                        processed = true;
                    }
                }
                _ => break,
            }
        }

        if processed {
            self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
            self.irq_line = true;
        }
    }

    /// Process RX virtqueue: copy incoming host characters into guest buffers.
    fn process_rx(&mut self, ram: &mut [u8]) {
        if self.rx_fifo.is_empty() {
            return;
        }

        let q = &mut self.queues[QUEUE_RX];
        let mut processed = false;

        while !self.rx_fifo.is_empty() {
            match pop_chain(ram, q.desc_addr, q.avail_addr, q.num as u16, q.last_avail_idx) {
                Ok(Some(pop)) => {
                    q.last_avail_idx = pop.next_avail;

                    let mut bytes_written: u32 = 0;
                    for buf in &pop.chain.writable {
                        let start = buf.addr as usize;
                        let max_len = buf.len as usize;
                        let mut count = 0;

                        while count < max_len && !self.rx_fifo.is_empty() {
                            if start + count < ram.len() {
                                ram[start + count] = self.rx_fifo.pop_front().unwrap();
                                count += 1;
                            } else {
                                break;
                            }
                        }
                        bytes_written += count as u32;
                        if self.rx_fifo.is_empty() {
                            break;
                        }
                    }

                    if let Ok(next_used) = push_used(
                        ram,
                        q.used_addr,
                        q.num as u16,
                        q.last_used_idx,
                        pop.chain.head,
                        bytes_written,
                    ) {
                        q.last_used_idx = next_used;
                        processed = true;
                    }
                }
                _ => break,
            }
        }

        if processed {
            self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
            self.irq_line = true;
        }
    }

    /// Host injects input characters into the console RX buffer.
    pub fn inject_rx(&mut self, data: &[u8], ram: &mut [u8]) {
        self.rx_fifo.extend(data.iter().copied());
        if self.queues[QUEUE_RX].ready && (self.status & VIRTIO_STATUS_DRIVER_OK) != 0 {
            self.process_rx(ram);
        }
    }

    /// Returns the captured TX log as a string.
    pub fn tx_string(&self) -> String {
        String::from_utf8_lossy(&self.tx_log).to_string()
    }
}

// ---------------------------------------------------------------------------
// Bare-Metal Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use u6_virtio_transport::{
        VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER, VIRTIO_STATUS_FEATURES_OK,
    };

    fn setup_ram(size: usize) -> Vec<u8> {
        vec![0u8; size]
    }

    #[test]
    fn console_mmio_probe() {
        let con = VirtioConsole::new();
        assert_eq!(con.mmio_read(MMIO_MAGIC, 4), MAGIC_VALUE as u32);
        assert_eq!(con.mmio_read(MMIO_VERSION, 4), MMIO_VERSION_VALUE as u32);
        assert_eq!(con.mmio_read(MMIO_DEVICE_ID, 4), 3);
        assert_eq!(con.mmio_read(MMIO_VENDOR_ID, 4), VENDOR_ID_VALUE as u32);
        assert_ne!(con.mmio_read(0x014, 4) & ((VIRTIO_F_VERSION_1 >> 32) as u32), 0);
    }

    #[test]
    fn console_feature_negotiation() {
        let mut con = VirtioConsole::new();
        let mut ram = setup_ram(4096);

        // 1. Reset
        con.mmio_write(MMIO_STATUS, 0, 4, &mut ram);
        assert_eq!(con.status, 0);

        // 2. Acknowledge
        con.mmio_write(MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE as u32, 4, &mut ram);
        assert_eq!(con.status, VIRTIO_STATUS_ACKNOWLEDGE);

        // 3. Driver
        con.mmio_write(MMIO_STATUS, (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER) as u32, 4, &mut ram);

        // 4. Negotiate features
        con.mmio_write(MMIO_DRIVER_FEATURES, (VIRTIO_F_VERSION_1 & 0xFFFF_FFFF) as u32, 4, &mut ram);
        con.mmio_write(0x024, (VIRTIO_F_VERSION_1 >> 32) as u32, 4, &mut ram);
        assert_eq!(con.driver_features, VIRTIO_F_VERSION_1);

        // 5. Features OK
        con.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK) as u32,
            4,
            &mut ram,
        );

        // 6. Driver OK
        con.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );
        assert_ne!(con.status & VIRTIO_STATUS_DRIVER_OK, 0);
    }

    #[test]
    fn console_baremetal_tx_flow() {
        let mut con = VirtioConsole::new();
        let mut ram = setup_ram(65536);

        // Complete driver negotiation
        con.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );

        // Setup Queue 1 (TX Queue)
        let q_size: u16 = 16;
        let desc_table = 0x1000u64;
        let avail_ring = 0x2000u64;
        let used_ring = 0x3000u64;

        con.mmio_write(MMIO_QUEUE_SEL, QUEUE_TX as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_NUM, q_size as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_DESC_LO, desc_table as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_DESC_HI, (desc_table >> 32) as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_AVAIL_LO, avail_ring as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_AVAIL_HI, (avail_ring >> 32) as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_USED_LO, used_ring as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_USED_HI, (used_ring >> 32) as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut ram);
        assert!(con.queues[QUEUE_TX].ready);

        // Put text into guest memory at 0x4000
        let msg = b"Hello, Bare-Metal VirtIO Console!\n";
        let buf_addr = 0x4000u64;
        ram[buf_addr as usize..buf_addr as usize + msg.len()].copy_from_slice(msg);

        // Populate Descriptor 0 in Desc table: addr=0x4000, len=msg.len(), flags=0 (read-only)
        let d0_off = desc_table as usize;
        ram[d0_off..d0_off + 8].copy_from_slice(&buf_addr.to_le_bytes());
        ram[d0_off + 8..d0_off + 12].copy_from_slice(&(msg.len() as u32).to_le_bytes());
        ram[d0_off + 12..d0_off + 14].copy_from_slice(&0u16.to_le_bytes()); // flags = 0
        ram[d0_off + 14..d0_off + 16].copy_from_slice(&0u16.to_le_bytes()); // next = 0

        // Populate Avail Ring: idx = 1, ring[0] = 0
        let a_off = avail_ring as usize;
        ram[a_off..a_off + 2].copy_from_slice(&0u16.to_le_bytes()); // flags
        ram[a_off + 2..a_off + 4].copy_from_slice(&1u16.to_le_bytes()); // idx = 1
        ram[a_off + 4..a_off + 6].copy_from_slice(&0u16.to_le_bytes()); // ring[0] = desc 0

        // Notify Queue 1
        con.mmio_write(MMIO_QUEUE_NOTIFY, QUEUE_TX as u32, 4, &mut ram);

        // Verify transmission captured
        assert_eq!(con.tx_string(), "Hello, Bare-Metal VirtIO Console!\n");
        assert!(con.irq_line, "Interrupt must be asserted");
        assert_eq!(con.mmio_read(MMIO_INTERRUPT_STATUS, 4) & VIRTIO_MMIO_INT_VRING, VIRTIO_MMIO_INT_VRING);

        // Verify used ring: idx = 1, used[0].id = 0, used[0].len = msg.len()
        let u_off = used_ring as usize;
        let used_idx = u16::from_le_bytes([ram[u_off + 2], ram[u_off + 3]]);
        assert_eq!(used_idx, 1);
        let used_elem_id = u32::from_le_bytes([ram[u_off + 4], ram[u_off + 5], ram[u_off + 6], ram[u_off + 7]]);
        let used_elem_len = u32::from_le_bytes([ram[u_off + 8], ram[u_off + 9], ram[u_off + 10], ram[u_off + 11]]);
        assert_eq!(used_elem_id, 0);
        assert_eq!(used_elem_len, msg.len() as u32);

        // Acknowledge interrupt
        con.mmio_write(MMIO_INTERRUPT_ACK, VIRTIO_MMIO_INT_VRING, 4, &mut ram);
        assert!(!con.irq_line);
        assert_eq!(con.mmio_read(MMIO_INTERRUPT_STATUS, 4), 0);
    }

    #[test]
    fn console_baremetal_rx_flow() {
        let mut con = VirtioConsole::new();
        let mut ram = setup_ram(65536);

        con.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );

        // Setup Queue 0 (RX Queue)
        let q_size: u16 = 16;
        let desc_table = 0x1000u64;
        let avail_ring = 0x2000u64;
        let used_ring = 0x3000u64;

        con.mmio_write(MMIO_QUEUE_SEL, QUEUE_RX as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_NUM, q_size as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_DESC_LO, desc_table as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_AVAIL_LO, avail_ring as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_USED_LO, used_ring as u32, 4, &mut ram);
        con.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut ram);

        // Guest supplies writable buffer at 0x5000 of length 64 bytes
        let rx_buf_addr = 0x5000u64;
        let d0_off = desc_table as usize;
        ram[d0_off..d0_off + 8].copy_from_slice(&rx_buf_addr.to_le_bytes());
        ram[d0_off + 8..d0_off + 12].copy_from_slice(&64u32.to_le_bytes());
        ram[d0_off + 12..d0_off + 14].copy_from_slice(&2u16.to_le_bytes()); // flags = VRING_DESC_F_WRITE (2)
        ram[d0_off + 14..d0_off + 16].copy_from_slice(&0u16.to_le_bytes());

        let a_off = avail_ring as usize;
        ram[a_off + 2..a_off + 4].copy_from_slice(&1u16.to_le_bytes()); // idx = 1
        ram[a_off + 4..a_off + 6].copy_from_slice(&0u16.to_le_bytes()); // ring[0] = 0

        // Host injects characters into RX
        let host_input = b"echo 'ping'\n";
        con.inject_rx(host_input, &mut ram);

        // Verify guest memory has the injected characters
        let received = &ram[rx_buf_addr as usize..rx_buf_addr as usize + host_input.len()];
        assert_eq!(received, host_input);

        // Verify used ring
        let u_off = used_ring as usize;
        let used_idx = u16::from_le_bytes([ram[u_off + 2], ram[u_off + 3]]);
        assert_eq!(used_idx, 1);
        let used_len = u32::from_le_bytes([ram[u_off + 8], ram[u_off + 9], ram[u_off + 10], ram[u_off + 11]]);
        assert_eq!(used_len, host_input.len() as u32);
        assert!(con.irq_line);
    }
}
