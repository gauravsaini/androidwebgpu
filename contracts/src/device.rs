//! Device contract types — owned by Wave 0 leaf 1.2.
//!
//! Covers U6 (virtio transport), U7 (virtio-gpu device), U8 (gpu host stack),
//! U13 (browser adapters speak these types at the boundary).

/// Events the orchestrator routes into a device model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevEvent {
    QueueNotify { queue_idx: u16 },
    ConfigWrite { offset: u64, value: u64 },
    ConfigRead { offset: u64 },
    Reset,
}

/// Outputs a device model returns. The orchestrator delivers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevOut {
    ConfigValue(u64),
    UsedRingUpdate { queue_idx: u16 },
    IrqAssert { num: u32 },
    /// virtio-gpu → GPU half (U8). This is the stream Path E never wired.
    GpuCommands(Vec<GpuCmd>),
    /// virtio-net → net adapter (U13).
    NetPacket(Vec<u8>),
}

/// Guest → host GPU command stream (virtio-gpu 3D protocol, typed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuCmd {
    Transfer2D {
        resource_id: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        data: Vec<u8>,
    },
    Submit3D {
        ctx_id: u32,
        commands: Vec<u8>,
    },
    Scanout {
        resource_id: u32,
        w: u32,
        h: u32,
    },
    Fence { id: u64 },
}

/// Explicit virtio transport state (U6). Queues live here, not in globals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportState {
    pub queue_count: u16,
    pub features: u64,
    pub status: u8,
}

/// Explicit virtio-gpu device state (U7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDevState {
    pub next_resource_id: u32,
    pub next_fence_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit3d_is_accepted_as_data() {
        // Path E's test asserted 3D was REJECTED. Path N accepts it.
        let cmd = GpuCmd::Submit3D {
            ctx_id: 1,
            commands: vec![0xDE, 0xAD],
        };
        let out = DevOut::GpuCommands(vec![cmd]);
        match out {
            DevOut::GpuCommands(cmds) => assert_eq!(cmds.len(), 1),
            _ => panic!("expected GpuCommands"),
        }
    }

    #[test]
    fn fence_ids_are_ordered_values() {
        let a = GpuCmd::Fence { id: 7 };
        let b = GpuCmd::Fence { id: 8 };
        assert_ne!(a, b);
    }

    #[test]
    fn dev_events_cover_config_read_path() {
        let ev = DevEvent::ConfigRead { offset: 0x10 };
        let out = DevOut::ConfigValue(0x42);
        assert_eq!(ev, DevEvent::ConfigRead { offset: 0x10 });
        assert_eq!(out, DevOut::ConfigValue(0x42));
    }
}
