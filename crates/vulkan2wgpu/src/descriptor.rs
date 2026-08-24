use crate::buffer::VkBuffer;
use crate::image::{VkImageView, VkSampler};
use crate::types::*;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VkDescriptorSetLayoutBinding {
    pub binding: u32,
    pub descriptor_type: u32,
    pub descriptor_count: u32,
    pub stage_flags: u32,
}

pub struct VkDescriptorSetLayout {
    pub id: u64,
    pub bindings: Vec<VkDescriptorSetLayoutBinding>,
    pub wgpu_layout: Option<wgpu::BindGroupLayout>,
}

impl VkDescriptorSetLayout {
    pub fn new(id: u64, bindings: Vec<VkDescriptorSetLayoutBinding>) -> Self {
        Self {
            id,
            bindings,
            wgpu_layout: None,
        }
    }

    pub fn create_wgpu_layout(&mut self, device: &wgpu::Device) {
        let mut entries = Vec::new();
        for b in &self.bindings {
            let visibility = wgpu::ShaderStages::all();
            let ty = match b.descriptor_type {
                VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER | VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER_DYNAMIC => {
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: b.descriptor_type == VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER_DYNAMIC,
                        min_binding_size: None,
                    }
                }
                VK_DESCRIPTOR_TYPE_STORAGE_BUFFER | VK_DESCRIPTOR_TYPE_STORAGE_BUFFER_DYNAMIC => {
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: b.descriptor_type == VK_DESCRIPTOR_TYPE_STORAGE_BUFFER_DYNAMIC,
                        min_binding_size: None,
                    }
                }
                VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE | VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER => {
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }
                }
                VK_DESCRIPTOR_TYPE_SAMPLER => wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                _ => wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            };

            entries.push(wgpu::BindGroupLayoutEntry {
                binding: b.binding,
                visibility,
                ty,
                count: None,
            });
        }

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(&format!("VkDescriptorSetLayout_{}", self.id)),
            entries: &entries,
        });

        self.wgpu_layout = Some(layout);
    }
}

#[derive(Clone)]
pub enum VkDescriptorResource {
    Buffer { buffer_id: u64, offset: u64, size: u64 },
    ImageView { view_id: u64 },
    Sampler { sampler_id: u64 },
}

pub struct VkDescriptorSet {
    pub id: u64,
    pub layout_id: u64,
    pub bound_resources: HashMap<u32, VkDescriptorResource>,
    pub is_dirty: bool,
    pub cached_bind_group: Option<wgpu::BindGroup>,
}

impl VkDescriptorSet {
    pub fn new(id: u64, layout_id: u64) -> Self {
        Self {
            id,
            layout_id,
            bound_resources: HashMap::new(),
            is_dirty: true,
            cached_bind_group: None,
        }
    }

    pub fn write_buffer(&mut self, binding: u32, buffer_id: u64, offset: u64, size: u64) {
        self.bound_resources.insert(binding, VkDescriptorResource::Buffer { buffer_id, offset, size });
        self.is_dirty = true;
        self.cached_bind_group = None;
    }

    pub fn write_image_view(&mut self, binding: u32, view_id: u64) {
        self.bound_resources.insert(binding, VkDescriptorResource::ImageView { view_id });
        self.is_dirty = true;
        self.cached_bind_group = None;
    }

    pub fn write_sampler(&mut self, binding: u32, sampler_id: u64) {
        self.bound_resources.insert(binding, VkDescriptorResource::Sampler { sampler_id });
        self.is_dirty = true;
        self.cached_bind_group = None;
    }

    pub fn get_or_create_bind_group(
        &mut self,
        device: &wgpu::Device,
        layouts: &HashMap<u64, VkDescriptorSetLayout>,
        buffers: &HashMap<u64, VkBuffer>,
        views: &HashMap<u64, VkImageView>,
        samplers: &HashMap<u64, VkSampler>,
    ) -> Option<&wgpu::BindGroup> {
        if !self.is_dirty && self.cached_bind_group.is_some() {
            return self.cached_bind_group.as_ref();
        }

        let layout = layouts.get(&self.layout_id)?;
        let w_bgl = layout.wgpu_layout.as_ref()?;

        let mut entries = Vec::new();
        for (&binding, res) in &self.bound_resources {
            match res {
                VkDescriptorResource::Buffer { buffer_id, offset, size } => {
                    if let Some(buf) = buffers.get(buffer_id) {
                        if let Some(w_buf) = &buf.wgpu_buffer {
                            let bound_size = if *size > 0 {
                                std::num::NonZeroU64::new(*size)
                            } else {
                                None
                            };
                            entries.push(wgpu::BindGroupEntry {
                                binding,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: w_buf,
                                    offset: *offset,
                                    size: bound_size,
                                }),
                            });
                        }
                    }
                }
                VkDescriptorResource::ImageView { view_id } => {
                    if let Some(view) = views.get(view_id) {
                        if let Some(w_view) = &view.wgpu_view {
                            entries.push(wgpu::BindGroupEntry {
                                binding,
                                resource: wgpu::BindingResource::TextureView(w_view),
                            });
                        }
                    }
                }
                VkDescriptorResource::Sampler { sampler_id } => {
                    if let Some(samp) = samplers.get(sampler_id) {
                        if let Some(w_samp) = &samp.wgpu_sampler {
                            entries.push(wgpu::BindGroupEntry {
                                binding,
                                resource: wgpu::BindingResource::Sampler(w_samp),
                            });
                        }
                    }
                }
            }
        }

        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("VkBindGroup_{}", self.id)),
            layout: w_bgl,
            entries: &entries,
        });

        self.cached_bind_group = Some(bg);
        self.is_dirty = false;
        self.cached_bind_group.as_ref()
    }
}
