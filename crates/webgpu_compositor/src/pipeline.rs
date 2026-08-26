use crate::layer::BlendMode;
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct QuadVertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LayerUniform {
    pub bounds: [f32; 4],
    pub color: [f32; 4],
    pub params: [f32; 4], // [alpha, use_texture, 0.0, 0.0]
    pub source_crop: [f32; 4], // [u_min, v_min, u_max, v_max]
    pub transform: [[f32; 4]; 4],
}

pub struct CompositorPipeline {
    pub pipelines: HashMap<BlendMode, wgpu::RenderPipeline>,
    pub vertex_buffer: wgpu::Buffer,
    pub sampler: wgpu::Sampler,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub dummy_texture_view: wgpu::TextureView,
}

impl CompositorPipeline {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compositor Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

struct LayerUniform {
    bounds: vec4<f32>, // x, y, width, height
    color: vec4<f32>,
    params: vec4<f32>, // x: alpha, y: use_texture, z: swizzle_bgrx (if > 0.5 swizzle BGRX to RGBA with full alpha)
    source_crop: vec4<f32>, // u_min, v_min, u_max, v_max
    transform: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> u_layer: LayerUniform;

@group(0) @binding(1)
var t_diffuse: texture_2d<f32>;

@group(0) @binding(2)
var s_diffuse: sampler;

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let local_pos = vec4<f32>(model.position, 0.0, 1.0);
    let transformed = u_layer.transform * local_pos;
    let pos = u_layer.bounds.xy + transformed.xy * u_layer.bounds.zw;
    out.clip_position = vec4<f32>(pos, 0.0, 1.0);

    let crop_min = u_layer.source_crop.xy;
    let crop_max = u_layer.source_crop.zw;
    out.tex_coords = crop_min + model.tex_coords * (crop_max - crop_min);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var base_color: vec4<f32>;
    if (u_layer.params.y > 0.5) {
        base_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
        if (u_layer.params.z > 0.5) {
            base_color = vec4<f32>(base_color.b, base_color.g, base_color.r, 1.0);
        }
    } else {
        base_color = u_layer.color;
    }
    return vec4<f32>(base_color.rgb, base_color.a * u_layer.params.x);
}
"#
                .into(),
            ),
        });

        let vertices = [
            QuadVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 0.0],
                tex_coords: [1.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
                tex_coords: [1.0, 0.0],
            },
            QuadVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
                tex_coords: [1.0, 0.0],
            },
            QuadVertex {
                position: [0.0, 1.0],
                tex_coords: [0.0, 0.0],
            },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Compositor Quad VB"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compositor BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Compositor Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let mut pipelines = HashMap::new();
        let modes = [
            (BlendMode::None, Some(wgpu::BlendState::REPLACE)),
            (
                BlendMode::Premultiplied,
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            ),
            (
                BlendMode::Coverage,
                Some(wgpu::BlendState::ALPHA_BLENDING),
            ),
        ];

        for (mode, blend_state) in modes {
            let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("Compositor Pipeline {:?}", mode)),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                            wgpu::VertexAttribute {
                                offset: 8,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x2,
                            },
                        ],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: blend_state,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });
            pipelines.insert(mode, p);
        }

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Compositor Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Compositor Dummy Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_texture_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            pipelines,
            vertex_buffer,
            sampler,
            bind_group_layout,
            dummy_texture_view,
        }
    }
}
