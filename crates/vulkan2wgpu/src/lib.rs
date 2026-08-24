pub mod buffer;
pub mod command;
pub mod descriptor;
pub mod device;
pub mod image;
pub mod pipeline;
pub mod spirv;
pub mod types;

pub use buffer::*;
pub use command::*;
pub use descriptor::*;
pub use device::*;
pub use image::*;
pub use pipeline::*;
pub use spirv::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vulkan_types_and_constants() {
        assert_eq!(VK_FORMAT_R8G8B8A8_UNORM, 37);
        assert_eq!(
            vk_format_to_wgpu(VK_FORMAT_R8G8B8A8_UNORM),
            Some(wgpu::TextureFormat::Rgba8Unorm)
        );
        assert_eq!(
            vk_compare_op_to_wgpu(VK_COMPARE_OP_LESS),
            wgpu::CompareFunction::Less
        );
    }

    #[test]
    fn test_vulkan_spirv_translator_creation() {
        let translator = SpirvTranslator::new();
        assert!(translator.translate_spirv_bytes(&[0u8; 3]).is_err());
    }

    #[test]
    fn test_vulkan_device_memory_mapping_and_dirty_ranges() {
        let mut mem = VkDeviceMemory::new(1, 1024, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
        let ptr = mem.map_memory(0, 128).unwrap();
        assert!(!ptr.is_null());

        unsafe {
            std::ptr::write_bytes(ptr, 0xAA, 128);
        }

        mem.flush_range(0, 128);
        assert_eq!(mem.dirty_ranges.len(), 1);
        assert_eq!(mem.dirty_ranges[0].size, 128);
        assert_eq!(mem.shadow_buffer[0], 0xAA);
    }
}
