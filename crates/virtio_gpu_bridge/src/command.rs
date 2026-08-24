use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpuCommand {
    CreateContext {
        ctx_id: u32,
        debug_name: String,
    },
    DestroyContext {
        ctx_id: u32,
    },
    CreateResource2D {
        resource_id: u32,
        format: u32,
        width: u32,
        height: u32,
    },
    UnrefResource {
        resource_id: u32,
    },
    AttachBacking {
        resource_id: u32,
        data_len: usize,
    },
    TransferToHost2D {
        resource_id: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        offset: u64,
        data: Vec<u8>,
    },
    SetScanout {
        scanout_id: u32,
        resource_id: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    ResourceFlush {
        resource_id: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Submit3D {
        ctx_id: u32,
        commands: Vec<u8>,
    },
    Present {
        scanout_id: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub status: u32,
    pub fence_id: u64,
    pub payload: Vec<u8>,
}
