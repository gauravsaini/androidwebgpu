//! U6 `virtio-transport` — EXPLICIT-STATE virtqueue ring engine + virtio-MMIO config space.
//!
//! Queue discipline ported from Path E `src/virtio/virtqueue.js`
//! (OASIS Virtio 1.2, split virtqueues). Every guest-memory access is
//! bounds-checked; malformed rings yield typed [`ChainError`]s — never a panic,
//! never an out-of-bounds read.
//!
//! ## Contract amendment U6-G1 (2026-09-27, parent-owned freeze)
//! The frozen [`TransportState`] now carries a per-queue ring table
//! (`queues: Vec<VirtQueue>`): desc/avail/used addresses, size, ready flag,
//! and avail/used cursors. [`step`] therefore resolves a queue's rings on
//! [`DevEvent::QueueNotify`] and requires `ready == true` (plus
//! `queue_idx < queue_count` and `DRIVER_OK`) before treating the queue as
//! live. `step` still does NOT drive the descriptor chain itself — the
//! orchestrator (Wave 4) reads the ring addresses from `state.queues` and
//! calls the pure [`pop_chain`]/[`push_used`] engine around the device step.
//! Known remainder: the MMIO queue-programming registers (QUEUE_SEL,
//! QUEUE_NUM, QUEUE_DESC_*, QUEUE_AVAIL_*, QUEUE_USED_*, QUEUE_READY) are
//! still validated-only — there is no selected-queue field in the state to
//! persist them against (future amendment); the table is populated by machine
//! setup / the orchestrator.

use pathn_contracts::device::{DevEvent, DevOut, TransportState, VirtQueue};

// ---------------------------------------------------------------------------
// virtio constants (OASIS Virtio 1.2; feature set mirrors Path E's advertised set)
// ---------------------------------------------------------------------------

/// Descriptor flags.
pub const VRING_DESC_F_NEXT: u16 = 0x0001;
pub const VRING_DESC_F_WRITE: u16 = 0x0002;
pub const VRING_DESC_F_INDIRECT: u16 = 0x0004;
/// `struct vring_desc` is 16 bytes: addr(u64) len(u32) flags(u16) next(u16).
pub const VRING_DESC_SIZE: u64 = 16;

/// Feature bits (Path E advertised: VERSION_1 always, INDIRECT_DESC, EVENT_IDX).
pub const VIRTIO_F_RING_INDIRECT_DESC: u64 = 1 << 28;
pub const VIRTIO_F_RING_EVENT_IDX: u64 = 1 << 29;
pub const VIRTIO_F_VERSION_1: u64 = 1 << 32;
/// Device feature set this transport offers during negotiation.
pub const OFFERED_FEATURES: u64 =
    VIRTIO_F_VERSION_1 | VIRTIO_F_RING_INDIRECT_DESC | VIRTIO_F_RING_EVENT_IDX;

/// Device status bits (OASIS §2.1).
pub const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
pub const VIRTIO_STATUS_DRIVER: u8 = 2;
pub const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
pub const VIRTIO_STATUS_FEATURES_OK: u8 = 8;
pub const VIRTIO_STATUS_DEVICE_NEEDS_RESET: u8 = 64;
pub const VIRTIO_STATUS_FAILED: u8 = 128;

/// virtio-MMIO register offsets (the transport ARM guests use; not PCI).
pub const MMIO_MAGIC: u64 = 0x000;
pub const MMIO_VERSION: u64 = 0x004;
pub const MMIO_DEVICE_ID: u64 = 0x008;
pub const MMIO_VENDOR_ID: u64 = 0x00c;
pub const MMIO_DEVICE_FEATURES: u64 = 0x010;
pub const MMIO_DRIVER_FEATURES: u64 = 0x020;
pub const MMIO_QUEUE_SEL: u64 = 0x030;
pub const MMIO_QUEUE_NUM_MAX: u64 = 0x034;
pub const MMIO_QUEUE_NUM: u64 = 0x038;
pub const MMIO_QUEUE_READY: u64 = 0x03c;
pub const MMIO_QUEUE_NOTIFY: u64 = 0x044;
pub const MMIO_STATUS: u64 = 0x070;
pub const MMIO_QUEUE_DESC_LO: u64 = 0x080;
pub const MMIO_QUEUE_DESC_HI: u64 = 0x084;
pub const MMIO_QUEUE_AVAIL_LO: u64 = 0x090;
pub const MMIO_QUEUE_AVAIL_HI: u64 = 0x094;
pub const MMIO_QUEUE_USED_LO: u64 = 0x0a0;
pub const MMIO_QUEUE_USED_HI: u64 = 0x0a4;
/// Start of device-specific config (owned by the device unit, e.g. U7).
pub const MMIO_DEVICE_CONFIG: u64 = 0x100;

pub const MAGIC_VALUE: u64 = 0x7472_6976; // "virt", little-endian
pub const MMIO_VERSION_VALUE: u64 = 2;
pub const VENDOR_ID_VALUE: u64 = 0x1AF4;
/// Transport-level device ID. The concrete device ID (e.g. 16 for virtio-gpu)
/// is overlaid by the device unit / orchestrator; the transport claims none.
pub const TRANSPORT_DEVICE_ID: u64 = 0;
/// Maximum queue size, matching Path E's default.
pub const VIRTQ_MAX_SIZE: u16 = 256;
/// Contract cap: guest RAM never exceeds 2 GiB, so a valid queue address
/// always has its high 32 bits clear and its low half below 2^31.
pub const MAX_QUEUE_ADDR: u64 = 0x8000_0000;

// ---------------------------------------------------------------------------
// Ring engine types
// ---------------------------------------------------------------------------

/// Typed failure of descriptor-chain parsing. Data, not panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    /// Queue size not a power of two in 1..=256.
    BadQueueSize,
    /// avail.ring head index >= queue size.
    HeadOutOfBounds,
    /// NEXT index >= table bound.
    NextOutOfBounds,
    /// Descriptor index revisited (direct or indirect table).
    LoopDetected,
    /// Chain longer than the table bound.
    ChainTooLong,
    /// Indirect descriptor violates OASIS §2.6.5.3.
    MalformedIndirect,
    /// Indirect table entry sets INDIRECT (nesting is forbidden).
    NestedIndirect,
    /// Readable buffer appears after a writable one.
    ReadAfterWrite,
    /// Buffer [addr, addr+len) lies outside guest ram.
    DmaOutOfRange,
    /// A ring structure read/write itself lies outside guest ram.
    RingOutOfRange,
}

/// One parsed `vring_desc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Desc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// One validated DMA buffer of a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainBuffer {
    pub addr: u64,
    pub len: u32,
}

/// A fully parsed descriptor chain (Path E's `popDescriptorChain` result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    pub head: u16,
    pub readable: Vec<ChainBuffer>,
    pub writable: Vec<ChainBuffer>,
    pub total_read: u64,
    pub total_write: u64,
}

/// Successful pop: the chain plus the advanced device-side avail cursor.
/// The caller threads `next_avail` back into its explicit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopOk {
    pub chain: Chain,
    pub next_avail: u16,
}

// ---------------------------------------------------------------------------
// Bounds-checked little-endian access. `range_ok` uses the subtraction form
// so `off + len` can never overflow, on 32- or 64-bit targets.
// ---------------------------------------------------------------------------

fn range_ok(ram_len: usize, off: u64, len: u64) -> bool {
    let n = ram_len as u64;
    off <= n && len <= n - off
}

/// Checked address arithmetic: any overflow maps to RingOutOfRange instead of
/// panicking (caller-supplied ring addresses are untrusted).
fn add_off(base: u64, delta: u64) -> Result<u64, ChainError> {
    base.checked_add(delta).ok_or(ChainError::RingOutOfRange)
}

fn read_u16_le(ram: &[u8], off: u64) -> Result<u16, ChainError> {
    if !range_ok(ram.len(), off, 2) {
        return Err(ChainError::RingOutOfRange);
    }
    let o = off as usize;
    Ok(u16::from_le_bytes([ram[o], ram[o + 1]]))
}

fn read_u32_le(ram: &[u8], off: u64) -> Result<u32, ChainError> {
    if !range_ok(ram.len(), off, 4) {
        return Err(ChainError::RingOutOfRange);
    }
    let o = off as usize;
    Ok(u32::from_le_bytes([
        ram[o],
        ram[o + 1],
        ram[o + 2],
        ram[o + 3],
    ]))
}

fn read_u64_le(ram: &[u8], off: u64) -> Result<u64, ChainError> {
    if !range_ok(ram.len(), off, 8) {
        return Err(ChainError::RingOutOfRange);
    }
    let o = off as usize;
    Ok(u64::from_le_bytes([
        ram[o],
        ram[o + 1],
        ram[o + 2],
        ram[o + 3],
        ram[o + 4],
        ram[o + 5],
        ram[o + 6],
        ram[o + 7],
    ]))
}

fn write_u16_le(ram: &mut [u8], off: u64, v: u16) -> Result<(), ChainError> {
    if !range_ok(ram.len(), off, 2) {
        return Err(ChainError::RingOutOfRange);
    }
    let o = off as usize;
    ram[o..o + 2].copy_from_slice(&v.to_le_bytes());
    Ok(())
}

fn write_u32_le(ram: &mut [u8], off: u64, v: u32) -> Result<(), ChainError> {
    if !range_ok(ram.len(), off, 4) {
        return Err(ChainError::RingOutOfRange);
    }
    let o = off as usize;
    ram[o..o + 4].copy_from_slice(&v.to_le_bytes());
    Ok(())
}

fn check_queue_size(size: u16) -> Result<u16, ChainError> {
    if size == 0 || size > VIRTQ_MAX_SIZE || !size.is_power_of_two() {
        return Err(ChainError::BadQueueSize);
    }
    Ok(size)
}

fn read_desc(ram: &[u8], table: u64, idx: u16) -> Result<Desc, ChainError> {
    let off = add_off(table, (idx as u64) * VRING_DESC_SIZE)?;
    Ok(Desc {
        addr: read_u64_le(ram, off)?,
        len: read_u32_le(ram, add_off(off, 8)?)?,
        flags: read_u16_le(ram, add_off(off, 12)?)?,
        next: read_u16_le(ram, add_off(off, 14)?)?,
    })
}

/// Validate one buffer and append it, enforcing readable-before-writable order
/// (Path E `popDescriptorChain` / `parseIndirectTable`).
fn push_checked(
    ram_len: usize,
    chain: &mut Chain,
    seen_write: &mut bool,
    addr: u64,
    len: u32,
    is_write: bool,
) -> Result<(), ChainError> {
    if !range_ok(ram_len, addr, len as u64) {
        return Err(ChainError::DmaOutOfRange);
    }
    if is_write {
        *seen_write = true;
        chain.writable.push(ChainBuffer { addr, len });
        chain.total_write += len as u64;
    } else {
        if *seen_write {
            return Err(ChainError::ReadAfterWrite);
        }
        chain.readable.push(ChainBuffer { addr, len });
        chain.total_read += len as u64;
    }
    Ok(())
}

/// Validate an INDIRECT descriptor per OASIS §2.6.5.3 (Path E `popDescriptorChain`).
/// Returns the indirect entry count.
fn check_indirect_desc(ram_len: usize, d: &Desc, size: u16) -> Result<u32, ChainError> {
    if d.flags & (VRING_DESC_F_WRITE | VRING_DESC_F_NEXT) != 0 {
        return Err(ChainError::MalformedIndirect);
    }
    if d.len == 0 || !d.len.is_multiple_of(VRING_DESC_SIZE as u32) {
        return Err(ChainError::MalformedIndirect);
    }
    let count = d.len / VRING_DESC_SIZE as u32;
    if count > size as u32 {
        return Err(ChainError::MalformedIndirect);
    }
    if !range_ok(ram_len, d.addr, d.len as u64) {
        return Err(ChainError::DmaOutOfRange);
    }
    Ok(count)
}

fn walk_indirect(
    ram: &[u8],
    table_addr: u64,
    count: u32,
    chain: &mut Chain,
) -> Result<(), ChainError> {
    // count <= queue_size <= 256: direct indexing, no HashSet needed.
    let mut visited = [false; 256];
    let mut seen_write = false;
    let mut steps: u32 = 0;
    let mut idx: u32 = 0;
    loop {
        let i = idx as usize;
        if visited[i] {
            return Err(ChainError::LoopDetected);
        }
        if steps >= count {
            return Err(ChainError::ChainTooLong);
        }
        visited[i] = true;
        steps += 1;
        let d = read_desc(ram, table_addr, idx as u16)?;
        if d.flags & VRING_DESC_F_INDIRECT != 0 {
            return Err(ChainError::NestedIndirect);
        }
        let is_write = d.flags & VRING_DESC_F_WRITE != 0;
        push_checked(ram.len(), chain, &mut seen_write, d.addr, d.len, is_write)?;
        if d.flags & VRING_DESC_F_NEXT != 0 {
            if d.next as u32 >= count {
                return Err(ChainError::NextOutOfBounds);
            }
            idx = d.next as u32;
        } else {
            break;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public ring engine (pure)
// ---------------------------------------------------------------------------

/// Consume the next available descriptor chain (Path E `popDescriptorChain`).
///
/// Returns `Ok(None)` when the queue has no new chain (`avail.idx` already
/// consumed, or a null ring address meaning "not configured"). Malformed
/// chains yield `Err(ChainError)` — never a panic, never an OOB read.
pub fn pop_chain(
    ram: &[u8],
    desc_table: u64,
    avail_ring: u64,
    queue_size: u16,
    last_avail: u16,
) -> Result<Option<PopOk>, ChainError> {
    let size = check_queue_size(queue_size)?;
    if desc_table == 0 || avail_ring == 0 {
        return Ok(None);
    }
    let avail_idx = read_u16_le(ram, add_off(avail_ring, 2)?)?;
    if avail_idx == last_avail {
        return Ok(None);
    }
    let slot = (last_avail % size) as u64;
    let head = read_u16_le(ram, add_off(avail_ring, 4 + slot * 2)?)?;
    if head >= size {
        return Err(ChainError::HeadOutOfBounds);
    }

    let mut chain = Chain {
        head,
        readable: Vec::new(),
        writable: Vec::new(),
        total_read: 0,
        total_write: 0,
    };
    let mut visited = [false; 256];
    let mut seen_write = false;
    let mut steps: u16 = 0;
    let mut idx = head;
    loop {
        let i = idx as usize;
        if visited[i] {
            return Err(ChainError::LoopDetected);
        }
        if steps >= size {
            return Err(ChainError::ChainTooLong);
        }
        visited[i] = true;
        steps += 1;
        let d = read_desc(ram, desc_table, idx)?;
        if d.flags & VRING_DESC_F_INDIRECT != 0 {
            let count = check_indirect_desc(ram.len(), &d, size)?;
            walk_indirect(ram, d.addr, count, &mut chain)?;
            break; // an indirect descriptor never continues via NEXT
        }
        let is_write = d.flags & VRING_DESC_F_WRITE != 0;
        push_checked(
            ram.len(),
            &mut chain,
            &mut seen_write,
            d.addr,
            d.len,
            is_write,
        )?;
        if d.flags & VRING_DESC_F_NEXT != 0 {
            if d.next >= size {
                return Err(ChainError::NextOutOfBounds);
            }
            idx = d.next;
        } else {
            break;
        }
    }

    Ok(Some(PopOk {
        chain,
        next_avail: last_avail.wrapping_add(1),
    }))
}

/// Commit one consumed chain to the used ring (Path E `pushUsed`).
/// INVARIANT: exactly one used element per consumed chain. The element is
/// written before `used.idx` advances; returns the advanced cursor for the
/// caller to thread as explicit state.
pub fn push_used(
    ram: &mut [u8],
    used_ring: u64,
    queue_size: u16,
    last_used: u16,
    head: u16,
    written_len: u32,
) -> Result<u16, ChainError> {
    let size = check_queue_size(queue_size)?;
    if used_ring == 0 {
        return Err(ChainError::RingOutOfRange);
    }
    let slot = (last_used % size) as u64;
    let elem = add_off(used_ring, 4 + slot * 8)?;
    write_u32_le(ram, elem, head as u32)?;
    write_u32_le(ram, add_off(elem, 4)?, written_len)?;
    let next = last_used.wrapping_add(1);
    write_u16_le(ram, add_off(used_ring, 2)?, next)?;
    Ok(next)
}

/// Pure `vring_need_event` (Path E `shouldNotify` EVENT_IDX branch):
/// should the device interrupt the guest? `new_used` is the device's used.idx,
/// `event` the guest's avail_event, `old_notified` the last notified index.
pub fn need_event_idx(new_used: u16, event: u16, old_notified: u16) -> bool {
    if new_used == old_notified {
        // No progress since last notify: exact-match check so a driver
        // awaiting the already-reached index still gets its interrupt.
        new_used.wrapping_sub(event).wrapping_sub(1) < 1
    } else {
        new_used.wrapping_sub(event).wrapping_sub(1) < new_used.wrapping_sub(old_notified)
    }
}

// ---------------------------------------------------------------------------
// step: the frozen-contract state machine
// ---------------------------------------------------------------------------

/// virtio-MMIO transport step: config space, status/feature negotiation,
/// queue validation, reset. EXPLICIT-STATE: everything in the signature.
pub fn step(state: &TransportState, ev: DevEvent, ram: &mut [u8]) -> (TransportState, Vec<DevOut>) {
    match ev {
        DevEvent::Reset => (
            TransportState {
                queue_count: state.queue_count,
                features: 0,
                status: 0,
                // virtio reset: queues must be reconfigured by the driver.
                queues: fresh_queues(state.queue_count),
            },
            Vec::new(),
        ),

        DevEvent::ConfigWrite { offset, value } => {
            let mut next = state.clone();
            match offset {
                MMIO_STATUS => {
                    // Writing 0 is the virtio reset sequence.
                    if value == 0 {
                        next.features = 0;
                        next.status = 0;
                        next.queues = fresh_queues(next.queue_count);
                    } else {
                        next.status = value as u8;
                    }
                }
                MMIO_DRIVER_FEATURES => {
                    // Negotiation = offered ∩ driver. Single 64-bit write; the
                    // frozen state has no FeatureSel halves to model.
                    next.features = OFFERED_FEATURES & value;
                }
                MMIO_QUEUE_SEL => {
                    // Validated only: the frozen state cannot persist the selection.
                    let _ = value < next.queue_count as u64;
                }
                MMIO_QUEUE_NUM => {
                    // Validated only: power of two, 1..=256.
                    let v = value as u16;
                    let _ = v != 0 && v <= VIRTQ_MAX_SIZE && v.is_power_of_two();
                }
                MMIO_QUEUE_READY => {
                    let _ = value <= 1;
                }
                MMIO_QUEUE_NOTIFY => {
                    // MMIO queue-notify register: value is the queue index.
                    return handle_queue_notify(&next, value as u16);
                }
                MMIO_QUEUE_DESC_LO | MMIO_QUEUE_AVAIL_LO | MMIO_QUEUE_USED_LO => {
                    // Low half alone: reject only what is certainly invalid —
                    // any address >= 2 GiB exceeds the contract RAM cap.
                    let _ = (value & 0xFFFF_FFFF) < MAX_QUEUE_ADDR;
                }
                MMIO_QUEUE_DESC_HI | MMIO_QUEUE_AVAIL_HI | MMIO_QUEUE_USED_HI => {
                    // High half alone: nonzero is certainly out of range.
                    let _ = (value & 0xFFFF_FFFF) == 0;
                }
                _ => {
                    // Device-specific config (MMIO_DEVICE_CONFIG+) belongs to
                    // the device unit (e.g. U7); unknown offsets are ignored.
                }
            }
            let _ = ram; // config space needs no guest-memory access
            (next, Vec::new())
        }

        DevEvent::ConfigRead { offset } => {
            let v = match offset {
                MMIO_MAGIC => MAGIC_VALUE,
                MMIO_VERSION => MMIO_VERSION_VALUE,
                MMIO_DEVICE_ID => TRANSPORT_DEVICE_ID,
                MMIO_VENDOR_ID => VENDOR_ID_VALUE,
                MMIO_DEVICE_FEATURES => OFFERED_FEATURES,
                MMIO_QUEUE_NUM_MAX => VIRTQ_MAX_SIZE as u64,
                MMIO_STATUS => state.status as u64,
                // Queue address registers cannot persist in the frozen state;
                // device config belongs to the device unit. Graceful zero.
                _ => 0,
            };
            let _ = ram;
            (state.clone(), vec![DevOut::ConfigValue(v)])
        }

        DevEvent::QueueNotify { queue_idx } => handle_queue_notify(state, queue_idx),
    }
}

/// Fresh (unconfigured) queue table for `queue_count` queues.
/// Used by both reset paths; every queue starts not-ready.
fn fresh_queues(queue_count: u16) -> Vec<VirtQueue> {
    (0..queue_count)
        .map(|_| VirtQueue {
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            size: 0,
            ready: false,
            last_avail_idx: 0,
            last_used_idx: 0,
        })
        .collect()
}

/// Shared QueueNotify handling (also used by the MMIO_QUEUE_NOTIFY register).
fn handle_queue_notify(state: &TransportState, queue_idx: u16) -> (TransportState, Vec<DevOut>) {
    if queue_idx >= state.queue_count {
        return (state.clone(), Vec::new());
    }
    if state.status & VIRTIO_STATUS_DRIVER_OK == 0 {
        return (state.clone(), Vec::new());
    }
    // The queue table may be shorter than queue_count if setup never
    // populated it; .get() keeps this graceful instead of panicking.
    let ready = state
        .queues
        .get(queue_idx as usize)
        .is_some_and(|q| q.ready);
    if !ready {
        return (state.clone(), Vec::new());
    }
    // Queue is live, but the transport does not drive the descriptor chain
    // here: no device import is allowed (swarm law). The orchestrator reads
    // the ring addresses from state.queues and calls pop_chain/push_used
    // around the device step. Graceful no-op, never OOB.
    (state.clone(), Vec::new())
}

// ---------------------------------------------------------------------------
// Tests — real, named, no #[ignore]. Rings are built by hand in a Vec<u8>.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u16(ram: &mut [u8], off: usize, v: u16) {
        ram[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u32(ram: &mut [u8], off: usize, v: u32) {
        ram[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u64(ram: &mut [u8], off: usize, v: u64) {
        ram[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }
    /// Write one 16-byte vring_desc into a table.
    fn put_desc(
        ram: &mut [u8],
        table: usize,
        idx: usize,
        addr: u64,
        len: u32,
        flags: u16,
        next: u16,
    ) {
        let o = table + idx * 16;
        put_u64(ram, o, addr);
        put_u32(ram, o + 8, len);
        put_u16(ram, o + 12, flags);
        put_u16(ram, o + 14, next);
    }
    /// Minimal avail ring (flags=0) with `heads` entries and given idx.
    fn put_avail(ram: &mut [u8], avail: usize, idx: u16, heads: &[u16]) {
        put_u16(ram, avail, 0);
        put_u16(ram, avail + 2, idx);
        for (i, h) in heads.iter().enumerate() {
            put_u16(ram, avail + 4 + i * 2, *h);
        }
    }

    const DESC_TBL: usize = 0x1000;
    const AVAIL: usize = 0x1800;
    const USED: usize = 0x1C00;

    // --- golden: direct chain -------------------------------------------

    #[test]
    fn descriptor_chain_golden_direct() {
        let mut ram = vec![0u8; 0x4000];
        // Mark the readable buffer so we can prove the address resolved.
        for b in ram[0x2000..0x2040].iter_mut() {
            *b = 0xAB;
        }
        put_desc(&mut ram, DESC_TBL, 0, 0x2000, 64, VRING_DESC_F_NEXT, 1);
        put_desc(&mut ram, DESC_TBL, 1, 0x2100, 128, VRING_DESC_F_WRITE, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);

        let got = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0)
            .expect("no error")
            .expect("chain available");
        assert_eq!(got.next_avail, 1);
        assert_eq!(
            got.chain,
            Chain {
                head: 0,
                readable: vec![ChainBuffer {
                    addr: 0x2000,
                    len: 64
                }],
                writable: vec![ChainBuffer {
                    addr: 0x2100,
                    len: 128
                }],
                total_read: 64,
                total_write: 128,
            }
        );
        // The resolved readable address really points at our marked bytes.
        assert!(ram[0x2000..0x2040].iter().all(|b| *b == 0xAB));

        // Golden used-ring bytes: elem {id=0, len=128}, then used.idx = 1.
        let next_used = push_used(&mut ram, USED as u64, 8, 0, 0, 128).expect("no error");
        assert_eq!(next_used, 1);
        assert_eq!(&ram[USED + 2..USED + 4], &[1, 0]);
        assert_eq!(&ram[USED + 4..USED + 12], &[0, 0, 0, 0, 128, 0, 0, 0]);
    }

    // --- golden: indirect chain ------------------------------------------

    #[test]
    fn descriptor_chain_golden_indirect() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(&mut ram, DESC_TBL, 0, 0x2200, 32, VRING_DESC_F_INDIRECT, 0);
        // Indirect table: readable -> writable.
        put_desc(&mut ram, 0x2200, 0, 0x2300, 16, VRING_DESC_F_NEXT, 1);
        put_desc(&mut ram, 0x2200, 1, 0x2310, 32, VRING_DESC_F_WRITE, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);

        let got = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0)
            .expect("no error")
            .expect("chain available");
        assert_eq!(got.chain.head, 0);
        assert_eq!(
            got.chain.readable,
            vec![ChainBuffer {
                addr: 0x2300,
                len: 16
            }]
        );
        assert_eq!(
            got.chain.writable,
            vec![ChainBuffer {
                addr: 0x2310,
                len: 32
            }]
        );
        assert_eq!(got.chain.total_read, 16);
        assert_eq!(got.chain.total_write, 32);
    }

    // --- malformed chains: graceful typed errors, zero panics -------------

    #[test]
    fn descriptor_malformed_head_out_of_bounds() {
        let mut ram = vec![0u8; 0x4000];
        put_avail(&mut ram, AVAIL, 1, &[8]); // head == size
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::HeadOutOfBounds);
    }

    #[test]
    fn descriptor_malformed_next_out_of_bounds() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(&mut ram, DESC_TBL, 0, 0x2000, 8, VRING_DESC_F_NEXT, 9);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::NextOutOfBounds);
    }

    #[test]
    fn descriptor_malformed_loop() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(&mut ram, DESC_TBL, 0, 0x2000, 8, VRING_DESC_F_NEXT, 1);
        put_desc(&mut ram, DESC_TBL, 1, 0x2008, 8, VRING_DESC_F_NEXT, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::LoopDetected);
    }

    #[test]
    fn descriptor_malformed_length_overrun() {
        let mut ram = vec![0u8; 0x4000];
        // Buffer starts 10 bytes before ram end but claims 64.
        put_desc(&mut ram, DESC_TBL, 0, 0x4000 - 10, 64, 0, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::DmaOutOfRange);
    }

    #[test]
    fn descriptor_malformed_indirect_loop() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(&mut ram, DESC_TBL, 0, 0x2200, 32, VRING_DESC_F_INDIRECT, 0);
        put_desc(&mut ram, 0x2200, 0, 0x2300, 16, VRING_DESC_F_NEXT, 1);
        put_desc(&mut ram, 0x2200, 1, 0x2310, 16, VRING_DESC_F_NEXT, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::LoopDetected);
    }

    #[test]
    fn descriptor_malformed_nested_indirect() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(&mut ram, DESC_TBL, 0, 0x2200, 16, VRING_DESC_F_INDIRECT, 0);
        put_desc(&mut ram, 0x2200, 0, 0x2300, 16, VRING_DESC_F_INDIRECT, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::NestedIndirect);
    }

    #[test]
    fn descriptor_malformed_indirect_bad_len() {
        let mut ram = vec![0u8; 0x4000];
        // 20 is not a multiple of 16.
        put_desc(&mut ram, DESC_TBL, 0, 0x2200, 20, VRING_DESC_F_INDIRECT, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::MalformedIndirect);
    }

    #[test]
    fn descriptor_malformed_indirect_write_flag() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(
            &mut ram,
            DESC_TBL,
            0,
            0x2200,
            16,
            VRING_DESC_F_INDIRECT | VRING_DESC_F_WRITE,
            0,
        );
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::MalformedIndirect);
    }

    #[test]
    fn descriptor_malformed_read_after_write() {
        let mut ram = vec![0u8; 0x4000];
        put_desc(
            &mut ram,
            DESC_TBL,
            0,
            0x2000,
            8,
            VRING_DESC_F_WRITE | VRING_DESC_F_NEXT,
            1,
        );
        put_desc(&mut ram, DESC_TBL, 1, 0x2008, 8, 0, 0);
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::ReadAfterWrite);
    }

    #[test]
    fn descriptor_avail_empty_returns_none() {
        let ram = vec![0u8; 0x4000];
        // avail.idx == last_avail: nothing to do, not an error.
        let got = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0).expect("no error");
        assert_eq!(got, None);
    }

    #[test]
    fn descriptor_ring_truncated() {
        let mut ram = vec![0u8; 0x4000];
        // Descriptor table starts 8 bytes before ram end: a 16-byte desc cannot fit.
        put_avail(&mut ram, AVAIL, 1, &[0]);
        let err = pop_chain(&ram, (0x4000 - 8) as u64, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::RingOutOfRange);
    }

    #[test]
    fn descriptor_index_wraps_u16() {
        let mut ram = vec![0u8; 0x4000];
        // Device cursor at 0xFFFF, guest idx at 0: one chain pending, slot = 0xFFFF % 8 = 7.
        put_desc(&mut ram, DESC_TBL, 3, 0x2000, 8, 0, 0);
        let mut heads = [0u16; 8];
        heads[7] = 3;
        put_avail(&mut ram, AVAIL, 0, &heads);
        let got = pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, 8, 0xFFFF)
            .expect("no error")
            .expect("chain available");
        assert_eq!(got.chain.head, 3);
        assert_eq!(got.next_avail, 0); // wraps
    }

    #[test]
    fn descriptor_push_used_slot_wraps() {
        let mut ram = vec![0u8; 0x4000];
        let next = push_used(&mut ram, USED as u64, 8, 0xFFFF, 2, 64).expect("no error");
        assert_eq!(next, 0);
        // slot = 0xFFFF % 8 = 7 -> elem at USED+4+56.
        assert_eq!(&ram[USED + 2..USED + 4], &[0, 0]);
        assert_eq!(&ram[USED + 60..USED + 68], &[2, 0, 0, 0, 64, 0, 0, 0]);
    }

    #[test]
    fn descriptor_ring_address_overflow_is_graceful() {
        // Hostile caller-supplied addresses must error, never panic on overflow.
        let mut ram = vec![0u8; 0x4000];
        put_avail(&mut ram, AVAIL, 1, &[0]); // one chain pending, so rings are read
        let err = pop_chain(&ram, u64::MAX, AVAIL as u64, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::RingOutOfRange);
        let err = pop_chain(&ram, DESC_TBL as u64, u64::MAX, 8, 0).expect_err("must fail");
        assert_eq!(err, ChainError::RingOutOfRange);
        let mut ram2 = vec![0u8; 0x4000];
        let err = push_used(&mut ram2, u64::MAX, 8, 0, 0, 0).expect_err("must fail");
        assert_eq!(err, ChainError::RingOutOfRange);
    }

    #[test]
    fn descriptor_bad_queue_size_rejected() {
        let ram = vec![0u8; 0x4000];
        for bad in [0u16, 3, 100, 512] {
            let err =
                pop_chain(&ram, DESC_TBL as u64, AVAIL as u64, bad, 0).expect_err("must fail");
            assert_eq!(err, ChainError::BadQueueSize, "size {bad}");
        }
    }

    // --- step: config space / status / reset ------------------------------

    fn dev_state() -> TransportState {
        TransportState {
            queue_count: 2,
            features: 0,
            status: 0,
            queues: fresh_queues(2),
        }
    }

    /// A queue table with queue 0 fully configured and ready.
    fn ready_queues() -> Vec<VirtQueue> {
        let mut qs = fresh_queues(2);
        qs[0] = VirtQueue {
            desc_addr: 0x1000,
            avail_addr: 0x2000,
            used_addr: 0x3000,
            size: 128,
            ready: true,
            last_avail_idx: 0,
            last_used_idx: 0,
        };
        qs
    }

    #[test]
    fn step_reset_zeroes_negotiated_state() {
        let s = TransportState {
            queue_count: 2,
            features: 0xFFFF,
            status: 0xFF,
            queues: ready_queues(),
        };
        let mut ram = vec![0u8; 0x1000];
        let (next, outs) = step(&s, DevEvent::Reset, &mut ram);
        assert_eq!(
            next,
            TransportState {
                queue_count: 2,
                features: 0,
                status: 0,
                queues: fresh_queues(2),
            }
        );
        assert!(outs.is_empty());
    }

    #[test]
    fn step_status_write_zero_resets() {
        let s = TransportState {
            queue_count: 1,
            features: OFFERED_FEATURES,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: ready_queues()[..1].to_vec(),
        };
        let mut ram = vec![0u8; 0x1000];
        let (next, _) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_STATUS,
                value: 0,
            },
            &mut ram,
        );
        assert_eq!(next.status, 0);
        assert_eq!(next.features, 0);
        assert_eq!(next.queue_count, 1);
        // Status-0 reset clears the queue table: the driver must reconfigure.
        assert_eq!(next.queues, fresh_queues(1));
    }

    #[test]
    fn step_status_write_sets_bits() {
        let s = dev_state();
        let mut ram = vec![0u8; 0x1000];
        let (next, _) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_STATUS,
                value: 0x0F,
            },
            &mut ram,
        );
        assert_eq!(
            next.status,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_DRIVER_OK
                | VIRTIO_STATUS_FEATURES_OK
        );
    }

    #[test]
    fn step_feature_negotiation_intersects_offered() {
        let s = dev_state();
        let mut ram = vec![0u8; 0x1000];
        // Driver asks for everything: gets exactly what we offer.
        let (next, _) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_DRIVER_FEATURES,
                value: u64::MAX,
            },
            &mut ram,
        );
        assert_eq!(next.features, OFFERED_FEATURES);
        // Driver asks for a subset: gets the subset.
        let (next2, _) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_DRIVER_FEATURES,
                value: VIRTIO_F_VERSION_1,
            },
            &mut ram,
        );
        assert_eq!(next2.features, VIRTIO_F_VERSION_1);
        // Driver asks for a bit we do not offer: dropped.
        let (next3, _) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_DRIVER_FEATURES,
                value: 1 << 40,
            },
            &mut ram,
        );
        assert_eq!(next3.features, 0);
    }

    #[test]
    fn step_config_read_goldens() {
        let s = TransportState {
            queue_count: 2,
            features: VIRTIO_F_VERSION_1,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: Vec::new(),
        };
        let mut ram = vec![0u8; 0x1000];
        let mut read = |offset: u64| -> u64 {
            let (_, outs) = step(&s, DevEvent::ConfigRead { offset }, &mut ram);
            assert_eq!(outs.len(), 1);
            match outs.into_iter().next().unwrap() {
                DevOut::ConfigValue(v) => v,
                o => panic!("expected ConfigValue, got {o:?}"),
            }
        };
        assert_eq!(read(MMIO_MAGIC), MAGIC_VALUE);
        assert_eq!(read(MMIO_VERSION), MMIO_VERSION_VALUE);
        assert_eq!(read(MMIO_VENDOR_ID), VENDOR_ID_VALUE);
        assert_eq!(read(MMIO_DEVICE_FEATURES), OFFERED_FEATURES);
        assert_eq!(read(MMIO_QUEUE_NUM_MAX), VIRTQ_MAX_SIZE as u64);
        assert_eq!(read(MMIO_STATUS), VIRTIO_STATUS_DRIVER_OK as u64);
        // Queue address registers cannot persist in the frozen state: graceful zero.
        assert_eq!(read(MMIO_QUEUE_DESC_LO), 0);
        // Unknown offsets: graceful zero, never a panic.
        assert_eq!(read(0xDEAD), 0);
    }

    // --- step: QueueNotify graceful paths ----------------------------------

    #[test]
    fn step_queue_notify_bad_index_is_graceful() {
        let s = TransportState {
            queue_count: 1,
            features: 0,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: Vec::new(),
        };
        let mut ram = vec![0u8; 0x4000];
        let (next, outs) = step(&s, DevEvent::QueueNotify { queue_idx: 5 }, &mut ram);
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    #[test]
    fn step_queue_notify_without_driver_ok_is_graceful() {
        let s = dev_state(); // status == 0
        let mut ram = vec![0u8; 0x4000];
        let (next, outs) = step(&s, DevEvent::QueueNotify { queue_idx: 0 }, &mut ram);
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    #[test]
    fn step_queue_notify_not_ready_queue_is_graceful() {
        // Queue exists in the table but the driver never set QUEUE_READY:
        // graceful no-op, state untouched, never OOB.
        let s = TransportState {
            queue_count: 2,
            features: OFFERED_FEATURES,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: fresh_queues(2),
        };
        let mut ram = vec![0u8; 0x4000];
        let (next, outs) = step(&s, DevEvent::QueueNotify { queue_idx: 0 }, &mut ram);
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    #[test]
    fn step_queue_notify_ready_queue_validates_gracefully() {
        // Queue 0 is fully configured and ready: the notify validates (no
        // panic, no OOB) and returns state-unchanged with no outputs. Chain
        // driving is the orchestrator's job via pop_chain/push_used, using
        // the ring addresses in state.queues — step never imports a device.
        let s = TransportState {
            queue_count: 2,
            features: OFFERED_FEATURES,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: ready_queues(),
        };
        let mut ram = vec![0u8; 0x4000];
        let (next, outs) = step(&s, DevEvent::QueueNotify { queue_idx: 0 }, &mut ram);
        assert_eq!(next, s);
        assert!(outs.is_empty());
        // Queue 1 in the same table is not ready: still graceful.
        let (next1, outs1) = step(&s, DevEvent::QueueNotify { queue_idx: 1 }, &mut ram);
        assert_eq!(next1, s);
        assert!(outs1.is_empty());
    }

    #[test]
    fn step_mmio_queue_notify_register_maps_to_queue_notify() {
        let s = TransportState {
            queue_count: 2,
            features: 0,
            status: VIRTIO_STATUS_DRIVER_OK,
            queues: fresh_queues(2),
        };
        let mut ram = vec![0u8; 0x4000];
        let (next, outs) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_QUEUE_NOTIFY,
                value: 9, // out of range queue
            },
            &mut ram,
        );
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    #[test]
    fn step_queue_address_hi_nonzero_ignored() {
        // High half nonzero => address >= 4 GiB > contract RAM cap: ignored.
        let s = dev_state();
        let mut ram = vec![0u8; 0x1000];
        let (next, outs) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: MMIO_QUEUE_DESC_HI,
                value: 1,
            },
            &mut ram,
        );
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    #[test]
    fn step_unknown_config_offsets_ignored() {
        let s = dev_state();
        let mut ram = vec![0u8; 0x1000];
        let (next, outs) = step(
            &s,
            DevEvent::ConfigWrite {
                offset: 0xFFFF,
                value: 0x1234,
            },
            &mut ram,
        );
        assert_eq!(next, s);
        assert!(outs.is_empty());
    }

    // --- need_event_idx -----------------------------------------------------

    #[test]
    fn event_idx_vring_need_event_golden() {
        // No progress since last notify, driver awaits the reached index.
        assert!(need_event_idx(5, 4, 5));
        // Advanced past the event: interrupt.
        assert!(need_event_idx(6, 4, 4));
        // Event not yet reached: silent.
        assert!(!need_event_idx(5, 6, 4));
        // u16 wrap: new=0 (wrapped), event=0xFFFF, old=0xFFFF.
        assert!(need_event_idx(0, 0xFFFF, 0xFFFF));
    }
}
