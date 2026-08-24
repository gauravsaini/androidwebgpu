use crate::types::*;

pub struct VkImage {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub mip_levels: u32,
    pub array_layers: u32,
    pub format: u32,
    pub usage: u32,
    pub wgpu_texture: Option<wgpu::Texture>,
}

impl VkImage {
    pub fn new(
        id: u64,
        width: u32,
        height: u32,
        depth: u32,
        mip_levels: u32,
        array_layers: u32,
        format: u32,
        usage: u32,
    ) -> Self {
        Self {
            id,
            width,
            height,
            depth,
            mip_levels,
            array_layers,
            format,
            usage,
            wgpu_texture: None,
        }
    }

    pub fn create_wgpu_texture(&mut self, device: &wgpu::Device) {
        let w_format = vk_format_to_wgpu(self.format).unwrap_or(wgpu::TextureFormat::Rgba8Unorm);
        let mut w_usage = wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST;

        if (self.usage & VK_IMAGE_USAGE_SAMPLED_BIT) != 0 {
            w_usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        }
        if (self.usage & VK_IMAGE_USAGE_STORAGE_BIT) != 0 {
            w_usage |= wgpu::TextureUsages::STORAGE_BINDING;
        }
        if (self.usage & (VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_DEPTH_STENCIL_ATTACHMENT_BIT)) != 0 {
            w_usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }

        let (dimension, depth_or_array_layers) = if self.depth > 1 {
            (wgpu::TextureDimension::D3, self.depth)
        } else if self.height > 1 || self.array_layers > 1 {
            (wgpu::TextureDimension::D2, self.array_layers.max(1))
        } else {
            (wgpu::TextureDimension::D1, 1)
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("VkImage_{}", self.id)),
            size: wgpu::Extent3d {
                width: self.width.max(1),
                height: self.height.max(1),
                depth_or_array_layers,
            },
            mip_level_count: self.mip_levels.max(1),
            sample_count: 1,
            dimension,
            format: w_format,
            usage: w_usage,
            view_formats: &[],
        });

        self.wgpu_texture = Some(texture);
    }
}

pub struct VkImageView {
    pub id: u64,
    pub image_id: u64,
    pub format: u32,
    pub wgpu_view: Option<wgpu::TextureView>,
}

impl VkImageView {
    pub fn new(id: u64, image_id: u64, format: u32) -> Self {
        Self {
            id,
            image_id,
            format,
            wgpu_view: None,
        }
    }

    pub fn create_wgpu_view(&mut self, image: &VkImage) {
        if let Some(tex) = &image.wgpu_texture {
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            self.wgpu_view = Some(view);
        }
    }
}

pub struct VkSampler {
    pub id: u64,
    pub mag_filter: u32,
    pub min_filter: u32,
    pub address_mode_u: u32,
    pub address_mode_v: u32,
    pub address_mode_w: u32,
    pub wgpu_sampler: Option<wgpu::Sampler>,
}

impl VkSampler {
    pub fn new(
        id: u64,
        mag_filter: u32,
        min_filter: u32,
        address_mode_u: u32,
        address_mode_v: u32,
        address_mode_w: u32,
    ) -> Self {
        Self {
            id,
            mag_filter,
            min_filter,
            address_mode_u,
            address_mode_v,
            address_mode_w,
            wgpu_sampler: None,
        }
    }

    pub fn create_wgpu_sampler(&mut self, device: &wgpu::Device) {
        let mag = if self.mag_filter == 1 {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        };
        let min = if self.min_filter == 1 {
            wgpu::FilterMode::Linear
        } else {
            wgpu::FilterMode::Nearest
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("VkSampler_{}", self.id)),
            address_mode_u: vk_address_mode_to_wgpu(self.address_mode_u),
            address_mode_v: vk_address_mode_to_wgpu(self.address_mode_v),
            address_mode_w: vk_address_mode_to_wgpu(self.address_mode_w),
            mag_filter: mag,
            min_filter: min,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        self.wgpu_sampler = Some(sampler);
    }
}
