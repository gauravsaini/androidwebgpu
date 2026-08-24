pub struct GlTexture {
    pub id: u32,
    pub target: u32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub wgpu_texture: Option<wgpu::Texture>,
    pub wgpu_view: Option<wgpu::TextureView>,
    pub wgpu_sampler: Option<wgpu::Sampler>,
    pub min_filter: u32,
    pub mag_filter: u32,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub dirty: bool,
}

impl GlTexture {
    pub fn new(id: u32, target: u32) -> Self {
        Self {
            id,
            target,
            width: 0,
            height: 0,
            format: 0x1908, // GL_RGBA
            wgpu_texture: None,
            wgpu_view: None,
            wgpu_sampler: None,
            min_filter: 0x2601, // GL_LINEAR
            mag_filter: 0x2601, // GL_LINEAR
            wrap_s: 0x812F,     // GL_CLAMP_TO_EDGE
            wrap_t: 0x812F,     // GL_CLAMP_TO_EDGE
            dirty: false,
        }
    }

    pub fn update_sampler(&mut self, device: &wgpu::Device) {
        let mag_filter = match self.mag_filter {
            0x2600 => wgpu::FilterMode::Nearest, // GL_NEAREST
            _ => wgpu::FilterMode::Linear,        // GL_LINEAR
        };

        let min_filter = match self.min_filter {
            0x2600 | 0x2700 | 0x2702 => wgpu::FilterMode::Nearest,
            _ => wgpu::FilterMode::Linear,
        };

        let mipmap_filter = match self.min_filter {
            0x2701 | 0x2703 => wgpu::FilterMode::Linear,
            _ => wgpu::FilterMode::Nearest,
        };

        let address_mode_u = match self.wrap_s {
            0x2901 => wgpu::AddressMode::Repeat,         // GL_REPEAT
            0x8370 => wgpu::AddressMode::MirrorRepeat,   // GL_MIRRORED_REPEAT
            _ => wgpu::AddressMode::ClampToEdge,        // GL_CLAMP_TO_EDGE
        };

        let address_mode_v = match self.wrap_t {
            0x2901 => wgpu::AddressMode::Repeat,
            0x8370 => wgpu::AddressMode::MirrorRepeat,
            _ => wgpu::AddressMode::ClampToEdge,
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("GL_Sampler_{}", self.id)),
            address_mode_u,
            address_mode_v,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter,
            min_filter,
            mipmap_filter,
            ..Default::default()
        });

        self.wgpu_sampler = Some(sampler);
    }

    pub fn allocate_2d(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        self.width = width;
        self.height = height;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("GL_Texture_{}", self.id)),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.wgpu_texture = Some(texture);
        self.wgpu_view = Some(view);
        self.update_sampler(device);
        self.dirty = false;
    }

    pub fn upload_image_data(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        if self.wgpu_texture.is_none() || self.width != width || self.height != height {
            self.allocate_2d(device, width, height, wgpu::TextureFormat::Rgba8UnormSrgb);
        }

        if let Some(texture) = &self.wgpu_texture {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}
