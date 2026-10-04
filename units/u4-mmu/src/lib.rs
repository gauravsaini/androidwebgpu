//! U4 `mmu` — AArch64 stage-1 virtual-to-physical address translation.
//!
//! Purity class: EXPLICIT-STATE. Every input is explicit: `&MmuState` carries
//! the registers, `ram` carries the page tables, `va` and `access` are plain
//! values. No globals, no I/O, no time, no threads; same inputs always give
//! the same output. All failures are data ([`MemFault`]), never panics.
//!
//! Two entry points:
//! - [`translate`] assumes the `ram` slice starts at guest-physical address
//!   0 (the unit-test convention).
//! - [`translate_with_base`] takes an explicit `ram_base`: guest-physical
//!   addresses (table descriptors, TTBRs) are rebased by
//!   `pa.checked_sub(ram_base)` before indexing `ram`. A descriptor PA below
//!   `ram_base` is a [`MemFault::TranslationFault`], never a panic or a
//!   wraparound read. Production callers (whose RAM slice starts at
//!   `RAM_BASE = 0x4000_0000`) must use this entry point.
//!
//! Documented model (vs the ARM ARM VMSA):
//! - Stage 1 only. `Access` carries no exception level, so the model is
//!   EL1-equivalent: `AP[7:6]` of `0b00`/`0b01` allows read+write,
//!   `0b10`/`0b11` is read-only. `PXN` (bit 53) denies `Access::Execute`;
//!   `UXN` (bit 54) does NOT deny execute here because it only restricts
//!   EL0, which this model has no concept of (AArch64 Linux maps all
//!   kernel text `UXN=1,PXN=0`; treating UXN as EL1 execute-never
//!   faulted every kernel fetch once the final page tables were live).
//! - Table-descriptor hierarchical controls accumulate down the walk:
//!   `PXNTable` (bit 59) ORs into the execute-never flag; `UXNTable`/
//!   `XNTable` (bit 60) is ignored for the same EL0-only reason.
//!   (`APTable[1]` accumulates a no-write flag. `APTable[0]` only
//!   restricts EL0, which this model has no concept of.)
//! - `SCTLR.M == 0` means the MMU is off: translation is the identity map.
//! - Granule comes from `TCR.TG0`/`TCR.TG1`; input address size from
//!   `TCR.T0SZ`/`TCR.T1SZ` (`IA = 64 - TxSZ`, capped at 48 bits — 52-bit LVA
//!   is not modeled). Reserved granule encodings fault.
//! - A misaligned `TTBR` or table-descriptor base is a `TranslationFault`
//!   (hardware treats this as a programming error; we make it data).
//! - Only page-table *reads* are bounds-checked against `ram`. An
//!   out-of-range table read is `MemFault::TranslationFault`, never a panic.
//!   The final output address is NOT bounds-checked: it may target MMIO.
//! - Output addresses are masked to 48 bits (pre-LVA physical address size).
//! - Ignored (do not affect address or fault decision): contiguous bit, nG,
//!   shareability, memory-type attributes, `NSTable`.

use pathn_contracts::cpu::{Access, MemFault, MmuState};

/// Translation granule, identified by its page offset width.
/// (Per-level index widths live in [`index_range`], straight from the ARM
/// granule table — the top-level index is narrower than the rest.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Granule {
    page_shift: u32,
}

impl Granule {
    /// Decode `TCR.TG0` (upper == false) or `TCR.TG1` (upper == true).
    /// Reserved encodings yield `None`.
    fn decode(tg: u64, upper: bool) -> Option<Granule> {
        // TG0: 00=4K 01=64K 10=16K 11=reserved
        // TG1: 00=reserved 01=16K 10=4K 11=64K
        match (upper, tg) {
            (false, 0b00) | (true, 0b10) => Some(Granule { page_shift: 12 }),
            (false, 0b10) | (true, 0b01) => Some(Granule { page_shift: 14 }),
            (false, 0b01) | (true, 0b11) => Some(Granule { page_shift: 16 }),
            _ => None,
        }
    }

    fn bytes(self) -> u64 {
        1u64 << self.page_shift
    }
}

/// Index bit range `[hi:lo]` for one table level, per the ARM
/// "Learn the Architecture: AArch64 memory management" granule table.
/// 64K L1 is capped at bit 47 (no 52-bit LVA in this model).
fn index_range(granule: Granule, level: u32) -> Option<(u32, u32)> {
    match (granule.page_shift, level) {
        (12, 0) => Some((47, 39)),
        (12, 1) => Some((38, 30)),
        (12, 2) => Some((29, 21)),
        (12, 3) => Some((20, 12)),
        (14, 0) => Some((47, 47)),
        (14, 1) => Some((46, 36)),
        (14, 2) => Some((35, 25)),
        (14, 3) => Some((24, 14)),
        (16, 1) => Some((47, 42)),
        (16, 2) => Some((41, 29)),
        (16, 3) => Some((28, 16)),
        _ => None,
    }
}

/// Highest table level whose index range resolves bit `ia - 1`
/// (the top bit of the input address). `None` = IA too small for any level.
fn start_level(granule: Granule, ia_bits: u32) -> Option<u32> {
    if ia_bits == 0 {
        return None;
    }
    let top = ia_bits - 1;
    for level in 0..=3 {
        if let Some((hi, lo)) = index_range(granule, level) {
            if lo <= top && top <= hi {
                return Some(level);
            }
        }
    }
    None
}

/// Read 8 bytes at guest-physical `pa` from `ram` as little-endian u64.
/// The `ram` slice starts at guest-physical `ram_base`; `pa` is rebased by
/// `checked_sub`, so a PA below `ram_base` is a `TranslationFault`, never a
/// panic or a wraparound read. Out-of-range reads are `TranslationFault`,
/// never panics.
fn read_desc(ram: &[u8], pa: u64, va: u64, ram_base: u64) -> Result<u64, MemFault> {
    let rebased = pa
        .checked_sub(ram_base)
        .ok_or(MemFault::TranslationFault { va })?;
    let end = rebased
        .checked_add(8)
        .ok_or(MemFault::TranslationFault { va })?;
    if end > ram.len() as u64 {
        return Err(MemFault::TranslationFault { va });
    }
    // Bounds proven above; `rebased <= ram.len()` always fits in `usize`
    // because `ram.len()` itself is a `usize`.
    let s = rebased as usize;
    let mut b = [0u8; 8];
    b.copy_from_slice(&ram[s..s + 8]);
    Ok(u64::from_le_bytes(b))
}

/// Translate `va` to a guest-physical address per the AArch64 VMSA stage-1
/// walk described in the module docs. Unit-test convention: the `ram`
/// slice starts at guest-physical address 0.
pub fn translate(state: &MmuState, ram: &[u8], va: u64, access: Access) -> Result<u64, MemFault> {
    translate_impl(state, ram, va, access, 0)
}

/// Translate `va` to a guest-physical address where the `ram` slice starts
/// at guest-physical `ram_base` (e.g. `0x4000_0000`). All descriptor PAs —
/// TTBR bases, table addresses, entry addresses — are rebased before
/// indexing `ram`; a PA below `ram_base` or outside the slice is a
/// [`MemFault::TranslationFault`]. The RETURNED address is a true
/// guest-physical address (not rebased): pass it on to the caller's
/// physical dispatch (MMIO / RAM offset), never as an index into `ram`.
pub fn translate_with_base(
    state: &MmuState,
    ram: &[u8],
    ram_base: u64,
    va: u64,
    access: Access,
) -> Result<u64, MemFault> {
    translate_impl(state, ram, va, access, ram_base)
}

/// Shared walk implementation. `ram_base` is the guest-physical address
/// the `ram` slice starts at; descriptor PAs are rebased against it.
fn translate_impl(
    state: &MmuState,
    ram: &[u8],
    va: u64,
    access: Access,
    ram_base: u64,
) -> Result<u64, MemFault> {
    // MMU off: flat identity map.
    if state.sctlr & 1 == 0 {
        return Ok(va);
    }

    // Region select: TTBR1 serves the top half (bit 63 set).
    let upper = va >> 63 == 1;
    let (ttbr, tsz, tg) = if upper {
        (
            state.ttbr1,
            (state.tcr >> 16) & 0x3F,
            (state.tcr >> 30) & 0x3,
        )
    } else {
        (state.ttbr0, state.tcr & 0x3F, (state.tcr >> 14) & 0x3)
    };
    let granule = Granule::decode(tg, upper).ok_or(MemFault::TranslationFault { va })?;
    if !(16..=63).contains(&tsz) {
        return Err(MemFault::TranslationFault { va });
    }
    let ia_bits = (64 - tsz) as u32;
    if ia_bits > 48 {
        return Err(MemFault::TranslationFault { va });
    }

    // VA must lie inside its region: low region all-zero top bits,
    // high region all-one top bits.
    if upper {
        if va >> ia_bits != u64::MAX >> ia_bits {
            return Err(MemFault::TranslationFault { va });
        }
    } else if va >> ia_bits != 0 {
        return Err(MemFault::TranslationFault { va });
    }

    let level0 = start_level(granule, ia_bits).ok_or(MemFault::TranslationFault { va })?;
    if ttbr & (granule.bytes() - 1) != 0 {
        return Err(MemFault::TranslationFault { va });
    }
    // Physical address bits only: attribute bits above bit 47 never leak
    // into table bases or output addresses.
    let addr_mask = 0x0000_FFFF_FFFF_FFFF & !(granule.bytes() - 1);

    let mut table_base = ttbr & addr_mask;
    // Hierarchical restrictions accumulated from table descriptors.
    let mut no_exec = false;
    let mut no_write = false;

    for level in level0..=3 {
        let (hi, lo) = index_range(granule, level).ok_or(MemFault::TranslationFault { va })?;
        let width = hi - lo + 1;
        let index = (va >> lo) & ((1u64 << width) - 1);
        let entry_pa = table_base
            .checked_add(index * 8)
            .ok_or(MemFault::TranslationFault { va })?;
        let desc = read_desc(ram, entry_pa, va, ram_base)?;

        match desc & 0b11 {
            0b00 | 0b10 => return Err(MemFault::TranslationFault { va }), // invalid / reserved
            0b01 => {
                // Block descriptor: valid at levels 0..2 only.
                if level == 3 {
                    return Err(MemFault::TranslationFault { va });
                }
                return finish_block(desc, va, lo, access, no_exec, no_write);
            }
            _ => {
                if level == 3 {
                    // Page descriptor.
                    return finish_block(desc, va, lo, access, no_exec, no_write);
                }
                // Table descriptor: descend. Bits [1:0] are the type tag;
                // the remaining low bits of the base must be zero.
                if desc & (granule.bytes() - 1) != 0b11 {
                    return Err(MemFault::TranslationFault { va });
                }
                // PXNTable (bit 59) denies EL1 execute below. UXNTable/XNTable
                // (bit 60) only restricts EL0: ignored in this EL1-only model.
                no_exec |= desc & (1u64 << 59) != 0; // PXNTable
                no_write |= desc & (1u64 << 61) != 0; // APTable[1]: no writes below
                table_base = desc & addr_mask;
            }
        }
    }
    // Unreachable: the level-3 iteration always returns.
    Err(MemFault::TranslationFault { va })
}

/// Permission check + output-address assembly for a block/page descriptor.
/// `lo` is the low index bit of this level: the block covers `va[lo-1:0]`
/// as offset and takes its base from the descriptor's high bits. Low
/// descriptor bits below the block size are masked (they are not address
/// bits); the result is additionally masked to 48 physical address bits.
fn finish_block(
    desc: u64,
    va: u64,
    lo: u32,
    access: Access,
    no_exec: bool,
    no_write: bool,
) -> Result<u64, MemFault> {
    let ap = (desc >> 6) & 0b11;
    let read_only = ap == 0b10 || ap == 0b11;
    // PXN (bit 53) denies EL1 execute. UXN (bit 54) only restricts EL0 and
    // is ignored: the kernel maps its own text UXN=1,PXN=0.
    let pxn = desc & (1u64 << 53) != 0; // PXN
    match access {
        Access::Write if read_only || no_write => {
            return Err(MemFault::PermissionFault { va });
        }
        Access::Execute if pxn || no_exec => {
            return Err(MemFault::PermissionFault { va });
        }
        _ => {}
    }
    let block_size = 1u64 << lo;
    let pa = (desc & !(block_size - 1)) | (va & (block_size - 1));
    Ok(pa & 0x0000_FFFF_FFFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w64(ram: &mut [u8], pa: u64, v: u64) {
        let s = pa as usize;
        ram[s..s + 8].copy_from_slice(&v.to_le_bytes());
    }

    fn table(base: u64) -> u64 {
        base | 0b11
    }

    fn page(out: u64, ap: u64) -> u64 {
        (out & !0xFFF) | (ap << 6) | 0b11
    }

    /// 4K granule, 48-bit IA: T0SZ=16, TG0=4K.
    fn st4k(ttbr0: u64) -> MmuState {
        MmuState {
            ttbr0,
            ttbr1: 0,
            tcr: 16,
            sctlr: 1,
        }
    }

    /// Build the canonical 4-level chain used by most golden tests:
    /// L0@0x1000 -> L1@0x2000 -> L2@0x3000 -> L3@0x4000, mapping
    /// va 0x401234 -> pa 0x800000 + offset via L3[1].
    /// Returns (ram, l3_page_pa) so tests can rewrite the leaf descriptor.
    fn chain4k() -> (Vec<u8>, u64) {
        let mut ram = vec![0u8; 0x10000];
        w64(&mut ram, 0x1000, table(0x2000)); // L0[0]
        w64(&mut ram, 0x2000, table(0x3000)); // L1[0]
        w64(&mut ram, 0x3000 + 2 * 8, table(0x4000)); // L2[2]
        let leaf = 0x4000 + 8; // L3[1]
        w64(&mut ram, leaf, page(0x800000, 0b00));
        (ram, leaf)
    }

    #[test]
    fn golden_valid_translation_4k_four_levels() {
        let (ram, _) = chain4k();
        // L0[0] L1[0] L2[2] L3[1], offset 0x234.
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x800234)
        );
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Write),
            Ok(0x800234)
        );
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Execute),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_permission_fault_write_to_readonly_page() {
        let (mut ram, leaf) = chain4k();
        w64(&mut ram, leaf, page(0x800000, 0b10)); // AP=10: read-only
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Write),
            Err(MemFault::PermissionFault { va: 0x401234 })
        );
        // Reads still succeed on a read-only page.
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_execute_allowed_on_uxn_page() {
        // UXN (bit 54) only restricts EL0: in this EL1-equivalent model a
        // UXN=1,PXN=0 page (how Linux maps kernel text) is executable.
        // Regression test for the FetchFault at 0xffffff8009456158.
        let (mut ram, leaf) = chain4k();
        w64(&mut ram, leaf, page(0x800000, 0b00) | (1u64 << 54)); // UXN
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Execute),
            Ok(0x800234)
        );
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_permission_fault_execute_pxn_page() {
        // PXN (bit 53) denies EL1 execute.
        let (mut ram, leaf) = chain4k();
        w64(&mut ram, leaf, page(0x800000, 0b00) | (1u64 << 53)); // PXN
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Execute),
            Err(MemFault::PermissionFault { va: 0x401234 })
        );
        // Data accesses are unaffected by PXN.
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_uxntable_does_not_deny_execute() {
        // UXNTable (bit 60) only restricts EL0: must not deny EL1 execute.
        let (mut ram, _) = chain4k();
        w64(&mut ram, 0x2000, table(0x3000) | (1u64 << 60));
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Execute),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_permission_fault_execute_pxn_table_accumulates() {
        let (mut ram, _) = chain4k();
        // L1[0] table descriptor with PXNTable set denies execute below.
        w64(&mut ram, 0x2000, table(0x3000) | (1u64 << 59));
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Execute),
            Err(MemFault::PermissionFault { va: 0x401234 })
        );
        // Data accesses are unaffected by PXNTable.
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x800234)
        );
    }

    #[test]
    fn golden_translation_fault_unmapped_descriptor() {
        let (mut ram, _) = chain4k();
        w64(&mut ram, 0x3000 + 2 * 8, 0); // L2[2] invalid
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }

    #[test]
    fn golden_translation_fault_table_base_out_of_bounds() {
        let (mut ram, _) = chain4k();
        // L1[0] points past the end of the 64 KiB ram slice.
        w64(&mut ram, 0x2000, table(0xF0000));
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }

    #[test]
    fn golden_translation_fault_misaligned_table_base() {
        let (mut ram, _) = chain4k();
        // L0[0] base has nonzero RES0 bits [11:2]: programming error -> fault.
        w64(&mut ram, 0x1000, 0x2100 | 0b11);
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }

    #[test]
    fn golden_block_descriptor_2mb() {
        let (mut ram, _) = chain4k();
        // L2[2] as a 2 MiB block: AP=00, output base 0x400000.
        w64(&mut ram, 0x3000 + 2 * 8, (0x400000u64 & !0x1F_FFFF) | 0b01);
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x401234, Access::Read),
            Ok(0x401234)
        );
    }

    #[test]
    fn golden_translation_fault_va_outside_region() {
        let (ram, _) = chain4k();
        // Bit 63 clear but VA exceeds the 48-bit TTBR0 range.
        assert_eq!(
            translate(&st4k(0x1000), &ram, 0x1_0000_0000_0000, Access::Read),
            Err(MemFault::TranslationFault {
                va: 0x1_0000_0000_0000
            })
        );
    }

    #[test]
    fn golden_valid_translation_ttbr1_top_region() {
        // T1SZ=16, TG1=4K; va with bit 63 set walks from TTBR1.
        let st = MmuState {
            ttbr0: 0,
            ttbr1: 0x5000,
            tcr: (16 << 16) | (0b10 << 30),
            sctlr: 1,
        };
        let mut ram = vec![0u8; 0x10000];
        let va = 0xFFFF_FFFF_FFFF_F000u64; // L0/L1/L2/L3 index all 511, offset 0
        w64(&mut ram, 0x5000 + 511 * 8, table(0x6000));
        w64(&mut ram, 0x6000 + 511 * 8, table(0x7000));
        w64(&mut ram, 0x7000 + 511 * 8, table(0x8000));
        w64(&mut ram, 0x8000 + 511 * 8, page(0x900000, 0b00));
        assert_eq!(translate(&st, &ram, va, Access::Read), Ok(0x900000));
    }

    #[test]
    fn golden_valid_translation_16k_granule() {
        // TG0=16K, T0SZ=17 (47-bit IA). Walk starts at L1.
        // 16K tables must be 16K-aligned.
        let st = MmuState {
            ttbr0: 0x4000,
            ttbr1: 0,
            tcr: (0b10 << 14) | 17,
            sctlr: 1,
        };
        let mut ram = vec![0u8; 0x10000];
        let va = 0x2401234u64; // L1[0] L2[1] L3[256], offset 0x1234
        w64(&mut ram, 0x4000, table(0x8000));
        w64(&mut ram, 0x8000 + 8, table(0xC000)); // L2[1]
        w64(&mut ram, 0xC000 + 256 * 8, (0xA00000u64 & !0x3FFF) | 0b11);
        assert_eq!(translate(&st, &ram, va, Access::Read), Ok(0xA01234));
    }

    #[test]
    fn golden_translation_fault_reserved_granule() {
        let st = MmuState {
            ttbr0: 0x1000,
            ttbr1: 0,
            tcr: (0b11 << 14) | 16, // TG0=11 reserved
            sctlr: 1,
        };
        let ram = vec![0u8; 0x10000];
        assert_eq!(
            translate(&st, &ram, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }

    #[test]
    fn golden_mmu_disabled_is_identity_map() {
        let st = MmuState {
            ttbr0: 0x1000,
            ttbr1: 0,
            tcr: 16,
            sctlr: 0, // SCTLR.M == 0
        };
        let ram = vec![0u8; 0x10000]; // no tables at all
        assert_eq!(
            translate(&st, &ram, 0xDEAD_BEEF, Access::Execute),
            Ok(0xDEAD_BEEF)
        );
    }

    // ---- GB-20: base-aware entry point ----

    /// Write helper rebased by `base`: stores `v` at guest-physical `pa`.
    fn w64_base(ram: &mut [u8], base: u64, pa: u64, v: u64) {
        let s = (pa - base) as usize;
        ram[s..s + 8].copy_from_slice(&v.to_le_bytes());
    }

    const RAM_BASE: u64 = 0x4000_0000;

    /// Real AOSP kernel V01 vector (GB-19 halt, step 7533):
    /// va 0xFFFF_FF80_096A_B158 -> pa 0x416A_B158 via TTBR1.
    /// SCTLR_EL1=0x34f5d91d (M=1), TCR_EL1=0x00400030b5593519
    /// (T1SZ=25, 39-bit IA, TG1=4K -> walk starts at level 1),
    /// TTBR1_EL1=0x4166a000. L1[0]=0x4166b003 -> L2 table at
    /// 0x4166b000; L2[75]=0x41600711 -> 2 MiB block, AF=1,
    /// AP=0b00 (read/write), physical block base 0x41600000,
    /// offset 0x0ab158 -> 0x416ab158.
    fn v01_state() -> MmuState {
        MmuState {
            ttbr0: 0x4166_5000,
            ttbr1: 0x4166_a000,
            tcr: 0x0040_0030_b559_3519,
            sctlr: 0x34f5_d91d,
        }
    }

    fn v01_ram() -> Vec<u8> {
        // Tables live at 0x4166a000..0x4166c000; a 0x1700000-byte slice
        // starting at RAM_BASE covers them at rebased offsets.
        let mut ram = vec![0u8; 0x170_0000];
        w64_base(&mut ram, RAM_BASE, 0x4166_a000, 0x4166_b003); // L1[0]
        w64_base(&mut ram, RAM_BASE, 0x4166_b258, 0x4160_0711); // L2[75]
        ram
    }

    #[test]
    fn gb20_translate_with_base_real_kernel_v01_vector() {
        let ram = v01_ram();
        let va = 0xFFFF_FF80_096A_B158u64;
        assert_eq!(
            translate_with_base(&v01_state(), &ram, RAM_BASE, va, Access::Read),
            Ok(0x416A_B158)
        );
        assert_eq!(
            translate_with_base(&v01_state(), &ram, RAM_BASE, va, Access::Write),
            Ok(0x416A_B158)
        );
    }

    #[test]
    fn gb20_translate_with_base_mmu_off_is_identity() {
        let st = MmuState {
            sctlr: 0,
            ..v01_state()
        };
        let ram = v01_ram();
        assert_eq!(
            translate_with_base(&st, &ram, RAM_BASE, 0xFFFF_FF80_096A_B158, Access::Read),
            Ok(0xFFFF_FF80_096A_B158)
        );
    }

    #[test]
    fn gb20_base_zero_matches_translate() {
        // translate_with_base with base 0 must equal the legacy entry point.
        let (ram, _) = chain4k();
        let st = st4k(0x1000);
        assert_eq!(
            translate_with_base(&st, &ram, 0, 0x401234, Access::Read),
            translate(&st, &ram, 0x401234, Access::Read)
        );
        assert_eq!(
            translate_with_base(&st, &ram, 0, 0x401234, Access::Write),
            translate(&st, &ram, 0x401234, Access::Write)
        );
    }

    #[test]
    fn gb20_base_underflow_is_translation_fault() {
        // TTBR0 below ram_base: rebasing underflows -> TranslationFault,
        // never a wraparound read.
        let st = MmuState {
            ttbr0: 0x1000, // < RAM_BASE
            ttbr1: 0,
            tcr: 16, // 4K, 48-bit, low region
            sctlr: 1,
        };
        let ram = vec![0u8; 0x10000];
        assert_eq!(
            translate_with_base(&st, &ram, RAM_BASE, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }

    #[test]
    fn gb20_base_out_of_range_is_translation_fault() {
        // TTBR0 rebases to an offset past the end of the slice.
        let st = MmuState {
            ttbr0: RAM_BASE + 0x1000, // rebased 0x1000 == slice len
            ttbr1: 0,
            tcr: 16,
            sctlr: 1,
        };
        let ram = vec![0u8; 0x1000];
        assert_eq!(
            translate_with_base(&st, &ram, RAM_BASE, 0x401234, Access::Read),
            Err(MemFault::TranslationFault { va: 0x401234 })
        );
    }
}
