use crate::shader::ShaderTranslator;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Shader {
    pub id: u32,
    pub stage: u32, // 0x8B31 = GL_VERTEX_SHADER, 0x8B30 = GL_FRAGMENT_SHADER
    pub source: String,
    pub wgsl: Option<String>,
    pub compiled: bool,
}

impl Shader {
    pub fn new(id: u32, stage: u32) -> Self {
        Self {
            id,
            stage,
            source: String::new(),
            wgsl: None,
            compiled: false,
        }
    }
}

pub struct ShaderProgram {
    pub id: u32,
    pub attached_shaders: Vec<u32>,
    pub vertex_wgsl: Option<String>,
    pub fragment_wgsl: Option<String>,
    pub uniform_locations: HashMap<String, u32>,
    pub linked: bool,
}

impl ShaderProgram {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            attached_shaders: Vec::new(),
            vertex_wgsl: None,
            fragment_wgsl: None,
            uniform_locations: HashMap::new(),
            linked: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttribLayoutKey {
    pub slot: u32,
    pub offset: u64,
    pub stride: u64,
    pub format_id: u32,
}

pub struct PipelineCache {
    pub shaders: HashMap<u32, Shader>,
    pub programs: HashMap<u32, ShaderProgram>,
    pub wgpu_pipelines: HashMap<(u32, wgpu::TextureFormat, bool, bool, u32, bool, Vec<AttribLayoutKey>), wgpu::RenderPipeline>,
    pub main_bind_group_layout: Option<wgpu::BindGroupLayout>,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self {
            shaders: HashMap::new(),
            programs: HashMap::new(),
            wgpu_pipelines: HashMap::new(),
            main_bind_group_layout: None,
        }
    }

    pub fn get_or_create_main_bind_group_layout(
        &mut self,
        device: &wgpu::Device,
    ) -> &wgpu::BindGroupLayout {
        if self.main_bind_group_layout.is_none() {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GLES Main Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
            self.main_bind_group_layout = Some(layout);
        }
        self.main_bind_group_layout.as_ref().unwrap()
    }

    pub fn compile_shader(
        &mut self,
        shader_id: u32,
        translator: &ShaderTranslator,
    ) -> Result<(), String> {
        let shader = self
            .shaders
            .get_mut(&shader_id)
            .ok_or_else(|| format!("Shader {} not found", shader_id))?;

        let stage = if shader.stage == 0x8B31 {
            naga::ShaderStage::Vertex
        } else {
            naga::ShaderStage::Fragment
        };

        let wgsl = translator
            .translate(&shader.source, stage)
            .map_err(|e| format!("Shader translation error: {:?}", e))?;

        shader.wgsl = Some(wgsl);
        shader.compiled = true;
        Ok(())
    }

    pub fn link_program(&mut self, program_id: u32) -> Result<(), String> {
        let program = self
            .programs
            .get_mut(&program_id)
            .ok_or_else(|| format!("Program {} not found", program_id))?;

        let mut vert_wgsl = None;
        let mut frag_wgsl = None;

        for &s_id in &program.attached_shaders {
            if let Some(shader) = self.shaders.get(&s_id) {
                if shader.stage == 0x8B31 {
                    vert_wgsl = shader.wgsl.clone();
                } else if shader.stage == 0x8B30 {
                    frag_wgsl = shader.wgsl.clone();
                }
            }
        }

        program.vertex_wgsl = vert_wgsl;
        program.fragment_wgsl = frag_wgsl;
        program.linked = program.vertex_wgsl.is_some() && program.fragment_wgsl.is_some();

        if !program.linked {
            return Err("Program link failed: missing vertex or fragment stage".to_string());
        }

        Ok(())
    }

    pub fn get_or_create_pipeline(
        &mut self,
        device: &wgpu::Device,
        program_id: u32,
        target_format: wgpu::TextureFormat,
        attrib_keys: &[AttribLayoutKey],
        blend_enabled: bool,
        depth_test_enabled: bool,
        cull_face_enabled: bool,
        depth_func: u32,
    ) -> Result<&wgpu::RenderPipeline, String> {
        let key = (
            program_id,
            target_format,
            blend_enabled,
            depth_test_enabled,
            if depth_test_enabled { depth_func } else { 0 },
            cull_face_enabled,
            attrib_keys.to_vec(),
        );

        if !self.wgpu_pipelines.contains_key(&key) {
            let program = self
                .programs
                .get(&program_id)
                .ok_or_else(|| format!("Program {} not found", program_id))?;

            let vert_src = program
                .vertex_wgsl
                .as_ref()
                .ok_or_else(|| "Missing vertex WGSL".to_string())?;
            let frag_src = program
                .fragment_wgsl
                .as_ref()
                .ok_or_else(|| "Missing fragment WGSL".to_string())?;

            let vert_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("Prog_{}_Vert_Shader", program_id)),
                source: wgpu::ShaderSource::Wgsl(vert_src.as_str().into()),
            });

            let frag_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&format!("Prog_{}_Frag_Shader", program_id)),
                source: wgpu::ShaderSource::Wgsl(frag_src.as_str().into()),
            });

            let main_layout = self.get_or_create_main_bind_group_layout(device);
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(&format!("Prog_{}_Pipeline_Layout", program_id)),
                bind_group_layouts: &[main_layout],
                push_constant_ranges: &[],
            });

            let blend_state = if blend_enabled {
                Some(wgpu::BlendState::ALPHA_BLENDING)
            } else {
                Some(wgpu::BlendState::REPLACE)
            };

            let depth_compare = match depth_func {
                0x0200 => wgpu::CompareFunction::Never,
                0x0201 => wgpu::CompareFunction::Less,
                0x0202 => wgpu::CompareFunction::Equal,
                0x0203 => wgpu::CompareFunction::LessEqual,
                0x0204 => wgpu::CompareFunction::Greater,
                0x0205 => wgpu::CompareFunction::NotEqual,
                0x0206 => wgpu::CompareFunction::GreaterEqual,
                0x0207 => wgpu::CompareFunction::Always,
                _ => wgpu::CompareFunction::Less,
            };

            let depth_stencil = if depth_test_enabled {
                Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: true,
                    depth_compare,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                })
            } else {
                None
            };

            let cull_mode = if cull_face_enabled {
                Some(wgpu::Face::Back)
            } else {
                None
            };

            // Construct VertexBufferLayouts on stack with zero heap allocation
            let num_attribs = attrib_keys.len().min(16);
            let mut attr_storage = [wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            }; 16];

            for (i, k) in attrib_keys.iter().take(num_attribs).enumerate() {
                let format = match k.format_id {
                    1 => wgpu::VertexFormat::Float32,
                    2 => wgpu::VertexFormat::Float32x2,
                    3 => wgpu::VertexFormat::Float32x3,
                    4 => wgpu::VertexFormat::Float32x4,
                    10 => wgpu::VertexFormat::Uint8x2,
                    11 => wgpu::VertexFormat::Uint8x4,
                    12 => wgpu::VertexFormat::Unorm8x2,
                    13 => wgpu::VertexFormat::Unorm8x4,
                    20 => wgpu::VertexFormat::Snorm16x2,
                    21 => wgpu::VertexFormat::Snorm16x4,
                    22 => wgpu::VertexFormat::Sint16x2,
                    23 => wgpu::VertexFormat::Sint16x4,
                    _ => wgpu::VertexFormat::Float32x3,
                };

                attr_storage[i] = wgpu::VertexAttribute {
                    offset: k.offset,
                    shader_location: k.slot,
                    format,
                };
            }

            let mut vbls = Vec::with_capacity(num_attribs);
            for (i, k) in attrib_keys.iter().take(num_attribs).enumerate() {
                vbls.push(wgpu::VertexBufferLayout {
                    array_stride: k.stride,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: std::slice::from_ref(&attr_storage[i]),
                });
            }

            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("Prog_{}_RenderPipeline", program_id)),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vert_module,
                    entry_point: Some("main"),
                    buffers: &vbls,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &frag_module,
                    entry_point: Some("main"),
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
                    cull_mode,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            self.wgpu_pipelines.insert(key.clone(), pipeline);
        }

        Ok(self.wgpu_pipelines.get(&key).unwrap())
    }
}
