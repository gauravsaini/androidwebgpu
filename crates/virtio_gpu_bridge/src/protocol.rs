use bytemuck::{Pod, Zeroable};

// OASIS Virtio 1.2 GPU Protocol Constants & Header Structs
pub const VIRTIO_GPU_CMD_GET_DISPLAY_INFO: u32 = 0x0100;
pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
pub const VIRTIO_GPU_CMD_RESOURCE_UNREF: u32 = 0x0102;
pub const VIRTIO_GPU_CMD_SET_SCANOUT: u32 = 0x0103;
pub const VIRTIO_GPU_CMD_RESOURCE_FLUSH: u32 = 0x0104;
pub const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
pub const VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
pub const VIRTIO_GPU_CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
pub const VIRTIO_GPU_CMD_GET_CAPSET_INFO: u32 = 0x0108;
pub const VIRTIO_GPU_CMD_GET_CAPSET: u32 = 0x0109;
pub const VIRTIO_GPU_CMD_GET_EDID: u32 = 0x010a;

// 3D Commands
pub const VIRTIO_GPU_CMD_CTX_CREATE: u32 = 0x0200;
pub const VIRTIO_GPU_CMD_CTX_DESTROY: u32 = 0x0201;
pub const VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE: u32 = 0x0202;
pub const VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE: u32 = 0x0203;
pub const VIRTIO_GPU_CMD_RESOURCE_CREATE_3D: u32 = 0x0204;
pub const VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D: u32 = 0x0205;
pub const VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D: u32 = 0x0206;
pub const VIRTIO_GPU_CMD_SUBMIT_3D: u32 = 0x0207;

// Virtio 3D Vulkan Wire Opcodes (for Submit3D Vulkan stream)
pub const VIRTGPU_VK_CMD_QUEUE_SUBMIT: u32 = 0x0020;
pub const VIRTGPU_VK_CMD_BEGIN_RENDERING: u32 = 0x0021;
pub const VIRTGPU_VK_CMD_END_RENDERING: u32 = 0x0022;
pub const VIRTGPU_VK_CMD_BIND_PIPELINE: u32 = 0x0023;
pub const VIRTGPU_VK_CMD_BIND_VERTEX_BUFFERS: u32 = 0x0024;
pub const VIRTGPU_VK_CMD_BIND_INDEX_BUFFER: u32 = 0x0025;
pub const VIRTGPU_VK_CMD_DRAW: u32 = 0x0026;
pub const VIRTGPU_VK_CMD_DRAW_INDEXED: u32 = 0x0027;
pub const VIRTGPU_VK_CMD_SET_VIEWPORT: u32 = 0x0028;
pub const VIRTGPU_VK_CMD_SET_SCISSOR: u32 = 0x0029;
pub const VIRTGPU_VK_CMD_BIND_DESCRIPTOR_SETS: u32 = 0x002A;
pub const VIRTGPU_VK_CMD_PUSH_CONSTANTS: u32 = 0x002B;
pub const VIRTGPU_VK_CMD_COPY_IMAGE_TO_BUFFER: u32 = 0x002C;

// Responses
pub const VIRTIO_GPU_RESP_OK_NODATA: u32 = 0x1100;
pub const VIRTIO_GPU_RESP_OK_DISPLAY_INFO: u32 = 0x1101;
pub const VIRTIO_GPU_RESP_OK_CAPSET_INFO: u32 = 0x1102;
pub const VIRTIO_GPU_RESP_OK_CAPSET: u32 = 0x1103;
pub const VIRTIO_GPU_RESP_OK_EDID: u32 = 0x1104;

pub const VIRTIO_GPU_RESP_ERR_UNSPEC: u32 = 0x1200;
pub const VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY: u32 = 0x1201;
pub const VIRTIO_GPU_RESP_ERR_INVALID_SCANOUT_ID: u32 = 0x1202;
pub const VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID: u32 = 0x1203;
pub const VIRTIO_GPU_RESP_ERR_INVALID_CONTEXT_ID: u32 = 0x1204;
pub const VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER: u32 = 0x1205;

// Formats
pub const VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM: u32 = 1;
pub const VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM: u32 = 2;
pub const VIRTIO_GPU_FORMAT_A8R8G8B8_UNORM: u32 = 3;
pub const VIRTIO_GPU_FORMAT_X8R8G8B8_UNORM: u32 = 4;
pub const VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM: u32 = 67;
pub const VIRTIO_GPU_FORMAT_X8B8G8R8_UNORM: u32 = 68;

// Flags
pub const VIRTIO_GPU_FLAG_FENCE: u32 = 1 << 0;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuCtrlHdr {
    pub type_: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuBox {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
    pub h: u32,
    pub d: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuDisplayOne {
    pub r: VirtioGpuRect,
    pub enabled: u32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VirtioGpuRespDisplayInfo {
    pub hdr: VirtioGpuCtrlHdr,
    pub pmodes: [VirtioGpuDisplayOne; 16],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuResourceCreate2d {
    pub hdr: VirtioGpuCtrlHdr,
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuSetScanout {
    pub hdr: VirtioGpuCtrlHdr,
    pub r: VirtioGpuRect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuResourceFlush {
    pub hdr: VirtioGpuCtrlHdr,
    pub r: VirtioGpuRect,
    pub resource_id: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuTransferToHost2d {
    pub hdr: VirtioGpuCtrlHdr,
    pub r: VirtioGpuRect,
    pub offset: u64,
    pub resource_id: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuTransferToHost3d {
    pub hdr: VirtioGpuCtrlHdr,
    pub box_: VirtioGpuBox,
    pub offset: u64,
    pub resource_id: u32,
    pub level: u32,
    pub stride: u32,
    pub layer_stride: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct VirtioGpuCtxCreate {
    pub hdr: VirtioGpuCtrlHdr,
    pub nlen: u32,
    pub padding: u32,
    pub debug_name: [u8; 64],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuSubmit3d {
    pub hdr: VirtioGpuCtrlHdr,
    pub size: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuResourceAttachBacking {
    pub hdr: VirtioGpuCtrlHdr,
    pub resource_id: u32,
    pub nr_entries: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuMemEntry {
    pub addr: u64,
    pub length: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuCtxResource {
    pub hdr: VirtioGpuCtrlHdr,
    pub resource_id: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuResourceCreate3d {
    pub hdr: VirtioGpuCtrlHdr,
    pub resource_id: u32,
    pub target: u32,
    pub format: u32,
    pub bind: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub array_size: u32,
    pub last_level: u32,
    pub nr_samples: u32,
    pub flags: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuGetCapsetInfo {
    pub hdr: VirtioGpuCtrlHdr,
    pub capset_index: u32,
    pub padding: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtioGpuRespCapsetInfo {
    pub hdr: VirtioGpuCtrlHdr,
    pub capset_id: u32,
    pub capset_max_version: u32,
    pub capset_max_size: u32,
    pub padding: u32,
}

// Virtqueue Ring Descriptors (OASIS VirtIO 1.2 Specification)
pub const VRING_DESC_F_NEXT: u16 = 1;
pub const VRING_DESC_F_WRITE: u16 = 2;
pub const VRING_DESC_F_INDIRECT: u16 = 4;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}
