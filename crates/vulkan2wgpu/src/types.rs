// Vulkan Format constants
pub const VK_FORMAT_UNDEFINED: u32 = 0;
pub const VK_FORMAT_R8_UNORM: u32 = 9;
pub const VK_FORMAT_R8_SNORM: u32 = 10;
pub const VK_FORMAT_R8_UINT: u32 = 13;
pub const VK_FORMAT_R8_SINT: u32 = 14;
pub const VK_FORMAT_R8G8_UNORM: u32 = 16;
pub const VK_FORMAT_R8G8_SNORM: u32 = 17;
pub const VK_FORMAT_R8G8_UINT: u32 = 20;
pub const VK_FORMAT_R8G8_SINT: u32 = 21;
pub const VK_FORMAT_R8G8B8A8_UNORM: u32 = 37;
pub const VK_FORMAT_R8G8B8A8_SNORM: u32 = 38;
pub const VK_FORMAT_R8G8B8A8_UINT: u32 = 41;
pub const VK_FORMAT_R8G8B8A8_SINT: u32 = 42;
pub const VK_FORMAT_R8G8B8A8_SRGB: u32 = 43;
pub const VK_FORMAT_B8G8R8A8_UNORM: u32 = 44;
pub const VK_FORMAT_B8G8R8A8_SRGB: u32 = 50;
pub const VK_FORMAT_A2B10G10R10_UNORM_PACK32: u32 = 64;
pub const VK_FORMAT_R16_SFLOAT: u32 = 76;
pub const VK_FORMAT_R16G16_SFLOAT: u32 = 83;
pub const VK_FORMAT_R16G16B16A16_SFLOAT: u32 = 97;
pub const VK_FORMAT_R32_UINT: u32 = 98;
pub const VK_FORMAT_R32_SINT: u32 = 99;
pub const VK_FORMAT_R32_SFLOAT: u32 = 100;
pub const VK_FORMAT_R32G32_SFLOAT: u32 = 103;
pub const VK_FORMAT_R32G32B32_SFLOAT: u32 = 106;
pub const VK_FORMAT_R32G32B32A32_SFLOAT: u32 = 109;
pub const VK_FORMAT_D16_UNORM: u32 = 124;
pub const VK_FORMAT_D32_SFLOAT: u32 = 126;
pub const VK_FORMAT_D24_UNORM_S8_UINT: u32 = 129;
pub const VK_FORMAT_D32_SFLOAT_S8_UINT: u32 = 130;

// ETC2 Compressed Formats
pub const VK_FORMAT_ETC2_R8G8B8_UNORM_BLOCK: u32 = 147;
pub const VK_FORMAT_ETC2_R8G8B8_SRGB_BLOCK: u32 = 148;
pub const VK_FORMAT_ETC2_R8G8B8A1_UNORM_BLOCK: u32 = 149;
pub const VK_FORMAT_ETC2_R8G8B8A1_SRGB_BLOCK: u32 = 150;
pub const VK_FORMAT_ETC2_R8G8B8A8_UNORM_BLOCK: u32 = 151;
pub const VK_FORMAT_ETC2_R8G8B8A8_SRGB_BLOCK: u32 = 152;
pub const VK_FORMAT_EAC_R11_UNORM_BLOCK: u32 = 153;
pub const VK_FORMAT_EAC_R11_SNORM_BLOCK: u32 = 154;
pub const VK_FORMAT_EAC_R11G11_UNORM_BLOCK: u32 = 155;
pub const VK_FORMAT_EAC_R11G11_SNORM_BLOCK: u32 = 156;

// ASTC Compressed Formats
pub const VK_FORMAT_ASTC_4X4_UNORM_BLOCK: u32 = 157;
pub const VK_FORMAT_ASTC_4X4_SRGB_BLOCK: u32 = 158;
pub const VK_FORMAT_ASTC_5X4_UNORM_BLOCK: u32 = 159;
pub const VK_FORMAT_ASTC_5X5_UNORM_BLOCK: u32 = 161;
pub const VK_FORMAT_ASTC_6X5_UNORM_BLOCK: u32 = 163;
pub const VK_FORMAT_ASTC_6X6_UNORM_BLOCK: u32 = 165;
pub const VK_FORMAT_ASTC_8X8_UNORM_BLOCK: u32 = 173;

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

// Vulkan Image Types
pub const VK_IMAGE_TYPE_1D: u32 = 0;
pub const VK_IMAGE_TYPE_2D: u32 = 1;
pub const VK_IMAGE_TYPE_3D: u32 = 2;

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

// Vulkan Sampler Address Modes
pub const VK_SAMPLER_ADDRESS_MODE_REPEAT: u32 = 0;
pub const VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT: u32 = 1;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE: u32 = 2;
pub const VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER: u32 = 3;

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
        VK_FORMAT_R8_UNORM => Some(wgpu::TextureFormat::R8Unorm),
        VK_FORMAT_R8_SNORM => Some(wgpu::TextureFormat::R8Snorm),
        VK_FORMAT_R8_UINT => Some(wgpu::TextureFormat::R8Uint),
        VK_FORMAT_R8_SINT => Some(wgpu::TextureFormat::R8Sint),
        VK_FORMAT_R8G8_UNORM => Some(wgpu::TextureFormat::Rg8Unorm),
        VK_FORMAT_R8G8_SNORM => Some(wgpu::TextureFormat::Rg8Snorm),
        VK_FORMAT_R8G8_UINT => Some(wgpu::TextureFormat::Rg8Uint),
        VK_FORMAT_R8G8_SINT => Some(wgpu::TextureFormat::Rg8Sint),
        VK_FORMAT_R8G8B8A8_UNORM => Some(wgpu::TextureFormat::Rgba8Unorm),
        VK_FORMAT_R8G8B8A8_SNORM => Some(wgpu::TextureFormat::Rgba8Snorm),
        VK_FORMAT_R8G8B8A8_UINT => Some(wgpu::TextureFormat::Rgba8Uint),
        VK_FORMAT_R8G8B8A8_SINT => Some(wgpu::TextureFormat::Rgba8Sint),
        VK_FORMAT_R8G8B8A8_SRGB => Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        VK_FORMAT_B8G8R8A8_UNORM => Some(wgpu::TextureFormat::Bgra8Unorm),
        VK_FORMAT_B8G8R8A8_SRGB => Some(wgpu::TextureFormat::Bgra8UnormSrgb),
        VK_FORMAT_A2B10G10R10_UNORM_PACK32 => Some(wgpu::TextureFormat::Rgb10a2Unorm),
        VK_FORMAT_R16_SFLOAT => Some(wgpu::TextureFormat::R16Float),
        VK_FORMAT_R16G16_SFLOAT => Some(wgpu::TextureFormat::Rg16Float),
        VK_FORMAT_R16G16B16A16_SFLOAT => Some(wgpu::TextureFormat::Rgba16Float),
        VK_FORMAT_R32_UINT => Some(wgpu::TextureFormat::R32Uint),
        VK_FORMAT_R32_SINT => Some(wgpu::TextureFormat::R32Sint),
        VK_FORMAT_R32_SFLOAT => Some(wgpu::TextureFormat::R32Float),
        VK_FORMAT_R32G32_SFLOAT => Some(wgpu::TextureFormat::Rg32Float),
        VK_FORMAT_R32G32B32A32_SFLOAT => Some(wgpu::TextureFormat::Rgba32Float),
        VK_FORMAT_D16_UNORM => Some(wgpu::TextureFormat::Depth16Unorm),
        VK_FORMAT_D24_UNORM_S8_UINT => Some(wgpu::TextureFormat::Depth24PlusStencil8),
        VK_FORMAT_D32_SFLOAT => Some(wgpu::TextureFormat::Depth32Float),
        VK_FORMAT_D32_SFLOAT_S8_UINT => Some(wgpu::TextureFormat::Depth32Float),
        VK_FORMAT_ETC2_R8G8B8_UNORM_BLOCK => Some(wgpu::TextureFormat::Etc2Rgb8Unorm),
        VK_FORMAT_ETC2_R8G8B8_SRGB_BLOCK => Some(wgpu::TextureFormat::Etc2Rgb8UnormSrgb),
        VK_FORMAT_ETC2_R8G8B8A1_UNORM_BLOCK => Some(wgpu::TextureFormat::Etc2Rgb8A1Unorm),
        VK_FORMAT_ETC2_R8G8B8A1_SRGB_BLOCK => Some(wgpu::TextureFormat::Etc2Rgb8A1UnormSrgb),
        VK_FORMAT_ETC2_R8G8B8A8_UNORM_BLOCK => Some(wgpu::TextureFormat::Etc2Rgba8Unorm),
        VK_FORMAT_ETC2_R8G8B8A8_SRGB_BLOCK => Some(wgpu::TextureFormat::Etc2Rgba8UnormSrgb),
        VK_FORMAT_EAC_R11_UNORM_BLOCK => Some(wgpu::TextureFormat::EacR11Unorm),
        VK_FORMAT_EAC_R11_SNORM_BLOCK => Some(wgpu::TextureFormat::EacR11Snorm),
        VK_FORMAT_EAC_R11G11_UNORM_BLOCK => Some(wgpu::TextureFormat::EacRg11Unorm),
        VK_FORMAT_EAC_R11G11_SNORM_BLOCK => Some(wgpu::TextureFormat::EacRg11Snorm),
        VK_FORMAT_ASTC_4X4_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::Unorm,
        }),
        VK_FORMAT_ASTC_4X4_SRGB_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B4x4,
            channel: wgpu::AstcChannel::UnormSrgb,
        }),
        VK_FORMAT_ASTC_5X4_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B5x4,
            channel: wgpu::AstcChannel::Unorm,
        }),
        VK_FORMAT_ASTC_5X5_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B5x5,
            channel: wgpu::AstcChannel::Unorm,
        }),
        VK_FORMAT_ASTC_6X5_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B6x5,
            channel: wgpu::AstcChannel::Unorm,
        }),
        VK_FORMAT_ASTC_6X6_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B6x6,
            channel: wgpu::AstcChannel::Unorm,
        }),
        VK_FORMAT_ASTC_8X8_UNORM_BLOCK => Some(wgpu::TextureFormat::Astc {
            block: wgpu::AstcBlock::B8x8,
            channel: wgpu::AstcChannel::Unorm,
        }),
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

pub fn vk_address_mode_to_wgpu(mode: u32) -> wgpu::AddressMode {
    match mode {
        VK_SAMPLER_ADDRESS_MODE_REPEAT => wgpu::AddressMode::Repeat,
        VK_SAMPLER_ADDRESS_MODE_MIRRORED_REPEAT => wgpu::AddressMode::MirrorRepeat,
        VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE => wgpu::AddressMode::ClampToEdge,
        VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_BORDER => wgpu::AddressMode::ClampToBorder,
        _ => wgpu::AddressMode::ClampToEdge,
    }
}
