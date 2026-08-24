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
                VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE => wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
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
    }

    pub fn write_image_view(&mut self, binding: u32, view_id: u64) {
        self.bound_resources.insert(binding, VkDescriptorResource::ImageView { view_id });
        self.is_dirty = true;
    }

    pub fn write_sampler(&mut self, binding: u32, sampler_id: u64) {
        self.bound_resources.insert(binding, VkDescriptorResource::Sampler { sampler_id });
        self.is_dirty = true;
    }
}
