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
        let mem_id = vk_device.vk_allocate_memory(1024, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT).unwrap();
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

        // 5. Create Pipeline Layout
        let layout_id = vk_device.vk_create_pipeline_layout(vec![], vec![]);
        assert!(layout_id > 0);

        // 6. Create Graphics Pipeline
        let vs_wgsl = r#"
            @vertex
            fn main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(pos, 1.0);
            }
        "#;
        let fs_wgsl = r#"
            @fragment
            fn main() -> @location(0) vec4<f32> {
                return vec4<f32>(0.0, 1.0, 0.0, 1.0);
            }
        "#;

        let pipe_id = vk_device.vk_create_graphics_pipeline(&VkGraphicsPipelineCreateInfo {
            layout_id,
            vertex_shader_wgsl: vs_wgsl.to_string(),
            fragment_shader_wgsl: Some(fs_wgsl.to_string()),
            vertex_bindings: vec![VkVertexBindingDescription {
                binding: 0,
                stride: 12,
                input_rate: 0,
            }],
            vertex_attributes: vec![VkVertexAttributeDescription {
                location: 0,
                binding: 0,
                format: VK_FORMAT_R32G32B32_SFLOAT,
                offset: 0,
            }],
            topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            color_formats: vec![VK_FORMAT_R8G8B8A8_UNORM],
            depth_format: None,
        }).expect("Failed to create graphics pipeline");
        assert!(pipe_id > 0);

        // 7. Write vertex data to memory and flush
        let triangle_vertices: [f32; 9] = [
            0.0, 0.5, 0.0,
            -0.5, -0.5, 0.0,
            0.5, -0.5, 0.0,
        ];
        let bytes: &[u8] = bytemuck::cast_slice(&triangle_vertices);
        if let Some(mem) = vk_device.memories.get_mut(&mem_id) {
            mem.write_memory(0, bytes).unwrap();
        }

        // 8. Create Readback Buffer for Pixel Verification
        let readback_buf_id = vk_device.vk_create_buffer(
            256 * 256 * 4,
            VK_BUFFER_USAGE_TRANSFER_DST_BIT,
        );

        // 9. Record Command Buffer
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
            cb.record(VkCommand::BindPipeline { pipeline_id: pipe_id });
            cb.record(VkCommand::BindVertexBuffers {
                first_binding: 0,
                buffer_ids: vec![buf_id],
                offsets: vec![0],
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
            cb.record(VkCommand::CopyImageToBuffer {
                image_id: img_id,
                buffer_id: readback_buf_id,
                width: 256,
                height: 256,
            });
            cb.end();
        }

        // 10. Submit to WebGPU Queue
        vk_device.vk_queue_submit(&[cb_id]);

        // 11. Verify Readback Pixels
        let readback_buf = vk_device.buffers.get(&readback_buf_id).unwrap();
        let w_buf = readback_buf.wgpu_buffer.as_ref().unwrap();

        let slice = w_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            tx.send(res).unwrap();
        });
        vk_device.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("Failed to map buffer");

        let mapped = slice.get_mapped_range();
        // Check pixel at (0,0) which was cleared to [0.1, 0.2, 0.3, 1.0] -> RGBA values ~ (25, 51, 76, 255)
        let r = mapped[0];
        let g = mapped[1];
        let b = mapped[2];
        let a = mapped[3];
        assert!(r >= 20 && r <= 30, "Red channel was: {}", r);
        assert!(g >= 45 && g <= 55, "Green channel was: {}", g);
        assert!(b >= 70 && b <= 85, "Blue channel was: {}", b);
        assert_eq!(a, 255);
        drop(mapped);
        w_buf.unmap();
    });
}

#[test]
fn test_vulkan_indexed_draw_with_depth_stencil() {
    pollster::block_on(async {
        let mut vk_device = match VkDevice::new().await {
            Ok(d) => d,
            Err(_) => return,
        };

        // Vertex Buffer (Quad: 4 vertices)
        let vbuf_id = vk_device.vk_create_buffer(1024, VK_BUFFER_USAGE_VERTEX_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT);
        let vmem_id = vk_device.vk_allocate_memory(1024, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT).unwrap();
        vk_device.vk_bind_buffer_memory(vbuf_id, vmem_id, 0);

        let quad_vertices: [f32; 12] = [
            -0.5,  0.5, 0.5,
            -0.5, -0.5, 0.5,
             0.5, -0.5, 0.5,
             0.5,  0.5, 0.5,
        ];
        if let Some(mem) = vk_device.memories.get_mut(&vmem_id) {
            mem.write_memory(0, bytemuck::cast_slice(&quad_vertices)).unwrap();
        }

        // Index Buffer (6 indices for 2 triangles)
        let ibuf_id = vk_device.vk_create_buffer(512, VK_BUFFER_USAGE_INDEX_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT);
        let imem_id = vk_device.vk_allocate_memory(512, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT).unwrap();
        vk_device.vk_bind_buffer_memory(ibuf_id, imem_id, 0);

        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        if let Some(mem) = vk_device.memories.get_mut(&imem_id) {
            mem.write_memory(0, bytemuck::cast_slice(&indices)).unwrap();
        }

        // Color & Depth Target
        let color_img = vk_device.vk_create_image(64, 64, 1, 1, 1, VK_FORMAT_R8G8B8A8_UNORM, VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT);
        let color_view = vk_device.vk_create_image_view(color_img, VK_FORMAT_R8G8B8A8_UNORM);

        let depth_img = vk_device.vk_create_image(64, 64, 1, 1, 1, VK_FORMAT_D24_UNORM_S8_UINT, VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT);
        let depth_view = vk_device.vk_create_image_view(depth_img, VK_FORMAT_D24_UNORM_S8_UINT);

        let layout_id = vk_device.vk_create_pipeline_layout(vec![], vec![]);

        let vs_wgsl = r#"
            @vertex
            fn main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(pos, 1.0);
            }
        "#;
        let fs_wgsl = r#"
            @fragment
            fn main() -> @location(0) vec4<f32> {
                return vec4<f32>(1.0, 0.0, 0.0, 1.0);
            }
        "#;

        let pipe_id = vk_device.vk_create_graphics_pipeline(&VkGraphicsPipelineCreateInfo {
            layout_id,
            vertex_shader_wgsl: vs_wgsl.to_string(),
            fragment_shader_wgsl: Some(fs_wgsl.to_string()),
            vertex_bindings: vec![VkVertexBindingDescription {
                binding: 0,
                stride: 12,
                input_rate: 0,
            }],
            vertex_attributes: vec![VkVertexAttributeDescription {
                location: 0,
                binding: 0,
                format: VK_FORMAT_R32G32B32_SFLOAT,
                offset: 0,
            }],
            topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            color_formats: vec![VK_FORMAT_R8G8B8A8_UNORM],
            depth_format: Some(VK_FORMAT_D24_UNORM_S8_UINT),
        }).expect("Failed to create pipeline with depth");

        let cb_id = vk_device.vk_create_command_buffer();
        if let Some(cb) = vk_device.command_buffers.get_mut(&cb_id) {
            cb.begin();
            cb.record(VkCommand::BeginRendering {
                color_attachments: vec![VkRenderingAttachmentInfo {
                    image_view_id: color_view,
                    load_op: 1,
                    store_op: 0,
                    clear_value: Some(VkClearValue::Color(VkClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] })),
                }],
                depth_attachment: Some(VkRenderingAttachmentInfo {
                    image_view_id: depth_view,
                    load_op: 1,
                    store_op: 0,
                    clear_value: Some(VkClearValue::DepthStencil(VkClearDepthStencilValue { depth: 1.0, stencil: 0 })),
                }),
            });
            cb.record(VkCommand::BindPipeline { pipeline_id: pipe_id });
            cb.record(VkCommand::BindVertexBuffers {
                first_binding: 0,
                buffer_ids: vec![vbuf_id],
                offsets: vec![0],
            });
            cb.record(VkCommand::BindIndexBuffer {
                buffer_id: ibuf_id,
                offset: 0,
                index_type: 0, // Uint16
            });
            cb.record(VkCommand::DrawIndexed {
                index_count: 6,
                instance_count: 1,
                first_index: 0,
                vertex_offset: 0,
                first_instance: 0,
            });
            cb.record(VkCommand::EndRendering);
            cb.end();
        }

        vk_device.vk_queue_submit(&[cb_id]);
    });
}

#[test]
fn test_vulkan_push_constants_and_descriptors() {
    pollster::block_on(async {
        let mut vk_device = match VkDevice::new().await {
            Ok(d) => d,
            Err(_) => return,
        };

        // Create uniform buffer
        let ubuf_id = vk_device.vk_create_buffer(256, VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT);
        let umem_id = vk_device.vk_allocate_memory(256, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT).unwrap();
        vk_device.vk_bind_buffer_memory(ubuf_id, umem_id, 0);

        let dsl_id = vk_device.vk_create_descriptor_set_layout(vec![
            VkDescriptorSetLayoutBinding {
                binding: 0,
                descriptor_type: VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER,
                descriptor_count: 1,
                stage_flags: 1,
            },
        ]);

        let ds_id = vk_device.vk_create_descriptor_set(dsl_id);
        if let Some(ds) = vk_device.descriptor_sets.get_mut(&ds_id) {
            ds.write_buffer(0, ubuf_id, 0, 64);
        }

        let pipe_layout_id = vk_device.vk_create_pipeline_layout(
            vec![dsl_id],
            vec![VkPushConstantRange {
                stage_flags: 1,
                offset: 0,
                size: 16,
            }],
        );
        assert!(pipe_layout_id > 0);

        let cb_id = vk_device.vk_create_command_buffer();
        if let Some(cb) = vk_device.command_buffers.get_mut(&cb_id) {
            cb.begin();
            cb.record(VkCommand::PushConstants {
                offset: 0,
                data: vec![1, 2, 3, 4],
            });
            cb.record(VkCommand::BindDescriptorSets {
                first_set: 0,
                descriptor_set_ids: vec![ds_id],
                dynamic_offsets: vec![],
            });
            cb.end();
        }

        vk_device.vk_queue_submit(&[cb_id]);
    });
}

#[test]
fn test_vulkan_multi_binding_vertex_layout() {
    pollster::block_on(async {
        let mut vk_device = match VkDevice::new().await {
            Ok(d) => d,
            Err(_) => return,
        };

        let layout_id = vk_device.vk_create_pipeline_layout(vec![], vec![]);

        let vs_wgsl = r#"
            @vertex
            fn main(
                @location(0) pos: vec3<f32>,
                @location(1) instance_pos: vec2<f32>,
            ) -> @builtin(position) vec4<f32> {
                return vec4<f32>(pos.xy + instance_pos, pos.z, 1.0);
            }
        "#;
        let fs_wgsl = r#"
            @fragment
            fn main() -> @location(0) vec4<f32> {
                return vec4<f32>(1.0, 1.0, 1.0, 1.0);
            }
        "#;

        let pipe_id = vk_device.vk_create_graphics_pipeline(&VkGraphicsPipelineCreateInfo {
            layout_id,
            vertex_shader_wgsl: vs_wgsl.to_string(),
            fragment_shader_wgsl: Some(fs_wgsl.to_string()),
            vertex_bindings: vec![
                VkVertexBindingDescription {
                    binding: 0,
                    stride: 12,
                    input_rate: 0, // Vertex
                },
                VkVertexBindingDescription {
                    binding: 1,
                    stride: 8,
                    input_rate: 1, // Instance
                },
            ],
            vertex_attributes: vec![
                VkVertexAttributeDescription {
                    location: 0,
                    binding: 0,
                    format: VK_FORMAT_R32G32B32_SFLOAT,
                    offset: 0,
                },
                VkVertexAttributeDescription {
                    location: 1,
                    binding: 1,
                    format: VK_FORMAT_R32G32_SFLOAT,
                    offset: 0,
                },
            ],
            topology: VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            color_formats: vec![VK_FORMAT_R8G8B8A8_UNORM],
            depth_format: None,
        }).expect("Multi-binding pipeline creation failed");

        assert!(pipe_id > 0);
    });
}

#[test]
fn test_vulkan_oversized_allocation_protection() {
    pollster::block_on(async {
        let mut vk_device = match VkDevice::new().await {
            Ok(d) => d,
            Err(_) => return,
        };

        // 1 GB request exceeds MAX_SAFE_BUFFER_SIZE (256 MB) -> must return Err
        let res = vk_device.vk_allocate_memory(1 << 30, 0, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
        assert_eq!(res, Err(VK_ERROR_OUT_OF_DEVICE_MEMORY));
    });
}

#[test]
fn test_vulkan_with_shared_device_queue() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = match instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }).await {
            Some(a) => a,
            None => return,
        };

        let (device, queue) = match adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Shared Device Test"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ).await {
            Ok(pair) => pair,
            Err(_) => return,
        };

        let dev_arc = std::sync::Arc::new(device);
        let queue_arc = std::sync::Arc::new(queue);

        let mut vk_device = VkDevice::with_device_queue(std::sync::Arc::clone(&dev_arc), std::sync::Arc::clone(&queue_arc))
            .expect("Failed to initialize VkDevice with shared device and queue");

        assert_eq!(std::sync::Arc::strong_count(&dev_arc), 2);
        assert_eq!(std::sync::Arc::strong_count(&queue_arc), 2);

        let buf_id = vk_device.vk_create_buffer(512, VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT);
        assert!(buf_id > 0);
    });
}

