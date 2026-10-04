//! Full Bare-Metal System Integration Test: Track E Devices.
//!
//! Connects:
//! - GICv2 (Distributor at 0x0800_0000, CPU Interface at 0x0801_0000)
//! - ARM Generic Timer (PPI 27)
//! - VirtIO Console (MMIO at 0x0A00_0000, SPI 16 = INTID 48)
//! - VirtIO Block (MMIO at 0x0A00_0200, SPI 17 = INTID 49)
//! - Guest Physical RAM

use u5_gic_timer::gic::{
    Gic, GICC_CTLR, GICC_EOIR, GICC_IAR, GICC_PMR, GICD_CTLR, GICD_IPRIORITYR_START,
    GICD_ISENABLER_START, SPURIOUS_IRQ,
};
use u5_gic_timer::timer::{GenericTimer, TIMER_CTL_ENABLE, VIRT_TIMER_IRQ_NUM};
use u17_virtio_blk::{
    VirtioBlk, MMIO_INTERRUPT_ACK, SECTOR_SIZE, VIRTIO_BLK_DEVICE_ID, VIRTIO_BLK_IRQ,
    VIRTIO_BLK_S_OK, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT,
};
use u18_virtio_console::{
    VirtioConsole, QUEUE_TX, VIRTIO_CONSOLE_DEVICE_ID, VIRTIO_CONSOLE_IRQ,
};
use u6_virtio_transport::{
    MAGIC_VALUE, MMIO_DEVICE_ID, MMIO_MAGIC, MMIO_QUEUE_AVAIL_LO, MMIO_QUEUE_DESC_LO,
    MMIO_QUEUE_NOTIFY, MMIO_QUEUE_NUM, MMIO_QUEUE_READY, MMIO_QUEUE_SEL, MMIO_QUEUE_USED_LO,
    MMIO_STATUS, MMIO_VERSION, MMIO_VERSION_VALUE, VIRTIO_STATUS_ACKNOWLEDGE, VIRTIO_STATUS_DRIVER,
    VIRTIO_STATUS_DRIVER_OK,
};

struct BareMetalSystem {
    gic: Gic,
    timer: GenericTimer,
    console: VirtioConsole,
    blk: VirtioBlk,
    ram: Vec<u8>,
}

impl BareMetalSystem {
    fn new(ram_size: usize, disk_sectors: u64) -> Self {
        Self {
            gic: Gic::new(),
            timer: GenericTimer::new(),
            console: VirtioConsole::new(),
            blk: VirtioBlk::new(disk_sectors),
            ram: vec![0u8; ram_size],
        }
    }

    /// Step time: advance timer, update timer lines to GIC.
    fn step_time(&mut self, cycles: u64) {
        self.timer.step(cycles, &mut self.gic);
    }

    /// Sync peripheral interrupt lines to GIC.
    fn sync_irqs(&mut self) {
        self.gic.set_irq_level(VIRTIO_CONSOLE_IRQ, self.console.irq_line);
        self.gic.set_irq_level(VIRTIO_BLK_IRQ, self.blk.irq_line);
    }
}

#[test]
fn test_baremetal_all_devices_end_to_end() {
    let mut sys = BareMetalSystem::new(262144, 2048); // 256 KiB RAM, 1 MiB disk

    // =======================================================================
    // 1. Probe all device MMIO signatures
    // =======================================================================
    assert_eq!(sys.console.mmio_read(MMIO_MAGIC, 4), MAGIC_VALUE as u32);
    assert_eq!(sys.console.mmio_read(MMIO_VERSION, 4), MMIO_VERSION_VALUE as u32);
    assert_eq!(sys.console.mmio_read(MMIO_DEVICE_ID, 4), VIRTIO_CONSOLE_DEVICE_ID);

    assert_eq!(sys.blk.mmio_read(MMIO_MAGIC, 4), MAGIC_VALUE as u32);
    assert_eq!(sys.blk.mmio_read(MMIO_VERSION, 4), MMIO_VERSION_VALUE as u32);
    assert_eq!(sys.blk.mmio_read(MMIO_DEVICE_ID, 4), VIRTIO_BLK_DEVICE_ID);

    // =======================================================================
    // 2. Initialize GIC (Distributor & CPU Interface)
    // =======================================================================
    sys.gic.dist_write(GICD_CTLR, 1, 4);
    sys.gic.cpu_write(GICC_CTLR, 1, 4);
    sys.gic.cpu_write(GICC_PMR, 0xFF, 4); // allow all priority levels

    // Enable Timer PPI 27, Console SPI 48, Block SPI 49
    // Bank 0 (0..31): Timer PPI 27
    sys.gic.dist_write(GICD_ISENABLER_START, 1 << VIRT_TIMER_IRQ_NUM, 4);
    sys.gic.dist_write(GICD_IPRIORITYR_START + VIRT_TIMER_IRQ_NUM, 0x20, 1); // High priority (0x20)

    // Bank 1 (32..63): Console SPI 48, Block SPI 49
    let bank1_mask = (1 << (VIRTIO_CONSOLE_IRQ - 32)) | (1 << (VIRTIO_BLK_IRQ - 32));
    sys.gic.dist_write(GICD_ISENABLER_START + 4, bank1_mask, 4);
    sys.gic.dist_write(GICD_IPRIORITYR_START + VIRTIO_CONSOLE_IRQ, 0x40, 1); // Priority 0x40
    sys.gic.dist_write(GICD_IPRIORITYR_START + VIRTIO_BLK_IRQ, 0x60, 1); // Priority 0x60

    assert!(!sys.gic.cpu_irq_asserted(), "No IRQs before device actions");

    // =======================================================================
    // 3. Test Timer IRQ through GIC
    // =======================================================================
    sys.timer.write_cntv_tval(1000);
    sys.timer.set_cntv_ctl(TIMER_CTL_ENABLE);

    // Step 500 cycles -> not expired
    sys.step_time(500);
    assert!(!sys.gic.cpu_irq_asserted());

    // Step 500 more cycles -> hits compare
    sys.step_time(500);
    assert!(sys.gic.cpu_irq_asserted(), "CPU IRQ must assert on timer match");

    // CPU ISR acknowledges interrupt:
    let intid = sys.gic.cpu_read(GICC_IAR, 4);
    assert_eq!(intid, VIRT_TIMER_IRQ_NUM, "Acknowledged IRQ must be Virtual Timer PPI 27");

    // ISR re-arms timer:
    sys.timer.write_cntv_tval(10000);
    sys.step_time(0); // sync deasserted line
    sys.gic.cpu_write(GICC_EOIR, VIRT_TIMER_IRQ_NUM, 4);
    assert_eq!(sys.gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);

    // =======================================================================
    // 4. Test VirtIO Console TX through GIC
    // =======================================================================
    // Negotiate console driver
    sys.console.mmio_write(
        MMIO_STATUS,
        (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
        4,
        &mut sys.ram,
    );

    // Setup Console Queue 1 (TX)
    let con_desc = 0x1000u64;
    let con_avail = 0x1800u64;
    let con_used = 0x2000u64;
    sys.console.mmio_write(MMIO_QUEUE_SEL, QUEUE_TX as u32, 4, &mut sys.ram);
    sys.console.mmio_write(MMIO_QUEUE_NUM, 16, 4, &mut sys.ram);
    sys.console.mmio_write(MMIO_QUEUE_DESC_LO, con_desc as u32, 4, &mut sys.ram);
    sys.console.mmio_write(MMIO_QUEUE_AVAIL_LO, con_avail as u32, 4, &mut sys.ram);
    sys.console.mmio_write(MMIO_QUEUE_USED_LO, con_used as u32, 4, &mut sys.ram);
    sys.console.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut sys.ram);

    // Transmit "Booting bare-metal kernel\n"
    let banner = b"Booting bare-metal kernel\n";
    let banner_addr = 0x3000u64;
    sys.ram[banner_addr as usize..banner_addr as usize + banner.len()].copy_from_slice(banner);

    // Descriptor 0
    let d0 = con_desc as usize;
    sys.ram[d0..d0 + 8].copy_from_slice(&banner_addr.to_le_bytes());
    sys.ram[d0 + 8..d0 + 12].copy_from_slice(&(banner.len() as u32).to_le_bytes());
    sys.ram[d0 + 12..d0 + 16].copy_from_slice(&0u32.to_le_bytes());

    // Avail ring
    let a0 = con_avail as usize;
    sys.ram[a0 + 2..a0 + 4].copy_from_slice(&1u16.to_le_bytes()); // idx = 1
    sys.ram[a0 + 4..a0 + 6].copy_from_slice(&0u16.to_le_bytes()); // ring[0] = 0

    // Notify Console Queue 1
    sys.console.mmio_write(MMIO_QUEUE_NOTIFY, QUEUE_TX as u32, 4, &mut sys.ram);
    sys.sync_irqs();

    // Verify console received string
    assert_eq!(sys.console.tx_string(), "Booting bare-metal kernel\n");

    // Verify GIC received SPI 48
    assert!(sys.gic.cpu_irq_asserted(), "GIC must signal CPU IRQ for Console TX");
    let intid = sys.gic.cpu_read(GICC_IAR, 4);
    assert_eq!(intid, VIRTIO_CONSOLE_IRQ, "Acknowledged IRQ must be Console SPI 48");

    // Console ISR acknowledges MMIO interrupt and clears IRQ line
    sys.console.mmio_write(u18_virtio_console::MMIO_INTERRUPT_ACK, 1, 4, &mut sys.ram);
    sys.sync_irqs();
    sys.gic.cpu_write(GICC_EOIR, VIRTIO_CONSOLE_IRQ, 4);
    assert_eq!(sys.gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);

    // =======================================================================
    // 5. Test VirtIO Block Write and Read through GIC
    // =======================================================================
    sys.blk.mmio_write(
        MMIO_STATUS,
        (VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK) as u32,
        4,
        &mut sys.ram,
    );

    // Setup Block Queue 0 (requestq)
    let blk_desc = 0x5000u64;
    let blk_avail = 0x5800u64;
    let blk_used = 0x6000u64;
    sys.blk.mmio_write(MMIO_QUEUE_SEL, 0, 4, &mut sys.ram);
    sys.blk.mmio_write(MMIO_QUEUE_NUM, 16, 4, &mut sys.ram);
    sys.blk.mmio_write(MMIO_QUEUE_DESC_LO, blk_desc as u32, 4, &mut sys.ram);
    sys.blk.mmio_write(MMIO_QUEUE_AVAIL_LO, blk_avail as u32, 4, &mut sys.ram);
    sys.blk.mmio_write(MMIO_QUEUE_USED_LO, blk_used as u32, 4, &mut sys.ram);
    sys.blk.mmio_write(MMIO_QUEUE_READY, 1, 4, &mut sys.ram);

    // Write 512 bytes with 0x99 to sector 12
    let blk_hdr_addr = 0x7000u64;
    let blk_data_addr = 0x7100u64;
    let blk_status_addr = 0x7400u64;

    sys.ram[blk_hdr_addr as usize..blk_hdr_addr as usize + 4]
        .copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
    sys.ram[blk_hdr_addr as usize + 8..blk_hdr_addr as usize + 16]
        .copy_from_slice(&12u64.to_le_bytes()); // sector 12

    let test_data = vec![0x99u8; SECTOR_SIZE];
    sys.ram[blk_data_addr as usize..blk_data_addr as usize + SECTOR_SIZE]
        .copy_from_slice(&test_data);
    sys.ram[blk_status_addr as usize] = 0xFF;

    // Descriptors for write
    let bd0 = blk_desc as usize;
    sys.ram[bd0..bd0 + 8].copy_from_slice(&blk_hdr_addr.to_le_bytes());
    sys.ram[bd0 + 8..bd0 + 12].copy_from_slice(&16u32.to_le_bytes());
    sys.ram[bd0 + 12..bd0 + 14].copy_from_slice(&1u16.to_le_bytes()); // NEXT
    sys.ram[bd0 + 14..bd0 + 16].copy_from_slice(&1u16.to_le_bytes());

    let bd1 = blk_desc as usize + 16;
    sys.ram[bd1..bd1 + 8].copy_from_slice(&blk_data_addr.to_le_bytes());
    sys.ram[bd1 + 8..bd1 + 12].copy_from_slice(&(SECTOR_SIZE as u32).to_le_bytes());
    sys.ram[bd1 + 12..bd1 + 14].copy_from_slice(&1u16.to_le_bytes()); // NEXT
    sys.ram[bd1 + 14..bd1 + 16].copy_from_slice(&2u16.to_le_bytes());

    let bd2 = blk_desc as usize + 32;
    sys.ram[bd2..bd2 + 8].copy_from_slice(&blk_status_addr.to_le_bytes());
    sys.ram[bd2 + 8..bd2 + 12].copy_from_slice(&1u32.to_le_bytes());
    sys.ram[bd2 + 12..bd2 + 14].copy_from_slice(&2u16.to_le_bytes()); // WRITE

    // Put head 0 in avail ring
    let ba = blk_avail as usize;
    sys.ram[ba + 2..ba + 4].copy_from_slice(&1u16.to_le_bytes());
    sys.ram[ba + 4..ba + 6].copy_from_slice(&0u16.to_le_bytes());

    // Notify Block queue 0
    sys.blk.mmio_write(MMIO_QUEUE_NOTIFY, 0, 4, &mut sys.ram);
    sys.sync_irqs();

    assert_eq!(sys.ram[blk_status_addr as usize], VIRTIO_BLK_S_OK);
    assert_eq!(
        &sys.blk.disk_data[12 * SECTOR_SIZE..(12 + 1) * SECTOR_SIZE],
        &test_data[..]
    );

    // Verify GIC received SPI 49
    assert!(sys.gic.cpu_irq_asserted(), "GIC must signal CPU IRQ for Block Write");
    let intid = sys.gic.cpu_read(GICC_IAR, 4);
    assert_eq!(intid, VIRTIO_BLK_IRQ, "Acknowledged IRQ must be Block SPI 49");

    // Block ISR acknowledges MMIO interrupt and clears IRQ line
    sys.blk.mmio_write(MMIO_INTERRUPT_ACK, 1, 4, &mut sys.ram);
    sys.sync_irqs();
    sys.gic.cpu_write(GICC_EOIR, VIRTIO_BLK_IRQ, 4);
    assert_eq!(sys.gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);

    // =======================================================================
    // 6. Test Block Read Verification
    // =======================================================================
    let read_buf_addr = 0x8000u64;
    let read_hdr_addr = 0x7800u64;
    let read_status_addr = 0x8400u64;

    sys.ram[read_hdr_addr as usize..read_hdr_addr as usize + 4]
        .copy_from_slice(&VIRTIO_BLK_T_IN.to_le_bytes());
    sys.ram[read_hdr_addr as usize + 8..read_hdr_addr as usize + 16]
        .copy_from_slice(&12u64.to_le_bytes()); // sector 12

    let bd3 = blk_desc as usize + 48;
    sys.ram[bd3..bd3 + 8].copy_from_slice(&read_hdr_addr.to_le_bytes());
    sys.ram[bd3 + 8..bd3 + 12].copy_from_slice(&16u32.to_le_bytes());
    sys.ram[bd3 + 12..bd3 + 14].copy_from_slice(&1u16.to_le_bytes());
    sys.ram[bd3 + 14..bd3 + 16].copy_from_slice(&4u16.to_le_bytes());

    let bd4 = blk_desc as usize + 64;
    sys.ram[bd4..bd4 + 8].copy_from_slice(&read_buf_addr.to_le_bytes());
    sys.ram[bd4 + 8..bd4 + 12].copy_from_slice(&(SECTOR_SIZE as u32).to_le_bytes());
    sys.ram[bd4 + 12..bd4 + 14].copy_from_slice(&(1u16 | 2u16).to_le_bytes()); // NEXT | WRITE
    sys.ram[bd4 + 14..bd4 + 16].copy_from_slice(&5u16.to_le_bytes());

    let bd5 = blk_desc as usize + 80;
    sys.ram[bd5..bd5 + 8].copy_from_slice(&read_status_addr.to_le_bytes());
    sys.ram[bd5 + 8..bd5 + 12].copy_from_slice(&1u32.to_le_bytes());
    sys.ram[bd5 + 12..bd5 + 14].copy_from_slice(&2u16.to_le_bytes()); // WRITE

    // Avail slot 1 -> desc 3
    sys.ram[ba + 2..ba + 4].copy_from_slice(&2u16.to_le_bytes());
    sys.ram[ba + 6..ba + 8].copy_from_slice(&3u16.to_le_bytes());

    sys.blk.mmio_write(MMIO_QUEUE_NOTIFY, 0, 4, &mut sys.ram);
    sys.sync_irqs();

    assert_eq!(sys.ram[read_status_addr as usize], VIRTIO_BLK_S_OK);
    assert_eq!(
        &sys.ram[read_buf_addr as usize..read_buf_addr as usize + SECTOR_SIZE],
        &test_data[..],
        "Read data must match written data"
    );

    let intid = sys.gic.cpu_read(GICC_IAR, 4);
    assert_eq!(intid, VIRTIO_BLK_IRQ);
    sys.blk.mmio_write(MMIO_INTERRUPT_ACK, 1, 4, &mut sys.ram);
    sys.sync_irqs();
    sys.gic.cpu_write(GICC_EOIR, VIRTIO_BLK_IRQ, 4);
    assert_eq!(sys.gic.cpu_read(GICC_IAR, 4), SPURIOUS_IRQ);
}
