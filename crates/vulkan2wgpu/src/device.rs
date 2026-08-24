use crate::buffer::{VkBuffer, VkDeviceMemory};
use crate::command::*;
use crate::descriptor::{VkDescriptorSet, VkDescriptorSetLayout};
use crate::image::{VkImage, VkImageView, VkSampler};
use crate::pipeline::{
    VkGraphicsPipelineCreateInfo, VkPipeline, VkPipelineLayout, VkPipelineType,
    VkPushConstantRange,
};
use crate::spirv::SpirvTranslator;
use crate::types::*;
use std::collections::HashMap;

pub struct VkDevice {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub next_id: u64,
    pub memories: HashMap<u64, VkDeviceMemory>,
    pub buffers: HashMap<u64, VkBuffer>,
    pub images: HashMap<u64, VkImage>,
    pub image_views: HashMap<u64, VkImageView>,
    pub samplers: HashMap<u64, VkSampler>,
    pub descriptor_set_layouts: HashMap<u64, VkDescriptorSetLayout>,
    pub descriptor_sets: HashMap<u64, VkDescriptorSet>,
    pub pipeline_layouts: HashMap<u64, VkPipelineLayout>,
    pub pipelines: HashMap<u64, VkPipeline>,
    pub command_buffers: HashMap<u64, VkCommandBuffer>,
    pub push_constant_bgl: wgpu::BindGroupLayout,
    pub push_constant_buffer: wgpu::Buffer,
    pub push_constant_bg: wgpu::BindGroup,
    pub spirv_translator: SpirvTranslator,
}

impl VkDevice {
    pub async fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Failed to find suitable GPU adapter".to_string())?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|e| format!("Failed to create device: {:?}", e))?;

        let push_constant_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("VkPushConstants_BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::all(),
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let push_constant_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("VkPushConstants_Buffer"),
            size: 1024,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let push_constant_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("VkPushConstants_BG"),
            layout: &push_constant_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: push_constant_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            device,
            queue,
            next_id: 1,
            memories: HashMap::new(),
            buffers: HashMap::new(),
            images: HashMap::new(),
            image_views: HashMap::new(),
            samplers: HashMap::new(),
            descriptor_set_layouts: HashMap::new(),
            descriptor_sets: HashMap::new(),
            pipeline_layouts: HashMap::new(),
            pipelines: HashMap::new(),
            command_buffers: HashMap::new(),
            push_constant_bgl,
            push_constant_buffer,
            push_constant_bg,
            spirv_translator: SpirvTranslator::new(),
        })
    }

    fn gen_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn vk_allocate_memory(&mut self, size: u64, memory_type_index: u32, property_flags: u32) -> u64 {
        let id = self.gen_id();
        let mem = VkDeviceMemory::new(id, size, memory_type_index, property_flags);
        self.memories.insert(id, mem);
        id
    }

    pub fn vk_create_buffer(&mut self, size: u64, usage: u32) -> u64 {
        let id = self.gen_id();
        let mut buf = VkBuffer::new(id, size, usage);
        buf.create_wgpu_buffer(&self.device, None);
        self.buffers.insert(id, buf);
        id
    }

    pub fn vk_bind_buffer_memory(&mut self, buffer_id: u64, memory_id: u64, memory_offset: u64) {
        if let Some(buf) = self.buffers.get_mut(&buffer_id) {
            buf.memory_id = Some(memory_id);
            buf.memory_offset = memory_offset;
        }
        if let Some(mem) = self.memories.get_mut(&memory_id) {
            mem.bound_buffer_id = Some(buffer_id);
        }
    }

    pub fn vk_create_image(
        &mut self,
        width: u32,
        height: u32,
        depth: u32,
        mip_levels: u32,
        array_layers: u32,
        format: u32,
        usage: u32,
    ) -> u64 {
        let id = self.gen_id();
        let mut img = VkImage::new(id, width, height, depth, mip_levels, array_layers, format, usage);
        img.create_wgpu_texture(&self.device);
        self.images.insert(id, img);
        id
    }

    pub fn vk_create_image_view(&mut self, image_id: u64, format: u32) -> u64 {
        let id = self.gen_id();
        let mut view = VkImageView::new(id, image_id, format);
        if let Some(img) = self.images.get(&image_id) {
            view.create_wgpu_view(img);
        }
        self.image_views.insert(id, view);
        id
    }

    pub fn vk_create_sampler(
        &mut self,
        mag_filter: u32,
        min_filter: u32,
        address_mode_u: u32,
        address_mode_v: u32,
        address_mode_w: u32,
    ) -> u64 {
        let id = self.gen_id();
        let mut samp = VkSampler::new(id, mag_filter, min_filter, address_mode_u, address_mode_v, address_mode_w);
        samp.create_wgpu_sampler(&self.device);
        self.samplers.insert(id, samp);
        id
    }

    pub fn vk_create_descriptor_set_layout(
        &mut self,
        bindings: Vec<crate::descriptor::VkDescriptorSetLayoutBinding>,
    ) -> u64 {
        let id = self.gen_id();
        let mut layout = VkDescriptorSetLayout::new(id, bindings);
        layout.create_wgpu_layout(&self.device);
        self.descriptor_set_layouts.insert(id, layout);
        id
    }

    pub fn vk_create_descriptor_set(&mut self, layout_id: u64) -> u64 {
        let id = self.gen_id();
        let set = VkDescriptorSet::new(id, layout_id);
        self.descriptor_sets.insert(id, set);
        id
    }

    pub fn vk_create_pipeline_layout(
        &mut self,
        set_layout_ids: Vec<u64>,
        push_constant_ranges: Vec<VkPushConstantRange>,
    ) -> u64 {
        let id = self.gen_id();
        let mut layout = VkPipelineLayout::new(id, set_layout_ids, push_constant_ranges);
        layout.create_wgpu_layout(&self.device, &self.descriptor_set_layouts, Some(&self.push_constant_bgl));
        self.pipeline_layouts.insert(id, layout);
        id
    }

    pub fn vk_create_graphics_pipeline(
        &mut self,
        info: &VkGraphicsPipelineCreateInfo,
    ) -> Result<u64, String> {
        let id = self.gen_id();

        let vs_module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&format!("VkVS_{}", id)),
            source: wgpu::ShaderSource::Wgsl(info.vertex_shader_wgsl.clone().into()),
        });

        let fs_module = if let Some(fs_wgsl) = &info.fragment_shader_wgsl {
            Some(self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("VkFS_{}", id)),
                source: wgpu::ShaderSource::Wgsl(fs_wgsl.clone().into()),
            }))
        } else {
            None
        };

        let pipe_layout = self
            .pipeline_layouts
            .get(&info.layout_id)
            .and_then(|l| l.wgpu_layout.as_ref())
            .ok_or_else(|| "Invalid or missing pipeline layout".to_string())?;

        // Construct Vertex Buffer Layouts
        let mut attr_storage = [wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        }; 16];

        let num_attrs = info.vertex_attributes.len().min(16);
        for (i, attr) in info.vertex_attributes.iter().take(num_attrs).enumerate() {
            let format = match attr.format {
                VK_FORMAT_R32_SFLOAT => wgpu::VertexFormat::Float32,
                VK_FORMAT_R32G32_SFLOAT => wgpu::VertexFormat::Float32x2,
                VK_FORMAT_R32G32B32_SFLOAT => wgpu::VertexFormat::Float32x3,
                VK_FORMAT_R32G32B32A32_SFLOAT => wgpu::VertexFormat::Float32x4,
                VK_FORMAT_R8G8B8A8_UNORM => wgpu::VertexFormat::Unorm8x4,
                _ => wgpu::VertexFormat::Float32x3,
            };
            attr_storage[i] = wgpu::VertexAttribute {
                offset: attr.offset as u64,
                shader_location: attr.location,
                format,
            };
        }

        let mut vbls = Vec::new();
        for binding in &info.vertex_bindings {
            let step_mode = if binding.input_rate == 1 {
                wgpu::VertexStepMode::Instance
            } else {
                wgpu::VertexStepMode::Vertex
            };

            let bound_attrs = &attr_storage[0..num_attrs];
            vbls.push(wgpu::VertexBufferLayout {
                array_stride: binding.stride as u64,
                step_mode,
                attributes: bound_attrs,
            });
        }

        let targets: Vec<Option<wgpu::ColorTargetState>> = info
            .color_formats
            .iter()
            .map(|&fmt| {
                let w_fmt = vk_format_to_wgpu(fmt).unwrap_or(wgpu::TextureFormat::Rgba8Unorm);
                Some(wgpu::ColorTargetState {
                    format: w_fmt,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect();

        let topology = match info.topology {
            VK_PRIMITIVE_TOPOLOGY_LINE_LIST => wgpu::PrimitiveTopology::LineList,
            VK_PRIMITIVE_TOPOLOGY_LINE_STRIP => wgpu::PrimitiveTopology::LineStrip,
            VK_PRIMITIVE_TOPOLOGY_TRIANGLE_STRIP => wgpu::PrimitiveTopology::TriangleStrip,
            _ => wgpu::PrimitiveTopology::TriangleList,
        };

        let depth_stencil = info.depth_format.map(|fmt| {
            let w_fmt = vk_format_to_wgpu(fmt).unwrap_or(wgpu::TextureFormat::Depth24PlusStencil8);
            wgpu::DepthStencilState {
                format: w_fmt,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }
        });

        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&format!("VkRenderPipeline_{}", id)),
            layout: Some(pipe_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: Some("main"),
                buffers: &vbls,
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState::default(),
            fragment: fs_module.as_ref().map(|fs| wgpu::FragmentState {
                module: fs,
                entry_point: Some("main"),
                targets: &targets,
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });

        let vk_pipe = VkPipeline::new_graphics(id, info.layout_id, pipeline);
        self.pipelines.insert(id, vk_pipe);
        Ok(id)
    }

    pub fn vk_create_command_buffer(&mut self) -> u64 {
        let id = self.gen_id();
        let cb = VkCommandBuffer::new(id);
        self.command_buffers.insert(id, cb);
        id
    }

    pub fn flush_dirty_memories(&mut self) {
        for mem in self.memories.values_mut() {
            if !mem.dirty_ranges.is_empty() {
                if let Some(buf_id) = mem.bound_buffer_id {
                    if let Some(buf) = self.buffers.get(&buf_id) {
                        if let Some(w_buf) = &buf.wgpu_buffer {
                            for range in &mem.dirty_ranges {
                                let start = range.offset as usize;
                                let end = (range.offset + range.size) as usize;
                                let dst_offset = buf.memory_offset + range.offset;
                                if end <= mem.shadow_buffer.len() && dst_offset + range.size <= buf.size {
                                    self.queue.write_buffer(w_buf, dst_offset, &mem.shadow_buffer[start..end]);
                                }
                            }
                        }
                    }
                }
                mem.dirty_ranges.clear();
            }
        }
    }

    pub fn vk_queue_submit(&mut self, command_buffer_ids: &[u64]) {
        self.flush_dirty_memories();

        // Prepare descriptor bind groups upfront
        for set in self.descriptor_sets.values_mut() {
            set.get_or_create_bind_group(
                &self.device,
                &self.descriptor_set_layouts,
                &self.buffers,
                &self.image_views,
                &self.samplers,
            );
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("VkQueueSubmit_Encoder"),
        });

        for &cb_id in command_buffer_ids {
            let cb_commands = match self.command_buffers.get(&cb_id) {
                Some(cb) => cb.commands.clone(),
                None => continue,
            };

            let mut i = 0;
            while i < cb_commands.len() {
                match &cb_commands[i] {
                    VkCommand::PushConstants { offset, data } => {
                        let dst_offset = (*offset as u64).min(1024 - data.len() as u64);
                        self.queue.write_buffer(&self.push_constant_buffer, dst_offset, data);
                        i += 1;
                    }
                    VkCommand::CopyImageToBuffer { image_id, buffer_id, width, height } => {
                        if let (Some(img), Some(buf)) = (self.images.get(image_id), self.buffers.get(buffer_id)) {
                            if let (Some(w_tex), Some(w_buf)) = (&img.wgpu_texture, &buf.wgpu_buffer) {
                                let bytes_per_row = (*width * 4 + 255) & !255;
                                encoder.copy_texture_to_buffer(
                                    wgpu::TexelCopyTextureInfo {
                                        texture: w_tex,
                                        mip_level: 0,
                                        origin: wgpu::Origin3d::ZERO,
                                        aspect: wgpu::TextureAspect::All,
                                    },
                                    wgpu::TexelCopyBufferInfo {
                                        buffer: w_buf,
                                        layout: wgpu::TexelCopyBufferLayout {
                                            offset: 0,
                                            bytes_per_row: Some(bytes_per_row),
                                            rows_per_image: Some(*height),
                                        },
                                    },
                                    wgpu::Extent3d {
                                        width: *width,
                                        height: *height,
                                        depth_or_array_layers: 1,
                                    },
                                );
                            }
                        }
                        i += 1;
                    }
                    VkCommand::BeginRendering { color_attachments, depth_attachment } => {
                        let mut w_color_atts = Vec::new();
                        for att in color_attachments {
                            if let Some(view) = self.image_views.get(&att.image_view_id) {
                                if let Some(w_view) = &view.wgpu_view {
                                    let clear_col = if let Some(VkClearValue::Color(c)) = &att.clear_value {
                                        wgpu::Color {
                                            r: c.float32[0] as f64,
                                            g: c.float32[1] as f64,
                                            b: c.float32[2] as f64,
                                            a: c.float32[3] as f64,
                                        }
                                    } else {
                                        wgpu::Color::BLACK
                                    };

                                    let load_op = if att.load_op == 1 {
                                        wgpu::LoadOp::Clear(clear_col)
                                    } else {
                                        wgpu::LoadOp::Load
                                    };

                                    w_color_atts.push(Some(wgpu::RenderPassColorAttachment {
                                        view: w_view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: load_op,
                                            store: wgpu::StoreOp::Store,
                                        },
                                    }));
                                }
                            }
                        }

                        let w_depth_att = depth_attachment.as_ref().and_then(|d_att| {
                            let view = self.image_views.get(&d_att.image_view_id)?;
                            let w_view = view.wgpu_view.as_ref()?;
                            let (depth_clear, stencil_clear) = if let Some(VkClearValue::DepthStencil(ds)) = &d_att.clear_value {
                                (ds.depth, ds.stencil)
                            } else {
                                (1.0, 0)
                            };

                            let depth_ops = if d_att.load_op == 1 {
                                wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(depth_clear),
                                    store: wgpu::StoreOp::Store,
                                }
                            } else {
                                wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                }
                            };

                            Some(wgpu::RenderPassDepthStencilAttachment {
                                view: w_view,
                                depth_ops: Some(depth_ops),
                                stencil_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(stencil_clear),
                                    store: wgpu::StoreOp::Store,
                                }),
                            })
                        });

                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("VkRenderPass"),
                            color_attachments: &w_color_atts,
                            depth_stencil_attachment: w_depth_att,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });

                        // Always bind push constants on group 3 by default
                        rpass.set_bind_group(3, &self.push_constant_bg, &[]);

                        i += 1;
                        while i < cb_commands.len() {
                            match &cb_commands[i] {
                                VkCommand::EndRendering => {
                                    i += 1;
                                    break;
                                }
                                VkCommand::BindPipeline { pipeline_id } => {
                                    if let Some(pipe) = self.pipelines.get(pipeline_id) {
                                        if let VkPipelineType::Graphics(w_pipe) = &pipe.inner {
                                            rpass.set_pipeline(w_pipe);
                                        }
                                    }
                                    i += 1;
                                }
                                VkCommand::BindDescriptorSets { first_set, descriptor_set_ids, dynamic_offsets } => {
                                    for (idx, &ds_id) in descriptor_set_ids.iter().enumerate() {
                                        let set_num = *first_set + idx as u32;
                                        if let Some(ds) = self.descriptor_sets.get(&ds_id) {
                                            if let Some(bg) = &ds.cached_bind_group {
                                                let offsets: Vec<wgpu::DynamicOffset> = dynamic_offsets.iter().map(|&o| o as wgpu::DynamicOffset).collect();
                                                rpass.set_bind_group(set_num, bg, &offsets);
                                            }
                                        }
                                    }
                                    i += 1;
                                }
                                VkCommand::BindVertexBuffers { first_binding, buffer_ids, offsets } => {
                                    for (idx, (&b_id, &off)) in buffer_ids.iter().zip(offsets.iter()).enumerate() {
                                        let slot = *first_binding + idx as u32;
                                        if let Some(buf) = self.buffers.get(&b_id) {
                                            if let Some(w_buf) = &buf.wgpu_buffer {
                                                rpass.set_vertex_buffer(slot, w_buf.slice(off..));
                                            }
                                        }
                                    }
                                    i += 1;
                                }
                                VkCommand::BindIndexBuffer { buffer_id, offset, index_type } => {
                                    if let Some(buf) = self.buffers.get(buffer_id) {
                                        if let Some(w_buf) = &buf.wgpu_buffer {
                                            let format = if *index_type == 0 {
                                                wgpu::IndexFormat::Uint16
                                            } else {
                                                wgpu::IndexFormat::Uint32
                                            };
                                            rpass.set_index_buffer(w_buf.slice(*offset..), format);
                                        }
                                    }
                                    i += 1;
                                }
                                VkCommand::SetViewport { x, y, width, height, min_depth, max_depth } => {
                                    rpass.set_viewport(*x, *y, *width, *height, *min_depth, *max_depth);
                                    i += 1;
                                }
                                VkCommand::SetScissor { x, y, width, height } => {
                                    rpass.set_scissor_rect(*x as u32, *y as u32, *width, *height);
                                    i += 1;
                                }
                                VkCommand::Draw { vertex_count, instance_count, first_vertex, first_instance } => {
                                    rpass.draw(
                                        *first_vertex..*first_vertex + *vertex_count,
                                        *first_instance..*first_instance + *instance_count,
                                    );
                                    i += 1;
                                }
                                VkCommand::DrawIndexed { index_count, instance_count, first_index, vertex_offset, first_instance } => {
                                    rpass.draw_indexed(
                                        *first_index..*first_index + *index_count,
                                        *vertex_offset,
                                        *first_instance..*first_instance + *instance_count,
                                    );
                                    i += 1;
                                }
                                _ => {
                                    i += 1;
                                }
                            }
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }
}
