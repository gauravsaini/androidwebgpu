//! U11 — snapshot: `MachineState` ↔ versioned blob.
//!
//! PURE: deterministic encode/decode. No I/O, no wall-clock, no threads,
//! no hidden state. A corrupt blob is data ([`SnapshotError`]), never a panic.
//!
//! Wire format v1 (all integers little-endian):
//! ```text
//! u32 version            == SNAPSHOT_VERSION (1)
//! u32 cpu_count
//! cpu_count × { 31×u64 regs, u64 sp, u64 pc, u64 pstate }   (272 bytes each)
//! u64 ttbr0, u64 ttbr1, u64 tcr, u64 sctlr
//! u32 irq_enabled, u64 irq_pending, u64 timer_count, u64 timer_compare
//! u64 ram_len, ram_len bytes
//! u32 device_count
//! device_count × { u8 kind_tag, u64 blob_len, blob_len bytes }
//! ```
//! `kind_tag`: 0=Block, 1=Net, 2=Gpu, 3=Input, 4=Console.
//! No trailing bytes are allowed: a snapshot is exact, and silently accepting
//! garbage is a forward-compatibility hazard.

use pathn_contracts::cpu::{IrqState, MmuState};
use pathn_contracts::machine::{
    CpuState, DeviceKind, DeviceState, MachineState, Snapshot, SnapshotError, MAX_GUEST_RAM_BYTES,
    SNAPSHOT_VERSION,
};

/// Encoded size of one [`CpuState`]: 31 regs + sp + pc + pstate, all u64LE.
const CPU_ENCODED_BYTES: usize = 34 * 8;
/// Minimum encoded size of one [`DeviceState`]: u8 tag + u64 blob length.
const DEVICE_MIN_BYTES: usize = 1 + 8;

fn kind_to_tag(kind: DeviceKind) -> u8 {
    match kind {
        DeviceKind::Block => 0,
        DeviceKind::Net => 1,
        DeviceKind::Gpu => 2,
        DeviceKind::Input => 3,
        DeviceKind::Console => 4,
    }
}

fn tag_to_kind(tag: u8) -> Option<DeviceKind> {
    match tag {
        0 => Some(DeviceKind::Block),
        1 => Some(DeviceKind::Net),
        2 => Some(DeviceKind::Gpu),
        3 => Some(DeviceKind::Input),
        4 => Some(DeviceKind::Console),
        _ => None,
    }
}

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    fn cpu(&mut self, c: &CpuState) {
        for r in c.regs {
            self.u64(r);
        }
        self.u64(c.sp);
        self.u64(c.pc);
        self.u64(c.pstate);
    }

    fn mmu(&mut self, m: &MmuState) {
        self.u64(m.ttbr0);
        self.u64(m.ttbr1);
        self.u64(m.tcr);
        self.u64(m.sctlr);
    }

    fn irq(&mut self, i: &IrqState) {
        self.u32(i.enabled);
        self.u64(i.pending);
        self.u64(i.timer_count);
        self.u64(i.timer_compare);
    }
}

/// Encode a [`MachineState`] into a versioned [`Snapshot`] blob.
///
/// Total and deterministic: every field of the state is encoded, and the same
/// state always yields byte-identical output.
pub fn snapshot(state: &MachineState) -> Snapshot {
    let mut w = Writer::new();
    w.u32(SNAPSHOT_VERSION);
    // A Vec<CpuState> longer than u32::MAX would need >1 TiB of backing store,
    // so the cast cannot truncate a representable value.
    w.u32(state.cpu.len() as u32);
    for cpu in &state.cpu {
        w.cpu(cpu);
    }
    w.mmu(&state.mmu);
    w.irq(&state.irq);
    w.u64(state.ram.len() as u64);
    w.bytes(&state.ram);
    w.u32(state.devices.len() as u32);
    for dev in &state.devices {
        w.u8(kind_to_tag(dev.kind));
        w.u64(dev.blob.len() as u64);
        w.bytes(&dev.blob);
    }
    Snapshot(w.buf)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], SnapshotError> {
        if n > self.remaining() {
            return Err(SnapshotError::Corrupt);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, SnapshotError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, SnapshotError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, SnapshotError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    /// Validate a u64 length prefix, then read that many bytes. A declared
    /// length past the 2 GiB contract cap is [`SnapshotError::TooLarge`]; a
    /// length that claims more bytes than are present is
    /// [`SnapshotError::Corrupt`].
    fn len_bytes(&mut self, n: u64) -> Result<&'a [u8], SnapshotError> {
        if n > MAX_GUEST_RAM_BYTES as u64 {
            return Err(SnapshotError::TooLarge);
        }
        self.take(n as usize)
    }

    fn cpu(&mut self) -> Result<CpuState, SnapshotError> {
        let mut regs = [0u64; 31];
        for reg in regs.iter_mut() {
            *reg = self.u64()?;
        }
        Ok(CpuState {
            regs,
            sp: self.u64()?,
            pc: self.u64()?,
            pstate: self.u64()?,
        })
    }
}

/// Decode a snapshot blob back into a [`MachineState`].
///
/// Never panics on hostile input: every malformed shape maps to an exact
/// [`SnapshotError`] variant —
/// * fewer than 4 bytes, or any truncation mid-field → `Corrupt`;
/// * declared byte-length past the 2 GiB contract cap → `TooLarge`;
/// * declared length within cap but beyond the bytes present → `Corrupt`;
/// * version != [`SNAPSHOT_VERSION`] → `VersionMismatch { found }`;
/// * unknown device tag, or any trailing bytes → `Corrupt`.
pub fn restore(blob: &[u8]) -> Result<MachineState, SnapshotError> {
    let mut r = Reader::new(blob);

    if r.remaining() < 4 {
        return Err(SnapshotError::Corrupt);
    }
    let found = r.u32()?;
    if found != SNAPSHOT_VERSION {
        return Err(SnapshotError::VersionMismatch { found });
    }

    let cpu_count = r.u32()? as u64;
    // Each vCPU costs exactly CPU_ENCODED_BYTES; reject absurd counts up front.
    // count <= u32::MAX, so the u64 multiply cannot overflow.
    if cpu_count * CPU_ENCODED_BYTES as u64 > r.remaining() as u64 {
        return Err(SnapshotError::Corrupt);
    }
    let mut cpu = Vec::new();
    for _ in 0..cpu_count {
        cpu.push(r.cpu()?);
    }

    let mmu = MmuState {
        ttbr0: r.u64()?,
        ttbr1: r.u64()?,
        tcr: r.u64()?,
        sctlr: r.u64()?,
    };

    let irq = IrqState {
        enabled: r.u32()?,
        pending: r.u64()?,
        timer_count: r.u64()?,
        timer_compare: r.u64()?,
    };

    let ram_len = r.u64()?;
    let ram = r.len_bytes(ram_len)?.to_vec();

    let device_count = r.u32()? as u64;
    if device_count * DEVICE_MIN_BYTES as u64 > r.remaining() as u64 {
        return Err(SnapshotError::Corrupt);
    }
    let mut devices = Vec::new();
    for _ in 0..device_count {
        let tag = r.u8()?;
        let kind = tag_to_kind(tag).ok_or(SnapshotError::Corrupt)?;
        let blob_len = r.u64()?;
        let dev_blob = r.len_bytes(blob_len)?.to_vec();
        devices.push(DeviceState {
            kind,
            blob: dev_blob,
        });
    }

    if r.remaining() != 0 {
        return Err(SnapshotError::Corrupt);
    }

    Ok(MachineState {
        cpu,
        mmu,
        irq,
        ram,
        devices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_state(seed: u64) -> CpuState {
        let mut regs = [0u64; 31];
        for (i, reg) in regs.iter_mut().enumerate() {
            *reg = seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(i as u64);
        }
        CpuState {
            regs,
            sp: seed.wrapping_add(0x1000),
            pc: 0x4000 + seed,
            pstate: seed & 0xF,
        }
    }

    fn machine(seed: u64, cpus: usize, ram_len: usize, devices: Vec<DeviceState>) -> MachineState {
        let ram: Vec<u8> = (0..ram_len)
            .map(|i| (seed.wrapping_add(i as u64) & 0xFF) as u8)
            .collect();
        MachineState {
            cpu: (0..cpus).map(|i| cpu_state(seed + i as u64)).collect(),
            mmu: MmuState {
                ttbr0: 0x1000 + seed,
                ttbr1: seed << 32,
                tcr: seed | 1,
                sctlr: seed & 1,
            },
            irq: IrqState {
                enabled: (seed & 1) as u32,
                pending: seed,
                timer_count: seed.wrapping_mul(3),
                timer_compare: seed.wrapping_mul(7).wrapping_add(100),
            },
            ram,
            devices,
        }
    }

    fn all_device_kinds() -> Vec<DeviceState> {
        vec![
            DeviceState {
                kind: DeviceKind::Block,
                blob: vec![1, 2, 3, 4],
            },
            DeviceState {
                kind: DeviceKind::Net,
                blob: vec![],
            },
            DeviceState {
                kind: DeviceKind::Gpu,
                blob: vec![0xFF; 1024],
            },
            DeviceState {
                kind: DeviceKind::Input,
                blob: b"input-state".to_vec(),
            },
            DeviceState {
                kind: DeviceKind::Console,
                blob: vec![0],
            },
        ]
    }

    #[test]
    fn roundtrip_empty_state() {
        let s = machine(0, 0, 0, vec![]);
        let back = restore(&snapshot(&s).0).expect("empty state must restore");
        assert_eq!(back, s);
        assert!(back.cpu.is_empty());
        assert!(back.ram.is_empty());
        assert!(back.devices.is_empty());
    }

    #[test]
    fn roundtrip_single_cpu_small_ram() {
        let s = machine(42, 1, 4096, vec![]);
        let back = restore(&snapshot(&s).0).expect("must restore");
        assert_eq!(back, s);
        assert_eq!(back.cpu[0].pc, 0x4000 + 42);
        assert_eq!(back.ram.len(), 4096);
    }

    #[test]
    fn roundtrip_multi_cpu_multi_device_large_ram() {
        let s = machine(0xDEAD_BEEF, 4, 1 << 20, all_device_kinds());
        let back = restore(&snapshot(&s).0).expect("must restore");
        assert_eq!(back, s);
        assert_eq!(back.cpu.len(), 4);
        assert_eq!(back.devices.len(), 5);
        assert_eq!(back.devices[2].kind, DeviceKind::Gpu);
        assert_eq!(back.devices[2].blob.len(), 1024);
        // ram pattern survived byte-for-byte
        assert_eq!(back.ram[0], 0xEF);
        let last = 0xDEAD_BEEFu64.wrapping_add(((1 << 20) - 1) as u64) as u8;
        assert_eq!(back.ram[(1 << 20) - 1], last);
    }

    #[test]
    fn roundtrip_extreme_field_values() {
        let s = MachineState {
            cpu: vec![CpuState {
                regs: [u64::MAX; 31],
                sp: u64::MAX,
                pc: u64::MAX,
                pstate: u64::MAX,
            }],
            mmu: MmuState {
                ttbr0: u64::MAX,
                ttbr1: u64::MAX,
                tcr: u64::MAX,
                sctlr: u64::MAX,
            },
            irq: IrqState {
                enabled: u32::MAX,
                pending: u64::MAX,
                timer_count: u64::MAX,
                timer_compare: u64::MAX,
            },
            ram: vec![0xFF; 65536],
            devices: vec![DeviceState {
                kind: DeviceKind::Console,
                blob: vec![0xFF; 777],
            }],
        };
        let back = restore(&snapshot(&s).0).expect("max values must restore");
        assert_eq!(back, s);
    }

    #[test]
    fn roundtrip_snapshot_bytes_are_deterministic() {
        let s = machine(7, 2, 8192, all_device_kinds());
        let a = snapshot(&s).0;
        let b = snapshot(&s).0;
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    // --- corruption matrix: every hostile shape maps to an exact variant ---

    /// Byte offset of the ram_len u64 for a state with `cpus` vCPUs:
    /// version(4) + cpu_count(4) + cpus*272 + mmu(32) + irq(28).
    fn ram_len_offset(cpus: usize) -> usize {
        4 + 4 + cpus * CPU_ENCODED_BYTES + 4 * 8 + (4 + 3 * 8)
    }

    fn patch_u64_le(blob: &mut [u8], offset: usize, v: u64) {
        blob[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn corrupt_empty_blob_is_corrupt() {
        assert_eq!(restore(&[]), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_short_header_is_corrupt() {
        assert_eq!(restore(&[1, 0, 0]), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_wrong_version_is_version_mismatch() {
        let s = machine(1, 1, 16, vec![]);
        let mut b = snapshot(&s).0;
        b[0..4].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            restore(&b),
            Err(SnapshotError::VersionMismatch { found: 2 })
        );
    }

    #[test]
    fn corrupt_flipped_version_byte_is_version_mismatch() {
        let s = machine(1, 1, 16, vec![]);
        let mut b = snapshot(&s).0;
        b[0] ^= 0xFF; // version 1 -> 0xFE, a version we never wrote
        match restore(&b) {
            Err(SnapshotError::VersionMismatch { found }) => {
                assert_ne!(found, SNAPSHOT_VERSION);
            }
            other => panic!("expected VersionMismatch, got {:?}", other),
        }
    }

    #[test]
    fn corrupt_truncated_mid_ram_is_corrupt() {
        let s = machine(3, 2, 4096, all_device_kinds());
        let b = snapshot(&s).0;
        let cut = b.len() / 2;
        assert_eq!(restore(&b[..cut]), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_inflated_ram_len_within_cap_is_corrupt() {
        let s = machine(5, 1, 64, vec![]);
        let mut b = snapshot(&s).0;
        // claim 1 GiB: within the 2 GiB cap, but far beyond bytes present
        patch_u64_le(&mut b, ram_len_offset(1), 1 << 30);
        assert_eq!(restore(&b), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_oversized_ram_len_is_too_large() {
        let s = machine(5, 1, 64, vec![]);
        let mut b = snapshot(&s).0;
        patch_u64_le(&mut b, ram_len_offset(1), MAX_GUEST_RAM_BYTES as u64 + 1);
        assert_eq!(restore(&b), Err(SnapshotError::TooLarge));
    }

    #[test]
    fn corrupt_unknown_device_tag_is_corrupt() {
        let s = machine(
            9,
            1,
            0,
            vec![DeviceState {
                kind: DeviceKind::Gpu,
                blob: vec![1, 2],
            }],
        );
        let mut b = snapshot(&s).0;
        // tag sits right after ram_len(8) + ram(0) + device_count(4)
        let tag_off = ram_len_offset(1) + 8 + 4;
        b[tag_off] = 7;
        assert_eq!(restore(&b), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_inflated_device_blob_len_is_corrupt() {
        let s = machine(
            9,
            1,
            0,
            vec![DeviceState {
                kind: DeviceKind::Net,
                blob: vec![9],
            }],
        );
        let mut b = snapshot(&s).0;
        let len_off = ram_len_offset(1) + 8 + 4 + 1; // tag(1) then blob_len(8)
        patch_u64_le(&mut b, len_off, 1 << 20); // claims 1 MiB, has 1 byte
        assert_eq!(restore(&b), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_trailing_garbage_is_corrupt() {
        let s = machine(11, 2, 128, all_device_kinds());
        let mut b = snapshot(&s).0;
        b.push(0xAA);
        assert_eq!(restore(&b), Err(SnapshotError::Corrupt));
    }

    #[test]
    fn corrupt_absurd_cpu_count_is_corrupt() {
        let s = machine(13, 0, 0, vec![]);
        let mut b = snapshot(&s).0;
        b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(restore(&b), Err(SnapshotError::Corrupt));
    }
}
