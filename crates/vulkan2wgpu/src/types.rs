
// Vulkan Format constants
pub const VK_FORMAT_UNDEFINED: u32 = 0;
pub const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
pub const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
pub const VK_FORMAT_B8G8R8A8_UNORM: u32 = 44;
pub const VK_FORMAT_B8G8R8A8_SRGB: u32 = 50;
pub const VK_FORMAT_D24_UNORM_S8_UINT: u32 = 129;
pub const VK_FORMAT_D32_SFLOAT: u32 = 126;
pub const VK_FORMAT_D32_SFLOAT_S8_UINT: u32 = 130;

// Vulkan Buffer Usage Flags
pub const VK_BUFFER_USAGE_TRANSFER_SRC_BIT: u32 = 0x00000001;
pub const VK_BUFFER_USAGE_TRANSFER_DST_BIT: u32 = 0x00000002;
pub const VK_BUFFER_USAGE_UNIFORM_TEXEL_BUFFER_BIT: u32 = 0x00000004;
pub const VK_BUFFER_USAGE_STORAGE_TEXEL_BUFFER_BIT: u32 = 0x00000008;
pub const VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT: u32 = 0x00000010;
pub const VK_BUFFER_USAGE_STORAGE_BUFFER_BIT: u32 = 0x00000020;
pub const VK_BUFFER_USAGE_INDEX_BUFFER_BIT: u32 = 0x00000040;
pub const VK_BUFFER_USAGE_VERTEX_BUFFER_BIT: u32 = 0x00000080;
pub const VK_BUFFER_USAGE_INDIRECT_BUFFER_BIT: u32 = 0x00000100;

// Vulkan Image Usage Flags
pub const VK_IMAGE_USAGE_TRANSFER_SRC_BIT: u32 = 0x00000001;
pub const VK_IMAGE_USAGE_TRANSFER_DST_BIT: u32 = 0x00000002;
pub const VK_IMAGE_USAGE_SAMPLED_BIT: u32 = 0x00000004;
pub const VK_IMAGE_USAGE_STORAGE_BIT: u32 = 0x00000008;
pub const VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT: u32 = 0x00000010;
pub const VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT: u32 = 0x00000020;

// Vulkan Memory Property Flags
pub const VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT: u32 = 0x00000001;
pub const VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT: u32 = 0x00000002;
pub const VK_MEMORY_PROPERTY_HOST_COHERENT_BIT: u32 = 0x00000004;
pub const VK_MEMORY_PROPERTY_HOST_CACHED_BIT: u32 = 0x00000008;

// Vulkan Descriptor Types
pub const VK_DESCRIPTOR_TYPE_SAMPLER: u32 = 0;
pub const VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER: u32 = 1;
pub const VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE: u32 = 2;
pub const VK_DESCRIPTOR_TYPE_STORAGE_IMAGE: u32 = 3;
pub const VK_DESCRIPTOR_TYPE_UNIFORM_TEXEL_BUFFER: u32 = 4;
pub const VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER: u32 = 5;
pub const VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER: u32 = 6;
pub const VK_DESCRIPTOR_TYPE_STORAGE_BUFFER: u32 = 7;
pub const VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER_DYNAMIC: u32 = 8;
pub const VK_DESCRIPTOR_TYPE_STORAGE_BUFFER_DYNAMIC: u32 = 9;

// Vulkan Primitive Topology
pub const VK_PRIMITIVE_TOPOLOGY_POINT_LIST: u32 = 0;
pub const VK_PRIMITIVE_TOPOLOGY_LINE_LIST: u32 = 1;
pub const VK_PRIMITIVE_TOPOLOGY_LINE_STRIP: u32 = 2;
pub const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST: u32 = 3;
pub const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP: u32 = 4;
pub const VK_PRIMITIVE_TOPOLOGY_TRIANGLE_FAN: u32 = 5;

// Vulkan Compare Op
pub const VK_COMPARE_OP_NEVER: u32 = 0;
pub const VK_COMPARE_OP_LESS: u32 = 1;
pub const VK_COMPARE_OP_EQUAL: u32 = 2;
pub const VK_COMPARE_OP_LESS_OR_EQUAL: u32 = 3;
pub const VK_COMPARE_OP_GREATER: u32 = 4;
pub const VK_COMPARE_OP_NOT_EQUAL: u32 = 5;
pub const VK_COMPARE_OP_GREATER_OR_EQUAL: u32 = 6;
pub const VK_COMPARE_OP_ALWAYS: u32 = 7;

// Vulkan Result codes
pub const VK_SUCCESS: i32 = 0;
pub const VK_NOT_READY: i32 = 1;
pub const VK_TIMEOUT: i32 = 2;
pub const VK_ERROR_OUT_OF_HOST_MEMORY: i32 = -1;
pub const VK_ERROR_OUT_OF_DEVICE_MEMORY: i32 = -2;
pub const VK_ERROR_INITIALIZATION_FAILED: i32 = -3;
pub const VK_ERROR_DEVICE_LOST: i32 = -4;

pub fn vk_format_to_wgpu(vk_format: u32) -> Option<wgpu::TextureFormat> {
    match vk_format {
        VK_FORMAT_R8G8B8A8_UNORM => Some(wgpu::TextureFormat::Rgba8Unorm),
        VK_FORMAT_R8G8B8A8_SRGB => Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        VK_FORMAT_B8G8R8A8_UNORM => Some(wgpu::TextureFormat::Bgra8Unorm),
        VK_FORMAT_B8G8R8A8_SRGB => Some(wgpu::TextureFormat::Bgra8UnormSrgb),
        VK_FORMAT_D24_UNORM_S8_UINT => Some(wgpu::TextureFormat::Depth24PlusStencil8),
        VK_FORMAT_D32_SFLOAT => Some(wgpu::TextureFormat::Depth32Float),
        VK_FORMAT_D32_SFLOAT_S8_UINT => Some(wgpu::TextureFormat::Depth32Float),
        _ => None,
    }
}

pub fn vk_compare_op_to_wgpu(op: u32) -> wgpu::CompareFunction {
    match op {
        VK_COMPARE_OP_NEVER => wgpu::CompareFunction::Never,
        VK_COMPARE_OP_LESS => wgpu::CompareFunction::Less,
        VK_COMPARE_OP_EQUAL => wgpu::CompareFunction::Equal,
        VK_COMPARE_OP_LESS_OR_EQUAL => wgpu::CompareFunction::LessEqual,
        VK_COMPARE_OP_GREATER => wgpu::CompareFunction::Greater,
        VK_COMPARE_OP_NOT_EQUAL => wgpu::CompareFunction::NotEqual,
        VK_COMPARE_OP_GREATER_OR_EQUAL => wgpu::CompareFunction::GreaterEqual,
        VK_COMPARE_OP_ALWAYS => wgpu::CompareFunction::Always,
        _ => wgpu::CompareFunction::LessEqual,
    }
}
