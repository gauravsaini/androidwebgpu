//! U17 `virtio-blk` — OASIS VirtIO Block Device Model (MMIO transport).
//!
//! Purity: EXPLICIT-STATE. All state lives inside [`VirtioBlk`].
//! Performs no I/O, no threading, no time reads — fully deterministic.
//!
//! # Architecture Reference
//! - OASIS Virtual I/O Device (VIRTIO) Version 1.2, Section 5.2 (Block Device)
//! - Device ID: 2 (`VIRTIO_ID_BLOCK`)
//! - Queue 0: `requestq` (requests from guest driver)
//!
//! Default QEMU virt MMIO slot: `0x0A00_0200`, size `0x200`.

use u6_virtio_transport::{
    pop_chain, push_used, MAGIC_VALUE, MMIO_DEVICE_CONFIG, MMIO_DEVICE_FEATURES, MMIO_DEVICE_ID,
    MMIO_DRIVER_FEATURES, MMIO_MAGIC, MMIO_QUEUE_AVAIL_HI, MMIO_QUEUE_AVAIL_LO,
    MMIO_QUEUE_DESC_HI, MMIO_QUEUE_DESC_LO, MMIO_QUEUE_NOTIFY, MMIO_QUEUE_NUM,
    MMIO_QUEUE_NUM_MAX, MMIO_QUEUE_READY, MMIO_QUEUE_SEL, MMIO_QUEUE_USED_HI, MMIO_QUEUE_USED_LO,
    MMIO_STATUS, MMIO_VENDOR_ID, MMIO_VERSION, MMIO_VERSION_VALUE, VENDOR_ID_VALUE,
    VIRTIO_F_VERSION_1, VIRTIO_STATUS_DRIVER_OK, VIRTQ_MAX_SIZE,
};

pub const MMIO_INTERRUPT_STATUS: u64 = 0x060;
pub const MMIO_INTERRUPT_ACK: u64 = 0x064;

pub const VIRTIO_BLK_BASE: u64 = 0x0A00_0200;
pub const VIRTIO_BLK_SIZE: u64 = 0x200;
pub const VIRTIO_BLK_DEVICE_ID: u32 = 2;
pub const VIRTIO_BLK_IRQ: u32 = 49; // SPI 17 = 32 + 17

pub const SECTOR_SIZE: usize = 512;

// Request types (OASIS §5.2.6)
pub const VIRTIO_BLK_T_IN: u32 = 0; // Read
pub const VIRTIO_BLK_T_OUT: u32 = 1; // Write
pub const VIRTIO_BLK_T_FLUSH: u32 = 4;
pub const VIRTIO_BLK_T_GET_ID: u32 = 8;

// Status codes (OASIS §5.2.6)
pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;
pub const VIRTIO_BLK_S_UNSUPP: u8 = 2;

// Features (OASIS §5.2.3)
pub const VIRTIO_BLK_F_SIZE_MAX: u64 = 1 << 1;
pub const VIRTIO_BLK_F_SEG_MAX: u64 = 1 << 2;
pub const VIRTIO_BLK_F_GEOMETRY: u64 = 1 << 4;
pub const VIRTIO_BLK_F_RO: u64 = 1 << 5;
pub const VIRTIO_BLK_F_BLK_SIZE: u64 = 1 << 6;
pub const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;

pub const DEFAULT_BLK_FEATURES: u64 = VIRTIO_F_VERSION_1
    | VIRTIO_BLK_F_FLUSH
    | VIRTIO_BLK_F_BLK_SIZE
    | VIRTIO_BLK_F_SEG_MAX
    | VIRTIO_BLK_F_SIZE_MAX;

pub const VIRTIO_MMIO_INT_VRING: u32 = 1 << 0;

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

/// Explicit state of the VirtIO Block Device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtioBlk {
    pub status: u8,
    pub queue_sel: u32,
    pub queue: QueueState,
    pub driver_features: u64,
    pub device_features: u64,
    pub interrupt_status: u32,
    pub irq_line: bool,
    pub irq_num: u32,
    pub read_only: bool,

    // Disk geometry and config
    pub capacity_sectors: u64,
    pub blk_size: u32,
    pub size_max: u32,
    pub seg_max: u32,

    // Storage backing
    pub disk_data: Vec<u8>,
}

impl VirtioBlk {
    /// Create a new block device with the specified capacity in 512-byte sectors.
    pub fn new(capacity_sectors: u64) -> Self {
        let size_bytes = (capacity_sectors as usize).saturating_mul(SECTOR_SIZE);
        Self {
            status: 0,
            queue_sel: 0,
            queue: QueueState::default(),
            driver_features: 0,
            device_features: DEFAULT_BLK_FEATURES,
            interrupt_status: 0,
            irq_line: false,
            irq_num: VIRTIO_BLK_IRQ,
            read_only: false,

            capacity_sectors,
            blk_size: SECTOR_SIZE as u32,
            size_max: 65536,
            seg_max: 128,

            disk_data: vec![0u8; size_bytes],
        }
    }

    /// Reset device state.
    pub fn reset(&mut self) {
        self.status = 0;
        self.queue_sel = 0;
        self.queue = QueueState::default();
        self.driver_features = 0;
        self.interrupt_status = 0;
        self.irq_line = false;
    }

    // -----------------------------------------------------------------------
    // MMIO Registers Interface
    // -----------------------------------------------------------------------

    pub fn mmio_read(&self, offset: u64, size: usize) -> u32 {
        let val32 = match offset {
            MMIO_MAGIC => MAGIC_VALUE as u32,
            MMIO_VERSION => MMIO_VERSION_VALUE as u32,
            MMIO_DEVICE_ID => VIRTIO_BLK_DEVICE_ID,
            MMIO_VENDOR_ID => VENDOR_ID_VALUE as u32,
            MMIO_DEVICE_FEATURES => (self.device_features & 0xFFFF_FFFF) as u32,
            0x014 => (self.device_features >> 32) as u32,
            MMIO_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    self.queue.num_max
                } else {
                    0
                }
            }
            MMIO_QUEUE_READY => {
                if self.queue_sel == 0 && self.queue.ready {
                    1
                } else {
                    0
                }
            }
            MMIO_INTERRUPT_STATUS => self.interrupt_status,
            MMIO_STATUS => self.status as u32,

            // Device-specific config space (0x100..0x140)
            off if off >= MMIO_DEVICE_CONFIG => {
                let cfg_off = off - MMIO_DEVICE_CONFIG;
                match cfg_off {
                    // 0x00..0x07: capacity (u64)
                    0x00 => (self.capacity_sectors & 0xFFFF_FFFF) as u32,
                    0x04 => (self.capacity_sectors >> 32) as u32,
                    // 0x08..0x0B: size_max
                    0x08 => self.size_max,
                    // 0x0C..0x0F: seg_max
                    0x0C => self.seg_max,
                    // 0x14..0x17: blk_size
                    0x14 => self.blk_size,
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
                if self.queue_sel == 0 {
                    self.queue.num = val;
                }
            }
            MMIO_QUEUE_READY => {
                if self.queue_sel == 0 {
                    self.queue.ready = (val & 1) != 0;
                }
            }
            MMIO_QUEUE_NOTIFY => {
                if val == 0 {
                    self.process_requests(ram);
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
                    self.reset();
                }
            }
            MMIO_QUEUE_DESC_LO => {
                if self.queue_sel == 0 {
                    self.queue.desc_addr = (self.queue.desc_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_DESC_HI => {
                if self.queue_sel == 0 {
                    self.queue.desc_addr = (self.queue.desc_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            MMIO_QUEUE_AVAIL_LO => {
                if self.queue_sel == 0 {
                    self.queue.avail_addr = (self.queue.avail_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_AVAIL_HI => {
                if self.queue_sel == 0 {
                    self.queue.avail_addr = (self.queue.avail_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            MMIO_QUEUE_USED_LO => {
                if self.queue_sel == 0 {
                    self.queue.used_addr = (self.queue.used_addr & 0xFFFF_FFFF_0000_0000) | (val as u64);
                }
            }
            MMIO_QUEUE_USED_HI => {
                if self.queue_sel == 0 {
                    self.queue.used_addr = (self.queue.used_addr & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // VirtIO Block Request Processing
    // -----------------------------------------------------------------------

    /// Process submitted block requests in Queue 0.
    pub fn process_requests(&mut self, ram: &mut [u8]) {
        if (self.status & VIRTIO_STATUS_DRIVER_OK) == 0 || !self.queue.ready {
            return;
        }

        let mut processed = false;
        let q = &mut self.queue;

        loop {
            match pop_chain(ram, q.desc_addr, q.avail_addr, q.num as u16, q.last_avail_idx) {
                Ok(Some(pop)) => {
                    q.last_avail_idx = pop.next_avail;

                    // Parse request:
                    // 1. Header (readable, >= 16 bytes: type u32, ioprio u32, sector u64)
                    // 2. Data (readable for write, writable for read)
                    // 3. Status byte (writable, 1 byte at end)
                    let (status_byte, bytes_transferred) = Self::handle_single_request(
                        &mut self.disk_data,
                        self.read_only,
                        &pop.chain,
                        ram,
                    );

                    // Commit used ring entry
                    if let Ok(next_used) = push_used(
                        ram,
                        q.used_addr,
                        q.num as u16,
                        q.last_used_idx,
                        pop.chain.head,
                        bytes_transferred + 1, // data bytes + 1 status byte
                    ) {
                        q.last_used_idx = next_used;
                        processed = true;
                    }

                    let _ = status_byte;
                }
                _ => break,
            }
        }

        if processed {
            self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
            self.irq_line = true;
        }
    }

    fn handle_single_request(
        disk_data: &mut [u8],
        read_only: bool,
        chain: &u6_virtio_transport::Chain,
        ram: &mut [u8],
    ) -> (u8, u32) {
        // Need at least 1 readable buffer for header and 1 writable buffer for status
        if chain.readable.is_empty() || chain.writable.is_empty() {
            return (VIRTIO_BLK_S_IOERR, 0);
        }

        let hdr_buf = &chain.readable[0];
        if hdr_buf.len < 16 || (hdr_buf.addr as usize) + 16 > ram.len() {
            return (VIRTIO_BLK_S_IOERR, 0);
        }

        let h_off = hdr_buf.addr as usize;
        let req_type = u32::from_le_bytes([ram[h_off], ram[h_off + 1], ram[h_off + 2], ram[h_off + 3]]);
        let sector = u64::from_le_bytes([
            ram[h_off + 8], ram[h_off + 9], ram[h_off + 10], ram[h_off + 11],
            ram[h_off + 12], ram[h_off + 13], ram[h_off + 14], ram[h_off + 15],
        ]);

        let status_buf = chain.writable.last().unwrap();
        let status_addr = (status_buf.addr + status_buf.len as u64 - 1) as usize;

        let write_status = |ram: &mut [u8], s: u8| {
            if status_addr < ram.len() {
                ram[status_addr] = s;
            }
        };

        match req_type {
            VIRTIO_BLK_T_IN => {
                // Read from disk into guest memory
                let mut transferred: u32 = 0;
                let mut disk_offset = (sector as usize).saturating_mul(SECTOR_SIZE);

                // All writable buffers except the final status byte
                for (i, buf) in chain.writable.iter().enumerate() {
                    let is_last = i == chain.writable.len() - 1;
                    let buf_len = if is_last {
                        (buf.len as usize).saturating_sub(1)
                    } else {
                        buf.len as usize
                    };

                    if buf_len == 0 {
                        continue;
                    }

                    if disk_offset + buf_len > disk_data.len() || (buf.addr as usize) + buf_len > ram.len() {
                        write_status(ram, VIRTIO_BLK_S_IOERR);
                        return (VIRTIO_BLK_S_IOERR, 0);
                    }

                    ram[buf.addr as usize..buf.addr as usize + buf_len]
                        .copy_from_slice(&disk_data[disk_offset..disk_offset + buf_len]);
                    disk_offset += buf_len;
                    transferred += buf_len as u32;
                }

                write_status(ram, VIRTIO_BLK_S_OK);
                (VIRTIO_BLK_S_OK, transferred)
            }
            VIRTIO_BLK_T_OUT => {
                // Write from guest memory into disk
                if read_only {
                    write_status(ram, VIRTIO_BLK_S_IOERR);
                    return (VIRTIO_BLK_S_IOERR, 0);
                }

                let mut transferred: u32 = 0;
                let mut disk_offset = (sector as usize).saturating_mul(SECTOR_SIZE);

                // All readable buffers except header
                for buf in chain.readable.iter().skip(1) {
                    let buf_len = buf.len as usize;
                    if disk_offset + buf_len > disk_data.len() || (buf.addr as usize) + buf_len > ram.len() {
                        write_status(ram, VIRTIO_BLK_S_IOERR);
                        return (VIRTIO_BLK_S_IOERR, 0);
                    }

                    disk_data[disk_offset..disk_offset + buf_len]
                        .copy_from_slice(&ram[buf.addr as usize..buf.addr as usize + buf_len]);
                    disk_offset += buf_len;
                    transferred += buf_len as u32;
                }

                write_status(ram, VIRTIO_BLK_S_OK);
                (VIRTIO_BLK_S_OK, transferred)
            }
            VIRTIO_BLK_T_FLUSH => {
                write_status(ram, VIRTIO_BLK_S_OK);
                (VIRTIO_BLK_S_OK, 0)
            }
            _ => {
                write_status(ram, VIRTIO_BLK_S_UNSUPP);
                (VIRTIO_BLK_S_UNSUPP, 0)
            }
        }
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
    fn blk_mmio_probe() {
        let blk = VirtioBlk::new(2048); // 1 MiB disk (2048 * 512)
        assert_eq!(blk.mmio_read(MMIO_MAGIC, 4), MAGIC_VALUE as u32);
        assert_eq!(blk.mmio_read(MMIO_VERSION, 4), MMIO_VERSION_VALUE as u32);
        assert_eq!(blk.mmio_read(MMIO_DEVICE_ID, 4), 2);
        assert_eq!(blk.mmio_read(MMIO_VENDOR_ID, 4), VENDOR_ID_VALUE as u32);

        // Check capacity in config space
        assert_eq!(blk.mmio_read(MMIO_DEVICE_CONFIG, 4), 2048);
        assert_eq!(blk.mmio_read(MMIO_DEVICE_CONFIG + 0x14, 4), 512); // sector size
    }

    #[test]
    fn blk_feature_negotiation() {
        let mut blk = VirtioBlk::new(1024);
        let mut ram = setup_ram(4096);

        blk.mmio_write(MMIO_STATUS, VIRTIO_STATUS_ACKNOWLEDGE as u32, 4, &mut ram);
        blk.mmio_write(MMIO_STATUS, (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER) as u32, 4, &mut ram);

        let feat = VIRTIO_F_VERSION_1 | VIRTIO_BLK_F_FLUSH | VIRTIO_BLK_F_BLK_SIZE;
        blk.mmio_write(MMIO_DRIVER_FEATURES, (feat & 0xFFFF_FFFF) as u32, 4, &mut ram);
        blk.mmio_write(0x024, (feat >> 32) as u32, 4, &mut ram);

        blk.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );
        assert_ne!(blk.status & VIRTIO_STATUS_DRIVER_OK, 0);
    }

    #[test]
    fn blk_baremetal_write_and_read_roundtrip() {
        let mut blk = VirtioBlk::new(1024); // 512 KiB disk
        let mut ram = setup_ram(131072);

        // Enable driver
        blk.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );

        // Setup Queue 0 (requestq)
        let q_size: u16 = 16;
        let desc_table = 0x1000u64;
        let avail_ring = 0x2000u64;
        let used_ring = 0x3000u64;

        blk.mmio_write(MMIO_QUEUE_SEL, 0, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_NUM, q_size as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_DESC_LO, desc_table as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_AVAIL_LO, avail_ring as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_USED_LO, used_ring as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut ram);

        // ---- Part 1: WRITE (VIRTIO_BLK_T_OUT) 512 bytes to sector 10 ----
        let hdr_addr = 0x4000u64;
        let data_write_addr = 0x4100u64;
        let status_addr = 0x4400u64;

        // Build header: type = 1 (OUT), ioprio = 0, sector = 10
        ram[hdr_addr as usize..hdr_addr as usize + 4].copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
        ram[hdr_addr as usize + 4..hdr_addr as usize + 8].copy_from_slice(&0u32.to_le_bytes());
        ram[hdr_addr as usize + 8..hdr_addr as usize + 16].copy_from_slice(&10u64.to_le_bytes());

        // Fill payload: 512 bytes with 0x42
        let payload = vec![0x42u8; 512];
        ram[data_write_addr as usize..data_write_addr as usize + 512].copy_from_slice(&payload);
        ram[status_addr as usize] = 0xFF; // uninitialized status

        // Descriptor 0: Header (16 bytes, NEXT, read-only)
        let d0 = desc_table as usize;
        ram[d0..d0 + 8].copy_from_slice(&hdr_addr.to_le_bytes());
        ram[d0 + 8..d0 + 12].copy_from_slice(&16u32.to_le_bytes());
        ram[d0 + 12..d0 + 14].copy_from_slice(&1u16.to_le_bytes()); // NEXT
        ram[d0 + 14..d0 + 16].copy_from_slice(&1u16.to_le_bytes()); // next = 1

        // Descriptor 1: Data (512 bytes, NEXT, read-only for OUT)
        let d1 = desc_table as usize + 16;
        ram[d1..d1 + 8].copy_from_slice(&data_write_addr.to_le_bytes());
        ram[d1 + 8..d1 + 12].copy_from_slice(&512u32.to_le_bytes());
        ram[d1 + 12..d1 + 14].copy_from_slice(&1u16.to_le_bytes()); // NEXT
        ram[d1 + 14..d1 + 16].copy_from_slice(&2u16.to_le_bytes()); // next = 2

        // Descriptor 2: Status (1 byte, WRITE)
        let d2 = desc_table as usize + 32;
        ram[d2..d2 + 8].copy_from_slice(&status_addr.to_le_bytes());
        ram[d2 + 8..d2 + 12].copy_from_slice(&1u32.to_le_bytes());
        ram[d2 + 12..d2 + 14].copy_from_slice(&2u16.to_le_bytes()); // WRITE
        ram[d2 + 14..d2 + 16].copy_from_slice(&0u16.to_le_bytes());

        // Put head 0 in avail ring: idx = 1, ring[0] = 0
        let a_off = avail_ring as usize;
        ram[a_off + 2..a_off + 4].copy_from_slice(&1u16.to_le_bytes());
        ram[a_off + 4..a_off + 6].copy_from_slice(&0u16.to_le_bytes());

        // Notify queue 0
        blk.mmio_write(MMIO_QUEUE_NOTIFY, 0, 4, &mut ram);

        // Verify write success
        assert_eq!(ram[status_addr as usize], VIRTIO_BLK_S_OK);
        let disk_target = 10 * 512;
        assert_eq!(&blk.disk_data[disk_target..disk_target + 512], &payload[..]);
        assert!(blk.irq_line);

        // Acknowledge IRQ
        blk.mmio_write(MMIO_INTERRUPT_ACK, VIRTIO_MMIO_INT_VRING, 4, &mut ram);
        assert!(!blk.irq_line);

        // ---- Part 2: READ (VIRTIO_BLK_T_IN) 512 bytes from sector 10 ----
        let data_read_addr = 0x5000u64;
        let status_read_addr = 0x5400u64;

        // Build header: type = 0 (IN), sector = 10
        let hdr_in_addr = 0x4800u64;
        ram[hdr_in_addr as usize..hdr_in_addr as usize + 4].copy_from_slice(&VIRTIO_BLK_T_IN.to_le_bytes());
        ram[hdr_in_addr as usize + 4..hdr_in_addr as usize + 8].copy_from_slice(&0u32.to_le_bytes());
        ram[hdr_in_addr as usize + 8..hdr_in_addr as usize + 16].copy_from_slice(&10u64.to_le_bytes());
        ram[status_read_addr as usize] = 0xFF;

        // Descriptor 3: Header (16 bytes, NEXT)
        let d3 = desc_table as usize + 48;
        ram[d3..d3 + 8].copy_from_slice(&hdr_in_addr.to_le_bytes());
        ram[d3 + 8..d3 + 12].copy_from_slice(&16u32.to_le_bytes());
        ram[d3 + 12..d3 + 14].copy_from_slice(&1u16.to_le_bytes()); // NEXT
        ram[d3 + 14..d3 + 16].copy_from_slice(&4u16.to_le_bytes()); // next = 4

        // Descriptor 4: Data buffer (512 bytes, NEXT, WRITE for IN)
        let d4 = desc_table as usize + 64;
        ram[d4..d4 + 8].copy_from_slice(&data_read_addr.to_le_bytes());
        ram[d4 + 8..d4 + 12].copy_from_slice(&512u32.to_le_bytes());
        ram[d4 + 12..d4 + 14].copy_from_slice(&(1u16 | 2u16).to_le_bytes()); // NEXT | WRITE
        ram[d4 + 14..d4 + 16].copy_from_slice(&5u16.to_le_bytes()); // next = 5

        // Descriptor 5: Status (1 byte, WRITE)
        let d5 = desc_table as usize + 80;
        ram[d5..d5 + 8].copy_from_slice(&status_read_addr.to_le_bytes());
        ram[d5 + 8..d5 + 12].copy_from_slice(&1u32.to_le_bytes());
        ram[d5 + 12..d5 + 14].copy_from_slice(&2u16.to_le_bytes()); // WRITE
        ram[d5 + 14..d5 + 16].copy_from_slice(&0u16.to_le_bytes());

        // Put head 3 in avail ring slot 1: idx = 2
        ram[a_off + 2..a_off + 4].copy_from_slice(&2u16.to_le_bytes());
        ram[a_off + 6..a_off + 8].copy_from_slice(&3u16.to_le_bytes());

        // Notify queue 0
        blk.mmio_write(MMIO_QUEUE_NOTIFY, 0, 4, &mut ram);

        // Verify read matches written data!
        assert_eq!(ram[status_read_addr as usize], VIRTIO_BLK_S_OK);
        assert_eq!(
            &ram[data_read_addr as usize..data_read_addr as usize + 512],
            &payload[..],
            "Data read from block device must match data previously written"
        );
        assert!(blk.irq_line);
    }

    #[test]
    fn blk_out_of_bounds_error() {
        let mut blk = VirtioBlk::new(10); // only 10 sectors
        let mut ram = setup_ram(65536);

        blk.mmio_write(
            MMIO_STATUS,
            (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
            4,
            &mut ram,
        );

        let desc_table = 0x1000u64;
        let avail_ring = 0x2000u64;
        let used_ring = 0x3000u64;

        blk.mmio_write(MMIO_QUEUE_SEL, 0, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_NUM, 16, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_DESC_LO, desc_table as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_AVAIL_LO, avail_ring as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_USED_LO, used_ring as u32, 4, &mut ram);
        blk.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut ram);

        // Request sector 50 (beyond capacity 10)
        let hdr_addr = 0x4000u64;
        let status_addr = 0x4500u64;
        ram[hdr_addr as usize..hdr_addr as usize + 4].copy_from_slice(&VIRTIO_BLK_T_IN.to_le_bytes());
        ram[hdr_addr as usize + 8..hdr_addr as usize + 16].copy_from_slice(&50u64.to_le_bytes());

        let d0 = desc_table as usize;
        ram[d0..d0 + 8].copy_from_slice(&hdr_addr.to_le_bytes());
        ram[d0 + 8..d0 + 12].copy_from_slice(&16u32.to_le_bytes());
        ram[d0 + 12..d0 + 14].copy_from_slice(&1u16.to_le_bytes());
        ram[d0 + 14..d0 + 16].copy_from_slice(&1u16.to_le_bytes());

        let d1 = desc_table as usize + 16;
        ram[d1..d1 + 8].copy_from_slice(&0x4200u64.to_le_bytes());
        ram[d1 + 8..d1 + 12].copy_from_slice(&512u32.to_le_bytes());
        ram[d1 + 12..d1 + 14].copy_from_slice(&(1u16 | 2u16).to_le_bytes());
        ram[d1 + 14..d1 + 16].copy_from_slice(&2u16.to_le_bytes());

        let d2 = desc_table as usize + 32;
        ram[d2..d2 + 8].copy_from_slice(&status_addr.to_le_bytes());
        ram[d2 + 8..d2 + 12].copy_from_slice(&1u32.to_le_bytes());
        ram[d2 + 12..d2 + 14].copy_from_slice(&2u16.to_le_bytes());
        ram[d2 + 14..d2 + 16].copy_from_slice(&0u16.to_le_bytes());

        let a_off = avail_ring as usize;
        ram[a_off + 2..a_off + 4].copy_from_slice(&1u16.to_le_bytes());
        ram[a_off + 4..a_off + 6].copy_from_slice(&0u16.to_le_bytes());

        blk.mmio_write(MMIO_QUEUE_NOTIFY, 0, 4, &mut ram);

        // Expect I/O error status (1)
        assert_eq!(ram[status_addr as usize], VIRTIO_BLK_S_IOERR);
    }
}
