use crate::buffer::{VkBuffer, VkDeviceMemory};
use crate::command::*;
use crate::descriptor::{VkDescriptorSet, VkDescriptorSetLayout};
use crate::image::{VkImage, VkImageView, VkSampler};
use crate::pipeline::{VkPipeline, VkPipelineLayout, VkPipelineType};
use crate::spirv::SpirvTranslator;
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
            size: 256,
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
                                if end <= mem.shadow_buffer.len() {
                                    self.queue.write_buffer(w_buf, range.offset, &mem.shadow_buffer[start..end]);
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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("VkQueueSubmit_Encoder"),
        });

        for &cb_id in command_buffer_ids {
            let cb = match self.command_buffers.get(&cb_id) {
                Some(cb) => cb,
                None => continue,
            };

            let mut active_pipeline: Option<u64> = None;
            let mut bound_vertex_buffers: Vec<(u32, u64, u64)> = Vec::new();
            let mut bound_index_buffer: Option<(u64, u64, u32)> = None;

            // Simple pass dispatch loop
            for cmd in &cb.commands {
                match cmd {
                    VkCommand::PushConstants { offset, data } => {
                        self.queue.write_buffer(&self.push_constant_buffer, *offset as u64, data);
                    }
                    VkCommand::BindPipeline { pipeline_id } => {
                        active_pipeline = Some(*pipeline_id);
                    }
                    VkCommand::BindVertexBuffers { first_binding, buffer_ids, offsets } => {
                        for (i, (&buf_id, &off)) in buffer_ids.iter().zip(offsets.iter()).enumerate() {
                            bound_vertex_buffers.push((*first_binding + i as u32, buf_id, off));
                        }
                    }
                    VkCommand::BindIndexBuffer { buffer_id, offset, index_type } => {
                        bound_index_buffer = Some((*buffer_id, *offset, *index_type));
                    }
                    VkCommand::BeginRendering { color_attachments, .. } => {
                        // Build color attachments
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

                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("VkRenderPass"),
                            color_attachments: &w_color_atts,
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });

                        if let Some(p_id) = active_pipeline {
                            if let Some(pipeline) = self.pipelines.get(&p_id) {
                                if let VkPipelineType::Graphics(w_pipe) = &pipeline.inner {
                                    rpass.set_pipeline(w_pipe);
                                    rpass.set_bind_group(3, &self.push_constant_bg, &[]);

                                    for (slot, b_id, off) in &bound_vertex_buffers {
                                        if let Some(buf) = self.buffers.get(b_id) {
                                            if let Some(w_buf) = &buf.wgpu_buffer {
                                                rpass.set_vertex_buffer(*slot, w_buf.slice(*off..));
                                            }
                                        }
                                    }

                                    if let Some((b_id, off, idx_type)) = bound_index_buffer {
                                        if let Some(buf) = self.buffers.get(&b_id) {
                                            if let Some(w_buf) = &buf.wgpu_buffer {
                                                let format = if idx_type == 0 {
                                                    wgpu::IndexFormat::Uint16
                                                } else {
                                                    wgpu::IndexFormat::Uint32
                                                };
                                                rpass.set_index_buffer(w_buf.slice(off..), format);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    VkCommand::Draw { .. } => {}
                    VkCommand::EndRendering => {}
                    _ => {}
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }
}
