//! Platform constants for the U9 bare-metal guest.
//!
//! This module is the single source of truth for the guest's physical memory
//! map. `PLATFORM.md` (same directory) is the human/device-side contract that
//! leaf 5.1 implements against; every address there is mirrored by a constant
//! here, and `image_platform_ranges` tests that the two agree on the critical
//! invariants (entry inside RAM, console MMIO outside RAM).

/// Guest RAM base (ARM virt machine RAM starts here).
pub const RAM_BASE: u64 = 0x4000_0000;
/// Guest RAM size: 128 MiB.
pub const RAM_SIZE: u64 = 0x0800_0000;
/// Top of RAM (exclusive). The initial stack pointer points here.
pub const STACK_TOP: u64 = RAM_BASE + RAM_SIZE;

/// Guest load address == image entry point (start of RAM).
pub const GUEST_LOAD_ADDR: u64 = RAM_BASE;

/// Console MMIO page base. TX/RX registers live here, outside RAM.
pub const CONSOLE_BASE: u64 = 0x0900_0000;
/// Console MMIO region size (one 4 KiB page).
pub const CONSOLE_SIZE: u64 = 0x1000;
/// Transmit register: STRB a byte here -> the host emits it on the console.
pub const CONSOLE_TX: u64 = CONSOLE_BASE;
/// Receive register: LDRB here -> next input byte, or 0 if none available.
pub const CONSOLE_RX: u64 = CONSOLE_BASE + 8;

/// Offset of the guest data section from the load address (one page).
/// Code must fit below this; `image_guest_code_fits` enforces it.
pub const DATA_OFFSET: u64 = 0x1000;
/// Absolute address of the guest data section.
pub const DATA_BASE: u64 = GUEST_LOAD_ADDR + DATA_OFFSET;

/// Image format version emitted by [`crate::image::build`].
pub const IMAGE_VERSION: u32 = 1;
/// Image header magic: ASCII "PNIM" (little-endian u32 0x4D494E50).
pub const IMAGE_MAGIC: [u8; 4] = *b"PNIM";
/// Image header length in bytes: magic(4) + version(4) + entry(8) + size(8).
pub const HEADER_LEN: usize = 24;

/// True if `addr` lies inside guest RAM.
pub fn in_ram(addr: u64) -> bool {
    (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&addr)
}

/// True if the half-open ranges `[a0, a1)` and `[b0, b1)` overlap.
pub fn ranges_overlap(a0: u64, a1: u64, b0: u64, b1: u64) -> bool {
    a0 < b1 && b0 < a1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_platform_entry_inside_ram() {
        assert!(in_ram(GUEST_LOAD_ADDR));
        assert!(in_ram(STACK_TOP - 1));
        assert!(!in_ram(RAM_BASE + RAM_SIZE));
    }

    #[test]
    fn image_platform_console_outside_ram() {
        assert!(!ranges_overlap(
            CONSOLE_BASE,
            CONSOLE_BASE + CONSOLE_SIZE,
            RAM_BASE,
            RAM_BASE + RAM_SIZE
        ));
        assert_eq!(CONSOLE_TX, CONSOLE_BASE);
        assert_eq!(CONSOLE_RX, CONSOLE_BASE + 8);
    }

    #[test]
    fn image_platform_data_page_aligned() {
        assert_eq!(DATA_BASE & 0xFFF, 0);
        assert!(in_ram(DATA_BASE));
    }
}
