//! Native virtio-blk device exposed through the QEMU-compatible MMIO transport.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use super::ram_offset_in;

pub const MMIO_MAGIC_VALUE: u32 = 0x000;
pub const MMIO_VERSION: u32 = 0x004;
pub const MMIO_DEVICE_ID: u32 = 0x008;
pub const MMIO_VENDOR_ID: u32 = 0x00c;
pub const MMIO_DEVICE_FEATURES: u32 = 0x010;
pub const MMIO_DEVICE_FEATURES_SEL: u32 = 0x014;
pub const MMIO_DRIVER_FEATURES: u32 = 0x020;
pub const MMIO_DRIVER_FEATURES_SEL: u32 = 0x024;
pub const MMIO_QUEUE_SEL: u32 = 0x030;
pub const MMIO_QUEUE_NUM_MAX: u32 = 0x034;
pub const MMIO_QUEUE_NUM: u32 = 0x038;
pub const MMIO_QUEUE_READY: u32 = 0x044;
pub const MMIO_QUEUE_NOTIFY: u32 = 0x050;
pub const MMIO_INTERRUPT_STATUS: u32 = 0x060;
pub const MMIO_INTERRUPT_ACK: u32 = 0x064;
pub const MMIO_STATUS: u32 = 0x070;
pub const MMIO_QUEUE_DESC_LOW: u32 = 0x080;
pub const MMIO_QUEUE_DESC_HIGH: u32 = 0x084;
pub const MMIO_QUEUE_AVAIL_LOW: u32 = 0x090;
pub const MMIO_QUEUE_AVAIL_HIGH: u32 = 0x094;
pub const MMIO_QUEUE_USED_LOW: u32 = 0x0a0;
pub const MMIO_QUEUE_USED_HIGH: u32 = 0x0a4;
pub const MMIO_CONFIG_GENERATION: u32 = 0x0fc;
pub const MMIO_CONFIG: u32 = 0x100;

const VIRTIO_VERSION_1: u64 = 1 << 32;
const VIRTIO_BLK_F_RO: u64 = 1 << 5;
const VIRTIO_BLK_F_BLK_SIZE: u64 = 1 << 6;
const VIRTIO_CONFIG_S_DRIVER_OK: u32 = 4;
const VIRTIO_CONFIG_S_FEATURES_OK: u32 = 8;
const VIRTIO_MMIO_INT_VRING: u32 = 1;
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VIRTIO_BLK_T_FLUSH: u32 = 4;
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;
const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const DESC_F_INDIRECT: u16 = 4;
const SECTOR_SIZE: u64 = 512;
const QUEUE_MAX: u16 = 256;

#[derive(Debug, Default)]
struct SplitQueue {
    size: u16,
    desc: u64,
    avail: u64,
    used: u64,
    ready: bool,
    last_avail_idx: u16,
}

#[derive(Debug)]
pub struct VirtioBlkDevice {
    file: File,
    capacity_sectors: u64,
    read_only: bool,
    device_features_sel: u32,
    driver_features_sel: u32,
    driver_features: u64,
    queue_sel: u32,
    queue: SplitQueue,
    status: u32,
    interrupt_status: u32,
    interrupt_line_asserted: bool,
}

#[derive(Debug, Clone, Copy)]
struct Descriptor {
    address: u64,
    length: u32,
    flags: u16,
    next: u16,
}

impl VirtioBlkDevice {
    pub fn open(path: &Path, read_only: bool) -> io::Result<Self> {
        let file = if read_only {
            OpenOptions::new().read(true).open(path)?
        } else {
            OpenOptions::new().read(true).write(true).open(path)?
        };
        let bytes = file.metadata()?.len();
        if bytes % SECTOR_SIZE != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("virtio-blk image length {bytes} is not a multiple of 512 bytes"),
            ));
        }
        Ok(Self {
            file,
            capacity_sectors: bytes / SECTOR_SIZE,
            read_only,
            device_features_sel: 0,
            driver_features_sel: 0,
            driver_features: 0,
            queue_sel: 0,
            queue: SplitQueue::default(),
            status: 0,
            interrupt_status: 0,
            interrupt_line_asserted: false,
        })
    }

    /// Create a new sparse raw image. Existing paths are deliberately never truncated.
    pub fn create_sparse_image(path: &Path, size_bytes: u64) -> io::Result<()> {
        if size_bytes == 0 || size_bytes % SECTOR_SIZE != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sparse virtio-blk image size must be a non-zero multiple of 512 bytes",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.set_len(size_bytes)
    }

    fn offered_features(&self) -> u64 {
        VIRTIO_VERSION_1 | VIRTIO_BLK_F_BLK_SIZE | if self.read_only { VIRTIO_BLK_F_RO } else { 0 }
    }

    pub fn read_mmio(&self, offset: u32) -> u32 {
        match offset {
            MMIO_MAGIC_VALUE => 0x7472_6976,
            MMIO_VERSION => 2,
            MMIO_DEVICE_ID => 2,
            MMIO_VENDOR_ID => 0x554d_4551,
            MMIO_DEVICE_FEATURES => {
                if self.device_features_sel < 2 {
                    (self.offered_features() >> (32 * self.device_features_sel)) as u32
                } else {
                    0
                }
            }
            MMIO_QUEUE_NUM_MAX => {
                if self.queue_sel == 0 {
                    QUEUE_MAX as u32
                } else {
                    0
                }
            }
            MMIO_QUEUE_READY => u32::from(self.queue_sel == 0 && self.queue.ready),
            MMIO_INTERRUPT_STATUS => self.interrupt_status,
            MMIO_STATUS => self.status,
            MMIO_CONFIG_GENERATION => 0,
            offset if offset >= MMIO_CONFIG => match offset - MMIO_CONFIG {
                0 => self.capacity_sectors as u32,
                4 => (self.capacity_sectors >> 32) as u32,
                // VIRTIO_BLK_F_BLK_SIZE is advertised.
                20 => SECTOR_SIZE as u32,
                _ => 0,
            },
            _ => 0,
        }
    }

    pub fn interrupt_line_asserted(&self) -> bool {
        self.interrupt_line_asserted
    }

    pub fn write_mmio(
        &mut self,
        offset: u32,
        value: u32,
        guest_ram: &mut [u8],
    ) -> Result<(), String> {
        match offset {
            MMIO_DEVICE_FEATURES_SEL => self.device_features_sel = value,
            MMIO_DRIVER_FEATURES_SEL => self.driver_features_sel = value,
            MMIO_DRIVER_FEATURES => {
                if self.driver_features_sel < 2 {
                    let shift = 32 * self.driver_features_sel;
                    let mask = 0xffff_ffffu64 << shift;
                    self.driver_features =
                        (self.driver_features & !mask) | ((value as u64) << shift);
                }
            }
            MMIO_QUEUE_SEL => self.queue_sel = value,
            MMIO_QUEUE_NUM if self.queue_sel == 0 => {
                self.queue.size = if value <= QUEUE_MAX as u32 {
                    value as u16
                } else {
                    0
                }
            }
            MMIO_QUEUE_DESC_LOW if self.queue_sel == 0 => {
                self.queue.desc = (self.queue.desc & !0xffff_ffff) | value as u64
            }
            MMIO_QUEUE_DESC_HIGH if self.queue_sel == 0 => {
                self.queue.desc = (self.queue.desc & 0xffff_ffff) | ((value as u64) << 32)
            }
            MMIO_QUEUE_AVAIL_LOW if self.queue_sel == 0 => {
                self.queue.avail = (self.queue.avail & !0xffff_ffff) | value as u64
            }
            MMIO_QUEUE_AVAIL_HIGH if self.queue_sel == 0 => {
                self.queue.avail = (self.queue.avail & 0xffff_ffff) | ((value as u64) << 32)
            }
            MMIO_QUEUE_USED_LOW if self.queue_sel == 0 => {
                self.queue.used = (self.queue.used & !0xffff_ffff) | value as u64
            }
            MMIO_QUEUE_USED_HIGH if self.queue_sel == 0 => {
                self.queue.used = (self.queue.used & 0xffff_ffff) | ((value as u64) << 32)
            }
            MMIO_QUEUE_READY if self.queue_sel == 0 => {
                self.queue.ready = value != 0 && self.queue_is_valid();
                if self.queue.ready {
                    self.queue.last_avail_idx = 0;
                }
            }
            MMIO_QUEUE_NOTIFY if value == 0 => self.process_available(guest_ram)?,
            MMIO_INTERRUPT_ACK => {
                self.interrupt_status &= !value;
                self.interrupt_line_asserted = self.interrupt_status != 0;
            }
            MMIO_STATUS => self.write_status(value),
            _ => {}
        }
        Ok(())
    }

    fn write_status(&mut self, value: u32) {
        if value == 0 {
            self.device_features_sel = 0;
            self.driver_features_sel = 0;
            self.driver_features = 0;
            self.queue_sel = 0;
            self.queue = SplitQueue::default();
            self.interrupt_status = 0;
            self.interrupt_line_asserted = false;
            self.status = 0;
            return;
        }
        let value = value & 0xff;
        if value & VIRTIO_CONFIG_S_FEATURES_OK != 0
            && self.driver_features & !self.offered_features() != 0
        {
            self.status = value & !VIRTIO_CONFIG_S_FEATURES_OK;
        } else {
            self.status = value;
        }
    }

    fn queue_is_valid(&self) -> bool {
        let size = self.queue.size;
        size != 0
            && size <= QUEUE_MAX
            && size.is_power_of_two()
            && self.queue.desc != 0
            && self.queue.avail != 0
            && self.queue.used != 0
            && self.queue.desc % 16 == 0
            && self.queue.avail % 2 == 0
            && self.queue.used % 4 == 0
    }

    fn process_available(&mut self, guest_ram: &mut [u8]) -> Result<(), String> {
        if self.queue_sel != 0 || !self.queue.ready || self.status & VIRTIO_CONFIG_S_DRIVER_OK == 0
        {
            return Ok(());
        }
        let avail_flags = read_guest_u16(guest_ram, self.queue.avail)?;
        let avail_idx = read_guest_u16(guest_ram, self.queue.avail + 2)?;
        let pending = avail_idx.wrapping_sub(self.queue.last_avail_idx);
        if pending > self.queue.size {
            return Err(format!(
                "virtio-blk avail ring overrun: {pending} > {}",
                self.queue.size
            ));
        }

        let mut completed = false;
        while self.queue.last_avail_idx != avail_idx {
            let ring_slot = self.queue.last_avail_idx % self.queue.size;
            let head = read_guest_u16(guest_ram, self.queue.avail + 4 + 2 * ring_slot as u64)?;
            let used_len = self.process_request(guest_ram, head)?;
            let used_idx = read_guest_u16(guest_ram, self.queue.used + 2)?;
            let used_slot = used_idx % self.queue.size;
            write_guest_u32(
                guest_ram,
                self.queue.used + 4 + 8 * used_slot as u64,
                head as u32,
            )?;
            write_guest_u32(
                guest_ram,
                self.queue.used + 8 + 8 * used_slot as u64,
                used_len,
            )?;
            write_guest_u16(guest_ram, self.queue.used + 2, used_idx.wrapping_add(1))?;
            self.queue.last_avail_idx = self.queue.last_avail_idx.wrapping_add(1);
            completed = true;
        }
        if completed {
            self.interrupt_status |= VIRTIO_MMIO_INT_VRING;
            self.interrupt_line_asserted = avail_flags & 1 == 0;
        }
        Ok(())
    }

    fn process_request(&mut self, guest_ram: &mut [u8], head: u16) -> Result<u32, String> {
        let descriptors = self.read_chain(guest_ram, head)?;
        if descriptors.len() < 2 {
            return Err("virtio-blk request must have header and status descriptors".into());
        }
        let header = descriptors[0];
        let status_desc = *descriptors.last().unwrap();
        if header.flags & (DESC_F_WRITE | DESC_F_INDIRECT) != 0 || header.length < 16 {
            return Err("invalid virtio-blk request header descriptor".into());
        }
        if status_desc.flags & DESC_F_WRITE == 0
            || status_desc.flags & (DESC_F_NEXT | DESC_F_INDIRECT) != 0
            || status_desc.length == 0
        {
            return Err("invalid virtio-blk status descriptor".into());
        }
        let header_bytes = read_guest_bytes(guest_ram, header.address, 16)?;
        let request_type = u32::from_le_bytes(header_bytes[0..4].try_into().unwrap());
        let sector = u64::from_le_bytes(header_bytes[8..16].try_into().unwrap());
        let data_descs = &descriptors[1..descriptors.len() - 1];

        let (used_len, request_status) = match request_type {
            VIRTIO_BLK_T_IN => {
                self.ensure_direction(data_descs, true)?;
                let byte_len = descriptor_bytes(data_descs)?;
                if byte_len % SECTOR_SIZE as usize != 0
                    || sector
                        .checked_add((byte_len as u64) / SECTOR_SIZE)
                        .is_none_or(|end| end > self.capacity_sectors)
                {
                    (1, VIRTIO_BLK_S_IOERR)
                } else {
                    let offset = sector
                        .checked_mul(SECTOR_SIZE)
                        .ok_or("virtio-blk LBA overflow")?;
                    let mut bytes = vec![0; byte_len];
                    let read_ok = self
                        .file
                        .seek(SeekFrom::Start(offset))
                        .and_then(|_| self.file.read_exact(&mut bytes))
                        .is_ok();
                    if read_ok {
                        let mut cursor = 0;
                        for descriptor in data_descs {
                            let len = descriptor.length as usize;
                            write_guest_bytes(
                                guest_ram,
                                descriptor.address,
                                &bytes[cursor..cursor + len],
                            )?;
                            cursor += len;
                        }
                        (
                            u32::try_from(byte_len + 1).unwrap_or(u32::MAX),
                            VIRTIO_BLK_S_OK,
                        )
                    } else {
                        (1, VIRTIO_BLK_S_IOERR)
                    }
                }
            }
            VIRTIO_BLK_T_OUT => {
                self.ensure_direction(data_descs, false)?;
                let byte_len = descriptor_bytes(data_descs)?;
                if self.read_only
                    || byte_len % SECTOR_SIZE as usize != 0
                    || sector
                        .checked_add((byte_len as u64) / SECTOR_SIZE)
                        .is_none_or(|end| end > self.capacity_sectors)
                {
                    (1, VIRTIO_BLK_S_IOERR)
                } else {
                    let offset = sector
                        .checked_mul(SECTOR_SIZE)
                        .ok_or("virtio-blk LBA overflow")?;
                    let mut bytes = Vec::with_capacity(byte_len);
                    for descriptor in data_descs {
                        bytes.extend_from_slice(&read_guest_bytes(
                            guest_ram,
                            descriptor.address,
                            descriptor.length as usize,
                        )?);
                    }
                    let write_ok = self
                        .file
                        .seek(SeekFrom::Start(offset))
                        .and_then(|_| self.file.write_all(&bytes))
                        .and_then(|_| self.file.flush())
                        .is_ok();
                    (
                        1,
                        if write_ok {
                            VIRTIO_BLK_S_OK
                        } else {
                            VIRTIO_BLK_S_IOERR
                        },
                    )
                }
            }
            VIRTIO_BLK_T_FLUSH => (1, VIRTIO_BLK_S_UNSUPP),
            _ => (1, VIRTIO_BLK_S_UNSUPP),
        };
        write_guest_bytes(guest_ram, status_desc.address, &[request_status])?;
        Ok(used_len)
    }

    fn ensure_direction(
        &self,
        descriptors: &[Descriptor],
        device_writes: bool,
    ) -> Result<(), String> {
        for descriptor in descriptors {
            if (descriptor.flags & DESC_F_WRITE != 0) != device_writes
                || descriptor.flags & DESC_F_INDIRECT != 0
            {
                return Err(
                    "virtio-blk data descriptor has invalid direction or INDIRECT flag".into(),
                );
            }
        }
        Ok(())
    }

    fn read_chain(&self, guest_ram: &[u8], head: u16) -> Result<Vec<Descriptor>, String> {
        if head >= self.queue.size {
            return Err(format!(
                "virtio-blk descriptor index {head} exceeds queue size {}",
                self.queue.size
            ));
        }
        let mut descriptors = Vec::new();
        let mut index = head;
        loop {
            if descriptors.len() >= self.queue.size as usize {
                return Err("virtio-blk descriptor chain loops or exceeds queue size".into());
            }
            let address = self.queue.desc + 16 * index as u64;
            let raw = read_guest_bytes(guest_ram, address, 16)?;
            let descriptor = Descriptor {
                address: u64::from_le_bytes(raw[0..8].try_into().unwrap()),
                length: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
                flags: u16::from_le_bytes(raw[12..14].try_into().unwrap()),
                next: u16::from_le_bytes(raw[14..16].try_into().unwrap()),
            };
            if descriptor.flags & DESC_F_INDIRECT != 0 {
                return Err("virtio-blk indirect descriptors are not negotiated".into());
            }
            descriptors.push(descriptor);
            if descriptor.flags & DESC_F_NEXT == 0 {
                break;
            }
            if descriptor.next >= self.queue.size {
                return Err(format!(
                    "virtio-blk next descriptor {} exceeds queue size",
                    descriptor.next
                ));
            }
            index = descriptor.next;
        }
        Ok(descriptors)
    }
}

fn descriptor_bytes(descriptors: &[Descriptor]) -> Result<usize, String> {
    descriptors.iter().try_fold(0usize, |total, descriptor| {
        total
            .checked_add(descriptor.length as usize)
            .ok_or_else(|| "virtio-blk data length overflow".into())
    })
}

fn guest_offset(ram: &[u8], address: u64, length: usize) -> Result<usize, String> {
    ram_offset_in(ram, address, length as u64)
        .map_err(|fault| format!("virtio-blk guest buffer outside RAM: {fault:?}"))
}

fn read_guest_bytes(ram: &[u8], address: u64, length: usize) -> Result<Vec<u8>, String> {
    let offset = guest_offset(ram, address, length)?;
    Ok(ram[offset..offset + length].to_vec())
}

fn write_guest_bytes(ram: &mut [u8], address: u64, bytes: &[u8]) -> Result<(), String> {
    let offset = guest_offset(ram, address, bytes.len())?;
    ram[offset..offset + bytes.len()].copy_from_slice(bytes);
    Ok(())
}

fn read_guest_u16(ram: &[u8], address: u64) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        read_guest_bytes(ram, address, 2)?.try_into().unwrap(),
    ))
}

fn write_guest_u16(ram: &mut [u8], address: u64, value: u16) -> Result<(), String> {
    write_guest_bytes(ram, address, &value.to_le_bytes())
}

fn write_guest_u32(ram: &mut [u8], address: u64, value: u32) -> Result<(), String> {
    write_guest_bytes(ram, address, &value.to_le_bytes())
}

pub struct AttachedVirtioBlk {
    pub base: u64,
    pub irq: u32,
    pub device: VirtioBlkDevice,
}

impl AttachedVirtioBlk {
    pub fn contains(&self, address: u64) -> bool {
        (self.base..self.base + 0x200).contains(&address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RAM_BASE;
    use sha2::Digest;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::path::Path;

    fn put_u16(ram: &mut [u8], offset: usize, value: u16) {
        ram[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(ram: &mut [u8], offset: usize, value: u32) {
        ram[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(ram: &mut [u8], offset: usize, value: u64) {
        ram[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn put_descriptor(
        ram: &mut [u8],
        table: usize,
        index: usize,
        address: u64,
        length: u32,
        flags: u16,
        next: u16,
    ) {
        let offset = table + index * 16;
        put_u64(ram, offset, address);
        put_u32(ram, offset + 8, length);
        put_u16(ram, offset + 12, flags);
        put_u16(ram, offset + 14, next);
    }

    fn init_device(device: &mut VirtioBlkDevice, ram: &mut [u8], desc: u64, avail: u64, used: u64) {
        device.write_mmio(MMIO_STATUS, 1, ram).unwrap();
        device.write_mmio(MMIO_STATUS, 3, ram).unwrap();
        device.write_mmio(MMIO_DEVICE_FEATURES_SEL, 0, ram).unwrap();
        let offered_low = device.read_mmio(MMIO_DEVICE_FEATURES);
        device.write_mmio(MMIO_DRIVER_FEATURES_SEL, 0, ram).unwrap();
        device
            .write_mmio(MMIO_DRIVER_FEATURES, offered_low, ram)
            .unwrap();
        device.write_mmio(MMIO_DEVICE_FEATURES_SEL, 1, ram).unwrap();
        let offered_high = device.read_mmio(MMIO_DEVICE_FEATURES);
        device.write_mmio(MMIO_DRIVER_FEATURES_SEL, 1, ram).unwrap();
        device
            .write_mmio(MMIO_DRIVER_FEATURES, offered_high, ram)
            .unwrap();
        device.write_mmio(MMIO_STATUS, 11, ram).unwrap();
        assert_eq!(device.read_mmio(MMIO_STATUS), 11, "FEATURES_OK accepted");

        device.write_mmio(MMIO_QUEUE_SEL, 0, ram).unwrap();
        assert_eq!(device.read_mmio(MMIO_QUEUE_NUM_MAX), 256);
        device.write_mmio(MMIO_QUEUE_NUM, 8, ram).unwrap();
        write_address(device, MMIO_QUEUE_DESC_LOW, desc, ram);
        write_address(device, MMIO_QUEUE_AVAIL_LOW, avail, ram);
        write_address(device, MMIO_QUEUE_USED_LOW, used, ram);
        device.write_mmio(MMIO_QUEUE_READY, 1, ram).unwrap();
        assert_eq!(device.read_mmio(MMIO_QUEUE_READY), 1);
        device.write_mmio(MMIO_STATUS, 15, ram).unwrap();
    }

    #[test]
    fn qemu_mmio_init_and_read_lba0_from_real_system_image() {
        let image = Path::new("/Users/Shared/android14-gsi/system.img");
        let mut expected_lba0 = [0u8; 512];
        let mut expected_lba8 = [0u8; 512];
        let mut image_file = File::open(image).expect("T271 requires the real Android system.img");
        image_file
            .read_exact(&mut expected_lba0)
            .expect("read image sector zero");
        image_file.seek(SeekFrom::Start(8 * SECTOR_SIZE)).unwrap();
        image_file.read_exact(&mut expected_lba8).unwrap();
        let mut device = VirtioBlkDevice::open(image, true).expect("open system.img read-only");

        assert_eq!(device.read_mmio(MMIO_MAGIC_VALUE), 0x7472_6976);
        assert_eq!(device.read_mmio(MMIO_VERSION), 2);
        assert_eq!(device.read_mmio(MMIO_DEVICE_ID), 2);
        assert_eq!(device.read_mmio(MMIO_VENDOR_ID), 0x554d_4551);
        assert_eq!(device.read_mmio(MMIO_QUEUE_NUM_MAX), 256);
        assert_eq!(device.read_mmio(MMIO_CONFIG), 4_023_080);
        assert_eq!(device.read_mmio(MMIO_CONFIG + 4), 0);
        assert_eq!(device.read_mmio(MMIO_CONFIG + 20), 512);

        let mut ram = vec![0u8; 0x10_000];
        let desc = 0x1000;
        let avail = 0x2000;
        let used = 0x3000;
        let header = 0x4000;
        let data = 0x5000;
        let status = 0x6000;
        let header_lba8 = 0x7000;
        let data_lba8 = 0x8000;
        let status_lba8 = 0x9000;

        put_u32(&mut ram, header, 0); // VIRTIO_BLK_T_IN
        put_u32(&mut ram, header + 4, 0);
        put_u64(&mut ram, header + 8, 0); // LBA 0
        put_descriptor(&mut ram, desc, 0, RAM_BASE + header as u64, 16, 1, 1);
        put_descriptor(&mut ram, desc, 1, RAM_BASE + data as u64, 512, 3, 2);
        put_descriptor(&mut ram, desc, 2, RAM_BASE + status as u64, 1, 2, 0);
        put_u32(&mut ram, header_lba8, 0); // VIRTIO_BLK_T_IN
        put_u32(&mut ram, header_lba8 + 4, 0);
        put_u64(&mut ram, header_lba8 + 8, 8); // first non-zero ext4 sector
        put_descriptor(&mut ram, desc, 3, RAM_BASE + header_lba8 as u64, 16, 1, 4);
        put_descriptor(&mut ram, desc, 4, RAM_BASE + data_lba8 as u64, 512, 3, 5);
        put_descriptor(&mut ram, desc, 5, RAM_BASE + status_lba8 as u64, 1, 2, 0);
        put_u16(&mut ram, avail + 2, 2);
        put_u16(&mut ram, avail + 4, 0);
        put_u16(&mut ram, avail + 6, 3);

        init_device(
            &mut device,
            &mut ram,
            RAM_BASE + desc as u64,
            RAM_BASE + avail as u64,
            RAM_BASE + used as u64,
        );
        device.write_mmio(MMIO_QUEUE_NOTIFY, 0, &mut ram).unwrap();

        assert_eq!(
            &ram[data..data + 512],
            &expected_lba0,
            "LBA 0 must match system.img byte-for-byte"
        );
        assert_eq!(
            &ram[data_lba8..data_lba8 + 512],
            &expected_lba8,
            "the non-zero LBA 8 sector must match system.img"
        );
        assert!(expected_lba8.iter().any(|byte| *byte != 0));
        assert_eq!(ram[status], 0, "virtio-blk request status");
        assert_eq!(ram[status_lba8], 0, "non-zero sector request status");
        assert_eq!(
            u16::from_le_bytes(ram[used + 2..used + 4].try_into().unwrap()),
            2
        );
        assert_eq!(device.read_mmio(MMIO_INTERRUPT_STATUS), 1);
        assert!(device.interrupt_line_asserted());
        let expected_hex = hex(&expected_lba0);
        let returned_hex = hex(&ram[data..data + 512]);
        let expected_hash = format!("{:x}", sha2::Sha256::digest(expected_lba0));
        let returned_hash = format!("{:x}", sha2::Sha256::digest(&ram[data..data + 512]));
        let lba8_expected_hex = hex(&expected_lba8);
        let lba8_returned_hex = hex(&ram[data_lba8..data_lba8 + 512]);
        let lba8_expected_hash = format!("{:x}", sha2::Sha256::digest(expected_lba8));
        let lba8_returned_hash = format!(
            "{:x}",
            sha2::Sha256::digest(&ram[data_lba8..data_lba8 + 512])
        );
        println!(
            "T271_LBA0_PROOF_JSON={}",
            serde_json::json!({
                "image_path": image.display().to_string(),
                "lba": 0,
                "bytes": 512,
                "expected_sha256": expected_hash,
                "returned_sha256": returned_hash,
                "expected_hex": expected_hex,
                "returned_hex": returned_hex,
                "lba8_expected_sha256": lba8_expected_hash,
                "lba8_returned_sha256": lba8_returned_hash,
                "lba8_expected_hex": lba8_expected_hex,
                "lba8_returned_hex": lba8_returned_hex,
                "used_idx": 2,
                "status": 0,
                "interrupt_status": 1
            })
        );
        device.write_mmio(MMIO_INTERRUPT_ACK, 1, &mut ram).unwrap();
        assert_eq!(device.read_mmio(MMIO_INTERRUPT_STATUS), 0);
        assert!(!device.interrupt_line_asserted());
    }

    #[test]
    fn writable_sparse_data_disk_completes_write_request() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let image = std::env::temp_dir().join(format!(
            "t271-rustblk-data-{}-{nonce}.img",
            std::process::id()
        ));
        VirtioBlkDevice::create_sparse_image(&image, 4096).unwrap();
        let mut device = VirtioBlkDevice::open(&image, false).unwrap();
        let mut ram = vec![0u8; 0x10_000];
        let desc = 0x1000;
        let avail = 0x2000;
        let used = 0x3000;
        let header = 0x4000;
        let data = 0x5000;
        let status = 0x6000;
        let payload: Vec<u8> = (0..512).map(|index| (index % 251) as u8).collect();

        put_u32(&mut ram, header, VIRTIO_BLK_T_OUT);
        put_u32(&mut ram, header + 4, 0);
        put_u64(&mut ram, header + 8, 2); // LBA 2
        ram[data..data + payload.len()].copy_from_slice(&payload);
        put_descriptor(&mut ram, desc, 0, RAM_BASE + header as u64, 16, 1, 1);
        put_descriptor(&mut ram, desc, 1, RAM_BASE + data as u64, 512, 1, 2);
        put_descriptor(&mut ram, desc, 2, RAM_BASE + status as u64, 1, 2, 0);
        put_u16(&mut ram, avail + 2, 1);
        put_u16(&mut ram, avail + 4, 0);

        init_device(
            &mut device,
            &mut ram,
            RAM_BASE + desc as u64,
            RAM_BASE + avail as u64,
            RAM_BASE + used as u64,
        );
        device.write_mmio(MMIO_QUEUE_NOTIFY, 0, &mut ram).unwrap();
        assert_eq!(ram[status], VIRTIO_BLK_S_OK);
        assert_eq!(device.read_mmio(MMIO_INTERRUPT_STATUS), 1);

        let mut persisted = [0u8; 512];
        let mut backing = File::open(&image).unwrap();
        backing.seek(SeekFrom::Start(2 * SECTOR_SIZE)).unwrap();
        backing.read_exact(&mut persisted).unwrap();
        assert_eq!(persisted.as_slice(), payload.as_slice());
        drop(device);
        std::fs::remove_file(image).unwrap();
    }

    fn write_address(device: &mut VirtioBlkDevice, offset: u32, address: u64, ram: &mut [u8]) {
        device.write_mmio(offset, address as u32, ram).unwrap();
        device
            .write_mmio(offset + 4, (address >> 32) as u32, ram)
            .unwrap();
    }

    fn hex(bytes: &[u8]) -> String {
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(encoded, "{byte:02x}").unwrap();
        }
        encoded
    }
}
