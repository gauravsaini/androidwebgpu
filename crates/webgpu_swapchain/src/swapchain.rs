pub enum SwapchainTarget {
    Offscreen,
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
}

pub struct WebGpuSwapchain {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub present_texture: wgpu::Texture,
    pub present_view: wgpu::TextureView,
    pub frame_count: u64,
    pub target: SwapchainTarget,
}

impl WebGpuSwapchain {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let (present_texture, present_view) = Self::create_textures(device, width, height, format);
        Self {
            width,
            height,
            format,
            present_texture,
            present_view,
            frame_count: 0,
            target: SwapchainTarget::Offscreen,
        }
    }

    pub fn new_with_surface(
        surface: wgpu::Surface<'static>,
        device: &wgpu::Device,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
        surface.configure(device, &config);
        let (present_texture, present_view) =
            Self::create_textures(device, config.width, config.height, config.format);
        Self {
            width: config.width,
            height: config.height,
            format: config.format,
            present_texture,
            present_view,
            frame_count: 0,
            target: SwapchainTarget::Surface { surface, config },
        }
    }

    fn create_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("WebGPU Swapchain Present Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            let (tex, view) = Self::create_textures(device, width, height, self.format);
            self.present_texture = tex;
            self.present_view = view;

            if let SwapchainTarget::Surface { surface, config } = &mut self.target {
                config.width = width.max(1);
                config.height = height.max(1);
                surface.configure(device, config);
            }
        }
    }

    pub fn get_current_texture_view(&self) -> &wgpu::TextureView {
        &self.present_view
    }

    pub fn is_surface(&self) -> bool {
        matches!(self.target, SwapchainTarget::Surface { .. })
    }

    pub fn present_surface(&mut self) -> Result<u64, wgpu::SurfaceError> {
        self.frame_count += 1;
        if let SwapchainTarget::Surface { surface, .. } = &self.target {
            let output = surface.get_current_texture()?;
            output.present();
        }
        Ok(self.frame_count)
    }

    pub fn present(&mut self) -> u64 {
        self.frame_count += 1;
        if let SwapchainTarget::Surface { surface, .. } = &self.target {
            if let Ok(output) = surface.get_current_texture() {
                output.present();
            }
        }
        self.frame_count
    }

    pub async fn readback_pixels(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<u8>, String> {
        let u32_size = std::mem::size_of::<u32>() as u32;
        let bytes_per_row = (u32_size * self.width + 255) & !255;
        let buffer_size = (bytes_per_row * self.height) as u64;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Swapchain Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Swapchain Readback Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.present_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            tx.send(res).unwrap();
        });

        device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("Channel error: {:?}", e))?
            .map_err(|e| format!("Buffer map error: {:?}", e))?;

        let data = buffer_slice.get_mapped_range().to_vec();
        Ok(data)
    }
}
