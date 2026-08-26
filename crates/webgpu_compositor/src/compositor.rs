use crate::layer::{CompositionLayer, BlendMode};
use crate::pipeline::{CompositorPipeline, LayerUniform};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub struct CachedLayerResources {
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub last_bounds: [f32; 4],
    pub last_color: [f32; 4],
    pub last_params: [f32; 4],
    pub last_source_crop: [f32; 4],
    pub last_transform: [[f32; 4]; 4],
    pub last_blend_mode: BlendMode,
    pub has_texture: bool,
    pub last_texture_ptr: usize,
}

pub struct WebGpuCompositor {
    pub pipeline: CompositorPipeline,
    pub layers: HashMap<u64, CompositionLayer>,
    pub resource_cache: HashMap<u64, CachedLayerResources>,
    pub query_set: Option<wgpu::QuerySet>,
    pub query_buffer: Option<wgpu::Buffer>,
    pub query_staging_buffer: Option<wgpu::Buffer>,
    pub last_gpu_duration_ms: f32,
}

unsafe impl Send for WebGpuCompositor {}
unsafe impl Sync for WebGpuCompositor {}

impl WebGpuCompositor {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let (query_set, query_buffer, query_staging_buffer) = if device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            let qs = device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("Compositor Timestamp Query Set"),
                count: 2,
                ty: wgpu::QueryType::Timestamp,
            });
            let qb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Compositor Timestamp Buffer"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let sb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Compositor Timestamp Staging Buffer"),
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            (Some(qs), Some(qb), Some(sb))
        } else {
            (None, None, None)
        };

        Self {
            pipeline: CompositorPipeline::new(device, target_format),
            layers: HashMap::new(),
            resource_cache: HashMap::new(),
            query_set,
            query_buffer,
            query_staging_buffer,
            last_gpu_duration_ms: 0.0,
        }
    }

    pub fn add_or_update_layer(&mut self, layer: CompositionLayer) {
        let id = layer.id;
        self.layers.insert(id, layer);
    }

    pub fn remove_layer(&mut self, id: u64) {
        self.layers.remove(&id);
        self.resource_cache.remove(&id);
    }

    fn to_mat4(transform: [f32; 4], hwc_transform: u32) -> [[f32; 4]; 4] {
        let (a, b, c, d) = if transform[1] == 0.0 && transform[2] == 0.0 && transform[0] != 0.0 && transform[3] != 0.0 {
            (transform[0], 0.0, 0.0, transform[3])
        } else if transform[2] == 0.0 && transform[3] == 0.0 && (transform[0] != 0.0 || transform[1] != 0.0) {
            (transform[0], 0.0, 0.0, transform[1])
        } else {
            (transform[0], transform[1], transform[2], transform[3])
        };

        let (r00, r01, r10, r11) = match hwc_transform {
            1 => (-1.0, 0.0, 0.0, 1.0),   // FLIP_H
            2 => (1.0, 0.0, 0.0, -1.0),   // FLIP_V
            4 => (0.0, 1.0, -1.0, 0.0),   // ROT_90
            3 => (-1.0, 0.0, 0.0, -1.0),  // ROT_180
            7 => (0.0, -1.0, 1.0, 0.0),   // ROT_270
            _ => (1.0, 0.0, 0.0, 1.0),    // None
        };

        let m00 = a * r00 + c * r10;
        let m01 = a * r01 + c * r11;
        let m10 = b * r00 + d * r10;
        let m11 = b * r01 + d * r11;

        [
            [m00, m01, 0.0, 0.0],
            [m10, m11, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn compose(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        clear_color: Option<wgpu::Color>,
    ) {
        let mut sorted_layers: Vec<&CompositionLayer> = self.layers.values().collect();
        sorted_layers.sort_by_key(|l| l.z_order);

        // Update / create cached uniform buffers & bind groups
        for layer in &sorted_layers {
            if !layer.visible {
                continue;
            }

            let color = layer.color.unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let params = [
                layer.alpha,
                if layer.texture_view.is_some() { 1.0 } else { 0.0 },
                if layer.swizzle_bgrx { 1.0 } else { 0.0 },
                0.0,
            ];
            let source_crop = layer.source_crop;
            let transform = Self::to_mat4(layer.transform, layer.hwc_transform);
            let tex_ptr = layer
                .texture_view
                .as_ref()
                .map(|v| v as *const _ as usize)
                .unwrap_or(0);

            let needs_recreate = match self.resource_cache.get(&layer.id) {
                Some(cached) => {
                    cached.last_blend_mode != layer.blend_mode
                        || cached.has_texture != layer.texture_view.is_some()
                        || cached.last_texture_ptr != tex_ptr
                }
                None => true,
            };

            if needs_recreate {
                let uniform = LayerUniform {
                    bounds: layer.bounds,
                    color,
                    params,
                    source_crop,
                    transform,
                };

                let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("Layer_{}_Uniform", layer.id)),
                    contents: bytemuck::cast_slice(&[uniform]),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });

                let texture_view = layer
                    .texture_view
                    .as_ref()
                    .unwrap_or(&self.pipeline.dummy_texture_view);

                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(&format!("Layer_{}_BG", layer.id)),
                    layout: &self.pipeline.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.pipeline.sampler),
                        },
                    ],
                });

                self.resource_cache.insert(
                    layer.id,
                    CachedLayerResources {
                        uniform_buffer,
                        bind_group,
                        last_bounds: layer.bounds,
                        last_color: color,
                        last_params: params,
                        last_source_crop: source_crop,
                        last_transform: transform,
                        last_blend_mode: layer.blend_mode,
                        has_texture: layer.texture_view.is_some(),
                        last_texture_ptr: tex_ptr,
                    },
                );
            } else if let Some(cached) = self.resource_cache.get_mut(&layer.id) {
                if cached.last_bounds != layer.bounds
                    || cached.last_color != color
                    || cached.last_params != params
                    || cached.last_source_crop != source_crop
                    || cached.last_transform != transform
                {
                    let uniform = LayerUniform {
                        bounds: layer.bounds,
                        color,
                        params,
                        source_crop,
                        transform,
                    };
                    queue.write_buffer(
                        &cached.uniform_buffer,
                        0,
                        bytemuck::cast_slice(&[uniform]),
                    );
                    cached.last_bounds = layer.bounds;
                    cached.last_color = color;
                    cached.last_params = params;
                    cached.last_source_crop = source_crop;
                    cached.last_transform = transform;
                }
            }
        }

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("WebGPU Compositor Encoder"),
        });

        let timestamp_writes = self.query_set.as_ref().map(|qs| wgpu::RenderPassTimestampWrites {
            query_set: qs,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        });

        {
            let load_op = match clear_color {
                Some(c) => wgpu::LoadOp::Clear(c),
                None => wgpu::LoadOp::Load,
            };

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("WebGPU Compositor Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
            });

            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));

            for layer in sorted_layers {
                if !layer.visible {
                    continue;
                }

                if let Some(damage) = layer.damage_rect {
                    render_pass.set_scissor_rect(
                        damage[0].max(0.0) as u32,
                        damage[1].max(0.0) as u32,
                        damage[2].max(1.0) as u32,
                        damage[3].max(1.0) as u32,
                    );
                }

                let pipeline = self
                    .pipeline
                    .pipelines
                    .get(&layer.blend_mode)
                    .unwrap_or(&self.pipeline.pipelines[&BlendMode::None]);

                render_pass.set_pipeline(pipeline);

                if let Some(cached) = self.resource_cache.get(&layer.id) {
                    render_pass.set_bind_group(0, &cached.bind_group, &[]);
                    render_pass.draw(0..6, 0..1);
                }
            }
        }

        if let (Some(qs), Some(qb), Some(sb)) = (&self.query_set, &self.query_buffer, &self.query_staging_buffer) {
            encoder.resolve_query_set(qs, 0..2, qb, 0);
            encoder.copy_buffer_to_buffer(qb, 0, sb, 0, 16);
        }

        queue.submit(Some(encoder.finish()));
    }

    pub async fn poll_gpu_duration(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
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
}

