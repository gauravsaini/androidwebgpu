use vulkan2wgpu::*;

#[test]
fn test_vulkan_device_and_resource_creation() {
    pollster::block_on(async {
        let mut vk_device = match VkDevice::new().await {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Adapter unavailable, skipping hardware test: {:?}", e);
                return;
            }
        };

        // 1. Create Buffer
        let buf_id = vk_device.vk_create_buffer(1024, VK_BUFFER_USAGE_VERTEX_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT);
        assert!(buf_id > 0);

        // 2. Allocate & Bind Memory
        let mem_id = vk_device.vk_allocate_memory(1024, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
        assert!(mem_id > 0);
        vk_device.vk_bind_buffer_memory(buf_id, mem_id, 0);

        // 3. Create Image & Image View
        let img_id = vk_device.vk_create_image(
            256,
            256,
            1,
            1,
            1,
            VK_FORMAT_R8G8B8A8_UNORM,
            VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_SAMPLED_BIT,
        );
        assert!(img_id > 0);

        let view_id = vk_device.vk_create_image_view(img_id, VK_FORMAT_R8G8B8A8_UNORM);
        assert!(view_id > 0);

        // 4. Create Sampler
        let samp_id = vk_device.vk_create_sampler(1, 1, 0, 0, 0);
        assert!(samp_id > 0);

        // 5. Create Command Buffer and record rendering pass
        let cb_id = vk_device.vk_create_command_buffer();
        assert!(cb_id > 0);

        if let Some(cb) = vk_device.command_buffers.get_mut(&cb_id) {
            cb.begin();
            cb.record(VkCommand::BeginRendering {
                color_attachments: vec![VkRenderingAttachmentInfo {
                    image_view_id: view_id,
                    load_op: 1, // Clear
                    store_op: 0, // Store
                    clear_value: Some(VkClearValue::Color(VkClearColorValue {
                        float32: [0.1, 0.2, 0.3, 1.0],
                    })),
                }],
                depth_attachment: None,
            });
            cb.record(VkCommand::SetViewport {
                x: 0.0,
                y: 0.0,
                width: 256.0,
                height: 256.0,
                min_depth: 0.0,
                max_depth: 1.0,
            });
            cb.record(VkCommand::SetScissor {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            });
            cb.record(VkCommand::Draw {
                vertex_count: 3,
                instance_count: 1,
                first_vertex: 0,
                first_instance: 0,
            });
            cb.record(VkCommand::EndRendering);
            cb.end();
        }

        // 6. Submit to WebGPU Queue
        vk_device.vk_queue_submit(&[cb_id]);
    });
}
