use crate::descriptor::VkDescriptorSetLayout;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VkPushConstantRange {
    pub stage_flags: u32,
    pub offset: u32,
    pub size: u32,
}

pub struct VkPipelineLayout {
    pub id: u64,
    pub set_layout_ids: Vec<u64>,
    pub push_constant_ranges: Vec<VkPushConstantRange>,
    pub wgpu_layout: Option<wgpu::PipelineLayout>,
}

impl VkPipelineLayout {
    pub fn new(
        id: u64,
        set_layout_ids: Vec<u64>,
        push_constant_ranges: Vec<VkPushConstantRange>,
    ) -> Self {
        Self {
            id,
            set_layout_ids,
            push_constant_ranges,
            wgpu_layout: None,
        }
    }

    pub fn create_wgpu_layout(
        &mut self,
        device: &wgpu::Device,
        layouts: &HashMap<u64, VkDescriptorSetLayout>,
        push_constant_bgl: Option<&wgpu::BindGroupLayout>,
    ) {
        let mut bgl_refs = Vec::new();
        for id in &self.set_layout_ids {
            if let Some(layout) = layouts.get(id) {
                if let Some(w_bgl) = &layout.wgpu_layout {
                    bgl_refs.push(w_bgl);
                }
            }
        }

        if let Some(pc_bgl) = push_constant_bgl {
            if !self.push_constant_ranges.is_empty() {
                bgl_refs.push(pc_bgl);
            }
        }

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("VkPipelineLayout_{}", self.id)),
            bind_group_layouts: &bgl_refs,
            push_constant_ranges: &[],
        });

        self.wgpu_layout = Some(layout);
    }
}

pub enum VkPipelineType {
    Graphics(wgpu::RenderPipeline),
    Compute(wgpu::ComputePipeline),
}

pub struct VkPipeline {
    pub id: u64,
    pub layout_id: u64,
    pub inner: VkPipelineType,
}

impl VkPipeline {
    pub fn new_graphics(id: u64, layout_id: u64, pipeline: wgpu::RenderPipeline) -> Self {
        Self {
            id,
            layout_id,
            inner: VkPipelineType::Graphics(pipeline),
        }
    }

    pub fn new_compute(id: u64, layout_id: u64, pipeline: wgpu::ComputePipeline) -> Self {
        Self {
            id,
            layout_id,
            inner: VkPipelineType::Compute(pipeline),
        }
    }
}
