use std::time::Instant;

pub enum SwapchainTarget {
    Offscreen {
        buffers: Vec<wgpu::Texture>,
        views: Vec<wgpu::TextureView>,
        current_idx: usize,
    },
    Surface {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct FrameTimingStats {
    pub frame_count: u64,
    pub fps: f32,
    pub frame_time_ms: f32,
    pub gpu_time_ms: f32,
    pub target_fps: f32,
    pub is_120fps_capable: bool,
}

pub struct WebGpuSwapchain {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub present_texture: wgpu::Texture,
    pub present_view: wgpu::TextureView,
    pub frame_count: u64,
    pub target: SwapchainTarget,
    pub present_mode: wgpu::PresentMode,
    pub target_fps: f32,
    pub last_frame_time: Option<Instant>,
    pub last_frame_duration_ms: f32,
    pub last_gpu_duration_ms: f32,
    pub query_set: Option<wgpu::QuerySet>,
    pub query_buffer: Option<wgpu::Buffer>,
    pub query_staging_buffer: Option<wgpu::Buffer>,
}

impl WebGpuSwapchain {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self::new_with_options(device, width, height, format, wgpu::PresentMode::Mailbox, 120.0)
    }

    pub fn new_with_options(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        present_mode: wgpu::PresentMode,
        target_fps: f32,
    ) -> Self {
        // Triple buffering for offscreen 120fps mailbox parity
        let mut buffers = Vec::with_capacity(3);
        let mut views = Vec::with_capacity(3);
        for _ in 0..3 {
            let (t, v) = Self::create_textures(device, width, height, format);
            buffers.push(t);
            views.push(v);
        }

        let present_texture = Self::create_textures(device, width, height, format).0;
        let present_view = present_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (query_set, query_buffer, query_staging_buffer) = Self::create_query_resources(device);

        Self {
            width,
            height,
            format,
            present_texture,
            present_view,
            frame_count: 0,
            target: SwapchainTarget::Offscreen {
                buffers,
                views,
                current_idx: 0,
            },
            present_mode,
            target_fps,
            last_frame_time: None,
            last_frame_duration_ms: 16.6,
            last_gpu_duration_ms: 0.0,
            query_set,
            query_buffer,
            query_staging_buffer,
        }
    }

    pub fn new_with_surface(
        surface: wgpu::Surface<'static>,
        device: &wgpu::Device,
        mut config: wgpu::SurfaceConfiguration,
    ) -> Self {
        // Enforce Mailbox present mode for 120fps low-latency tear-free VSync
        config.present_mode = wgpu::PresentMode::Mailbox;
        surface.configure(device, &config);

        let (present_texture, present_view) =
            Self::create_textures(device, config.width, config.height, config.format);

        let (query_set, query_buffer, query_staging_buffer) = Self::create_query_resources(device);

        Self {
            width: config.width,
            height: config.height,
            format: config.format,
            present_texture,
            present_view,
            frame_count: 0,
            target: SwapchainTarget::Surface { surface, config },
            present_mode: wgpu::PresentMode::Mailbox,
            target_fps: 120.0,
            last_frame_time: None,
            last_frame_duration_ms: 16.6,
            last_gpu_duration_ms: 0.0,
            query_set,
            query_buffer,
            query_staging_buffer,
        }
    }

    fn create_query_resources(
        device: &wgpu::Device,
    ) -> (Option<wgpu::QuerySet>, Option<wgpu::Buffer>, Option<wgpu::Buffer>) {
        if device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Swapchain Timestamp Query Set"),
                count: 2,
                ty: wgpu::QueryType::Timestamp,
            });
            let query_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Swapchain Timestamp Buffer"),
                size: 16, // 2 * u64
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Swapchain Timestamp Staging Buffer"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            (Some(query_set), Some(query_buffer), Some(staging_buffer))
        } else {
            (None, None, None)
        }
    }

    fn create_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("WebGPU Swapchain Texture"),
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

            match &mut self.target {
                SwapchainTarget::Surface { surface, config } => {
                    config.width = width.max(1);
                    config.height = height.max(1);
                    surface.configure(device, config);
                }
                SwapchainTarget::Offscreen { buffers, views, current_idx } => {
                    buffers.clear();
                    views.clear();
                    for _ in 0..3 {
                        let (t, v) = Self::create_textures(device, width, height, self.format);
                        buffers.push(t);
                        views.push(v);
                    }
                    *current_idx = 0;
                }
            }
        }
    }

    pub fn get_current_texture_view(&self) -> &wgpu::TextureView {
        match &self.target {
            SwapchainTarget::Offscreen { views, current_idx, .. } if !views.is_empty() => {
                &views[*current_idx]
            }
            _ => &self.present_view,
        }
    }

    pub fn get_current_texture(&self) -> &wgpu::Texture {
        match &self.target {
            SwapchainTarget::Offscreen { buffers, current_idx, .. } if !buffers.is_empty() => {
                &buffers[*current_idx]
            }
            _ => &self.present_texture,
        }
    }

    pub fn is_surface(&self) -> bool {
        matches!(self.target, SwapchainTarget::Surface { .. })
    }

    fn update_frame_timing(&mut self) {
        self.frame_count += 1;
        let now = Instant::now();
        if let Some(prev) = self.last_frame_time {
            let elapsed = now.duration_since(prev).as_secs_f32() * 1000.0;
            self.last_frame_duration_ms = elapsed.max(0.001);
        }
        self.last_frame_time = Some(now);
    }

    pub fn get_timing_stats(&self) -> FrameTimingStats {
        let fps = if self.last_frame_duration_ms > 0.0 {
            1000.0 / self.last_frame_duration_ms
        } else {
            120.0
        };

        FrameTimingStats {
            frame_count: self.frame_count,
            fps,
            frame_time_ms: self.last_frame_duration_ms,
            gpu_time_ms: self.last_gpu_duration_ms,
            target_fps: self.target_fps,
            is_120fps_capable: self.last_frame_duration_ms <= 8.33 || (self.last_gpu_duration_ms > 0.0 && self.last_gpu_duration_ms <= 8.33),
        }
    }

    pub fn resolve_gpu_timestamp_query(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if let (Some(qs), Some(qb), Some(sb)) = (&self.query_set, &self.query_buffer, &self.query_staging_buffer) {
            encoder.resolve_query_set(qs, 0..2, qb, 0);
            encoder.copy_buffer_to_buffer(qb, 0, sb, 0, 16);
        }
    }

    pub async fn poll_gpu_timestamps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if let Some(sb) = &self.query_staging_buffer {
            let slice = sb.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = tx.send(res);
            });
            device.poll(wgpu::Maintain::Wait);
            if let Ok(Ok(())) = rx.recv() {
                let view = slice.get_mapped_range();
                if view.len() >= 16 {
                    let ts_start = u64::from_le_bytes(view[0..8].try_into().unwrap());
                    let ts_end = u64::from_le_bytes(view[8..16].try_into().unwrap());
                    let period_ns = queue.get_timestamp_period();
                    if ts_end >= ts_start {
                        let duration_ns = (ts_end - ts_start) as f32 * period_ns;
                        self.last_gpu_duration_ms = duration_ns / 1_000_000.0;
                    }
                }
                drop(view);
                sb.unmap();
            }
        }
    }

    pub fn present_surface_with_copy(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<u64, wgpu::SurfaceError> {
        self.update_frame_timing();

        if let SwapchainTarget::Surface { surface, .. } = &self.target {
            let output = surface.get_current_texture()?;
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Swapchain Surface Blit Encoder"),
            });

            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: self.get_current_texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &output.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
            );

            queue.submit(Some(encoder.finish()));
            output.present();
        } else if let SwapchainTarget::Offscreen { buffers, views: _, current_idx } = &mut self.target {
            if !buffers.is_empty() {
                *current_idx = (*current_idx + 1) % buffers.len();
            }
        }
        Ok(self.frame_count)
    }

    pub fn present_surface(&mut self) -> Result<u64, wgpu::SurfaceError> {
        self.update_frame_timing();
        if let SwapchainTarget::Surface { surface, .. } = &self.target {
            let output = surface.get_current_texture()?;
            output.present();
        } else if let SwapchainTarget::Offscreen { buffers, views: _, current_idx } = &mut self.target {
            if !buffers.is_empty() {
                *current_idx = (*current_idx + 1) % buffers.len();
            }
        }
        Ok(self.frame_count)
    }

    pub fn present(&mut self) -> u64 {
        self.update_frame_timing();
        if let SwapchainTarget::Surface { surface, .. } = &self.target {
            if let Ok(output) = surface.get_current_texture() {
                output.present();
            }
        } else if let SwapchainTarget::Offscreen { buffers, views: _, current_idx } = &mut self.target {
            if !buffers.is_empty() {
                *current_idx = (*current_idx + 1) % buffers.len();
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
                texture: self.get_current_texture(),
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
