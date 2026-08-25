use crate::buffer::GlBuffer;
use crate::framebuffer::GlFramebuffer;
use crate::pipeline::{AttribLayoutKey, PipelineCache, Shader, ShaderProgram};
use crate::shader::ShaderTranslator;
use crate::texture::GlTexture;
use metrics_overlay::MetricsTracker;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ScissorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct VertexAttrib {
    pub enabled: bool,
    pub size: i32,
    pub attrib_type: u32,
    pub normalized: bool,
    pub stride: usize,
    pub offset: usize,
    pub buffer_id: u32,
}

impl Default for VertexAttrib {
    fn default() -> Self {
        Self {
            enabled: false,
            size: 4,
            attrib_type: 0x1406, // GL_FLOAT
            normalized: false,
            stride: 0,
            offset: 0,
            buffer_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GlVertexArrayObject {
    pub id: u32,
    pub attribs: Vec<VertexAttrib>,
    pub bound_element_array_buffer_id: u32,
}

pub struct GlContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface_format: wgpu::TextureFormat,

    // GLES state
    pub viewport: Viewport,
    pub scissor: ScissorRect,
    pub scissor_test_enabled: bool,
    pub clear_color: [f32; 4],
    pub clear_depth: f32,
    pub clear_stencil: u32,
    pub blend_enabled: bool,
    pub depth_test_enabled: bool,
    pub cull_face_enabled: bool,
    pub blend_src_factor: u32,
    pub blend_dst_factor: u32,
    pub depth_func: u32,
    pub depth_mask: bool,
    pub last_error: u32,

    pub current_program_id: u32,
    pub bound_array_buffer_id: u32,
    pub bound_element_array_buffer_id: u32,
    pub bound_vertex_array_id: u32,
    pub bound_framebuffer_id: u32,
    pub active_texture_unit: u32,
    pub bound_texture_2d_id: u32,

    // Uniform buffer (256 bytes for standard MVP + color + vec4 constants)
    pub uniform_buffer: wgpu::Buffer,
    pub uniform_data: Vec<u8>,

    // Resources
    pub buffers: HashMap<u32, GlBuffer>,
    pub textures: HashMap<u32, GlTexture>,
    pub framebuffers: HashMap<u32, GlFramebuffer>,
    pub vertex_arrays: HashMap<u32, GlVertexArrayObject>,
    pub pipeline_cache: PipelineCache,
    pub vertex_attribs: Vec<VertexAttrib>,

    // Offscreen render targets
    pub default_render_target: GlTexture,
    pub default_depth_texture: Option<wgpu::Texture>,
    pub default_depth_target: Option<wgpu::TextureView>,
    pub dummy_texture_view: wgpu::TextureView,
    pub dummy_sampler: wgpu::Sampler,

    pub metrics: MetricsTracker,
    pub shader_translator: ShaderTranslator,
    next_id: u32,
}

impl GlContext {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "Failed to find an appropriate GPU adapter (WebGPU disabled or unavailable)".to_string())?;

        let limits = adapter.limits();
        let mut required_features = wgpu::Features::empty();
        if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("GLES2WGPU Device"),
                    required_features,
                    required_limits: limits,
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| format!("Failed to create WebGPU device: {:?}", e))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut default_render_target = GlTexture::new(0, 0x0DE1); // GL_TEXTURE_2D
        default_render_target.allocate_2d(&device, width, height, surface_format);

        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Default Depth Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GLES Dummy Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: surface_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_texture_view = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let dummy_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GLES Dummy Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GLES Uniform Buffer"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_data = vec![0u8; 256];

        let mut attribs = Vec::with_capacity(16);
        for _ in 0..16 {
            attribs.push(VertexAttrib::default());
        }

        Ok(Self {
            device,
            queue,
            surface_format,
            viewport: Viewport {
                x: 0,
                y: 0,
                width,
                height,
            },
            scissor: ScissorRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            scissor_test_enabled: false,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            clear_depth: 1.0,
            clear_stencil: 0,
            blend_enabled: false,
            depth_test_enabled: false,
            cull_face_enabled: false,
            blend_src_factor: 0x0302, // GL_SRC_ALPHA
            blend_dst_factor: 0x0303, // GL_ONE_MINUS_SRC_ALPHA
            depth_func: 0x0201,       // GL_LESS
            depth_mask: true,
            last_error: 0,            // GL_NO_ERROR
            current_program_id: 0,
            bound_array_buffer_id: 0,
            bound_element_array_buffer_id: 0,
            bound_vertex_array_id: 0,
            bound_framebuffer_id: 0,
            active_texture_unit: 0,
            bound_texture_2d_id: 0,
            uniform_buffer,
            uniform_data,
            buffers: HashMap::new(),
            textures: HashMap::new(),
            framebuffers: HashMap::new(),
            vertex_arrays: HashMap::new(),
            pipeline_cache: PipelineCache::new(),
            vertex_attribs: attribs,
            default_render_target,
            default_depth_texture: Some(depth_tex),
            default_depth_target: Some(depth_view),
            dummy_texture_view,
            dummy_sampler,
            metrics: MetricsTracker::new(),
            shader_translator: ShaderTranslator::new(),
            next_id: 1,
        })
    }

    pub fn gen_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn gl_viewport(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.viewport = Viewport {
            x,
            y,
            width,
            height,
        };
    }

    pub fn gl_scissor(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.scissor = ScissorRect {
            x,
            y,
            width,
            height,
        };
    }

    pub fn gl_enable(&mut self, cap: u32) {
        match cap {
            0x0BE2 => self.blend_enabled = true,        // GL_BLEND
            0x0B71 => self.depth_test_enabled = true,   // GL_DEPTH_TEST
            0x0B44 => self.cull_face_enabled = true,    // GL_CULL_FACE
            0x0C11 => self.scissor_test_enabled = true, // GL_SCISSOR_TEST
            _ => {}
        }
    }

    pub fn gl_disable(&mut self, cap: u32) {
        match cap {
            0x0BE2 => self.blend_enabled = false,
            0x0B71 => self.depth_test_enabled = false,
            0x0B44 => self.cull_face_enabled = false,
            0x0C11 => self.scissor_test_enabled = false,
            _ => {}
        }
    }

    pub fn gl_blend_func(&mut self, sfactor: u32, dfactor: u32) {
        self.blend_src_factor = sfactor;
        self.blend_dst_factor = dfactor;
    }

    pub fn gl_depth_func(&mut self, func: u32) {
        self.depth_func = func;
    }

    pub fn gl_depth_mask(&mut self, flag: bool) {
        self.depth_mask = flag;
    }

    pub fn gl_get_error(&mut self) -> u32 {
        let err = self.last_error;
        self.last_error = 0;
        err
    }

    pub fn gl_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }

    pub fn gl_clear_depthf(&mut self, depth: f32) {
        self.clear_depth = depth;
    }

    pub fn gl_clear_stencil(&mut self, s: u32) {
        self.clear_stencil = s;
    }

    pub fn gl_clear(&mut self, mask: u32) {
        let target_view = if self.bound_framebuffer_id != 0 {
            if let Some(fb) = self.framebuffers.get(&self.bound_framebuffer_id) {
                if let Some(tex_id) = fb.color_attachment_texture_id {
                    self.textures.get(&tex_id).and_then(|t| t.wgpu_view.as_ref())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            self.default_render_target.wgpu_view.as_ref()
        };

        if let Some(view) = target_view {
            let mut encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("glClear Encoder"),
                    });

            let load_op = if mask & 0x00004000 != 0 { // GL_COLOR_BUFFER_BIT
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: self.clear_color[0] as f64,
                    g: self.clear_color[1] as f64,
                    b: self.clear_color[2] as f64,
                    a: self.clear_color[3] as f64,
                })
            } else {
                wgpu::LoadOp::Load
            };

            let depth_stencil_attachment = if (mask & 0x00000100 != 0 || mask & 0x00000400 != 0) // GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT
                && self.default_depth_target.is_some()
            {
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.default_depth_target.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_depth),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_stencil),
                        store: wgpu::StoreOp::Store,
                    }),
                })
            } else {
                None
            };

            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glClear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: load_op,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }

            self.queue.submit(Some(encoder.finish()));
        }
    }

    pub fn gl_gen_vertex_arrays(&mut self, count: usize) -> Vec<u32> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.gen_id();
            let mut attribs = Vec::with_capacity(16);
            for _ in 0..16 {
                attribs.push(VertexAttrib::default());
            }
            self.vertex_arrays.insert(
                id,
                GlVertexArrayObject {
                    id,
                    attribs,
                    bound_element_array_buffer_id: 0,
                },
            );
            ids.push(id);
        }
        ids
    }

    pub fn gl_bind_vertex_array(&mut self, id: u32) {
        self.bound_vertex_array_id = id;
        if id != 0 {
            if let Some(vao) = self.vertex_arrays.get(&id) {
                self.vertex_attribs = vao.attribs.clone();
                self.bound_element_array_buffer_id = vao.bound_element_array_buffer_id;
            }
        }
    }

    pub fn gl_delete_vertex_arrays(&mut self, ids: &[u32]) {
        for &id in ids {
            self.vertex_arrays.remove(&id);
            if self.bound_vertex_array_id == id {
                self.bound_vertex_array_id = 0;
            }
        }
    }

    pub fn gl_create_shader(&mut self, stage: u32) -> u32 {
        let id = self.gen_id();
        self.pipeline_cache.shaders.insert(id, Shader::new(id, stage));
        id
    }

    pub fn gl_shader_source(&mut self, shader_id: u32, source: &str) {
        if let Some(shader) = self.pipeline_cache.shaders.get_mut(&shader_id) {
            shader.source = source.to_string();
        }
    }

    pub fn gl_compile_shader(&mut self, shader_id: u32) -> Result<(), String> {
        self.pipeline_cache
            .compile_shader(shader_id, &self.shader_translator)
    }

    pub fn gl_create_program(&mut self) -> u32 {
        let id = self.gen_id();
        self.pipeline_cache
            .programs
            .insert(id, ShaderProgram::new(id));
        id
    }

    pub fn gl_attach_shader(&mut self, program_id: u32, shader_id: u32) {
        if let Some(prog) = self.pipeline_cache.programs.get_mut(&program_id) {
            if !prog.attached_shaders.contains(&shader_id) {
                prog.attached_shaders.push(shader_id);
            }
        }
    }

    pub fn gl_link_program(&mut self, program_id: u32) -> Result<(), String> {
        self.pipeline_cache.link_program(program_id)
    }

    pub fn gl_use_program(&mut self, program_id: u32) {
        self.current_program_id = program_id;
    }

    pub fn gl_get_uniform_location(&mut self, program_id: u32, name: &str) -> i32 {
        if let Some(prog) = self.pipeline_cache.programs.get_mut(&program_id) {
            let next_loc = prog.uniform_locations.len() as u32;
            let loc = *prog.uniform_locations.entry(name.to_string()).or_insert(next_loc);
            loc as i32
        } else {
            -1
        }
    }

    pub fn gl_uniform_1f(&mut self, location: i32, v0: f32) {
        if location >= 0 {
            let offset = (location as usize) * 16;
            if offset + 4 <= self.uniform_data.len() {
                let bytes = v0.to_le_bytes();
                self.uniform_data[offset..offset + 4].copy_from_slice(&bytes);
                self.queue.write_buffer(&self.uniform_buffer, offset as u64, &bytes);
            }
        }
    }

    pub fn gl_uniform_4fv(&mut self, location: i32, count: usize, value: &[f32]) {
        if location >= 0 {
            let offset = (location as usize) * 16;
            let byte_len = count * 16;
            if offset + byte_len <= self.uniform_data.len() && value.len() >= count * 4 {
                let bytes: &[u8] = bytemuck::cast_slice(&value[0..count * 4]);
                self.uniform_data[offset..offset + bytes.len()].copy_from_slice(bytes);
                self.queue.write_buffer(&self.uniform_buffer, offset as u64, bytes);
            }
        }
    }

    pub fn gl_uniform_matrix_4fv(&mut self, location: i32, _count: usize, _transpose: bool, value: &[f32]) {
        if location >= 0 && value.len() >= 16 {
            let offset = (location as usize) * 64;
            if offset + 64 <= self.uniform_data.len() {
                let bytes: &[u8] = bytemuck::cast_slice(&value[0..16]);
                self.uniform_data[offset..offset + 64].copy_from_slice(bytes);
                self.queue.write_buffer(&self.uniform_buffer, offset as u64, bytes);
            }
        }
    }

    pub fn gl_gen_buffers(&mut self, count: usize) -> Vec<u32> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.gen_id();
            self.buffers.insert(id, GlBuffer::new(id, 0));
            ids.push(id);
        }
        ids
    }

    pub fn gl_bind_buffer(&mut self, target: u32, id: u32) {
        match target {
            0x8892 => self.bound_array_buffer_id = id,
            0x8893 => {
                self.bound_element_array_buffer_id = id;
                if self.bound_vertex_array_id != 0 {
                    if let Some(vao) = self.vertex_arrays.get_mut(&self.bound_vertex_array_id) {
                        vao.bound_element_array_buffer_id = id;
                    }
                }
            }
            _ => {}
        }
        if let Some(buf) = self.buffers.get_mut(&id) {
            buf.target = target;
        }
    }

    pub fn gl_buffer_data(&mut self, target: u32, data: &[u8], _usage: u32) {
        let buf_id = match target {
            0x8892 => self.bound_array_buffer_id,
            0x8893 => self.bound_element_array_buffer_id,
            _ => 0,
        };
        if buf_id != 0 {
            if let Some(buf) = self.buffers.get_mut(&buf_id) {
                buf.set_data(data);
                let wgpu_usage = if target == 0x8892 {
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
                } else {
                    wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST
                };
                buf.sync_to_wgpu(&self.device, wgpu_usage);
            }
        }
    }

    pub fn gl_delete_buffers(&mut self, ids: &[u32]) {
        for &id in ids {
            self.buffers.remove(&id);
            if self.bound_array_buffer_id == id {
                self.bound_array_buffer_id = 0;
            }
            if self.bound_element_array_buffer_id == id {
                self.bound_element_array_buffer_id = 0;
            }
        }
    }

    pub fn gl_gen_textures(&mut self, count: usize) -> Vec<u32> {
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.gen_id();
            self.textures.insert(id, GlTexture::new(id, 0x0DE1));
            ids.push(id);
        }
        ids
    }

    pub fn gl_bind_texture(&mut self, target: u32, id: u32) {
        if target == 0x0DE1 {
            self.bound_texture_2d_id = id;
        }
    }

    pub fn gl_tex_parameteri(&mut self, target: u32, pname: u32, param: i32) {
        if target == 0x0DE1 && self.bound_texture_2d_id != 0 {
            if let Some(tex) = self.textures.get_mut(&self.bound_texture_2d_id) {
                match pname {
                    0x2800 => tex.mag_filter = param as u32, // GL_TEXTURE_MAG_FILTER
                    0x2801 => tex.min_filter = param as u32, // GL_TEXTURE_MIN_FILTER
                    0x2802 => tex.wrap_s = param as u32,     // GL_TEXTURE_WRAP_S
                    0x2803 => tex.wrap_t = param as u32,     // GL_TEXTURE_WRAP_T
                    _ => {}
                }
                tex.update_sampler(&self.device);
            }
        }
    }

    pub fn gl_tex_image_2d(
        &mut self,
        _target: u32,
        _level: i32,
        _internal_format: i32,
        width: u32,
        height: u32,
        _border: i32,
        _format: u32,
        _type: u32,
        pixels: Option<&[u8]>,
    ) {
        let tex_id = self.bound_texture_2d_id;
        if tex_id != 0 {
            if let Some(tex) = self.textures.get_mut(&tex_id) {
                if let Some(data) = pixels {
                    tex.upload_image_data(&self.device, &self.queue, width, height, data);
                } else {
                    tex.allocate_2d(&self.device, width, height, self.surface_format);
                }
            }
        }
    }

    pub fn gl_delete_textures(&mut self, ids: &[u32]) {
        for &id in ids {
            self.textures.remove(&id);
            if self.bound_texture_2d_id == id {
                self.bound_texture_2d_id = 0;
            }
        }
    }

    pub fn gl_vertex_attrib_pointer(
        &mut self,
        index: usize,
        size: i32,
        attrib_type: u32,
        normalized: bool,
        stride: usize,
        offset: usize,
    ) {
        if index < self.vertex_attribs.len() {
            let attr = VertexAttrib {
                enabled: true,
                size,
                attrib_type,
                normalized,
                stride,
                offset,
                buffer_id: self.bound_array_buffer_id,
            };
            self.vertex_attribs[index] = attr;
            if self.bound_vertex_array_id != 0 {
                if let Some(vao) = self.vertex_arrays.get_mut(&self.bound_vertex_array_id) {
                    vao.attribs[index] = attr;
                }
            }
        }
    }

    pub fn gl_enable_vertex_attrib_array(&mut self, index: usize) {
        if index < self.vertex_attribs.len() {
            self.vertex_attribs[index].enabled = true;
            if self.bound_vertex_array_id != 0 {
                if let Some(vao) = self.vertex_arrays.get_mut(&self.bound_vertex_array_id) {
                    vao.attribs[index].enabled = true;
                }
            }
        }
    }

    pub fn gl_disable_vertex_attrib_array(&mut self, index: usize) {
        if index < self.vertex_attribs.len() {
            self.vertex_attribs[index].enabled = false;
            if self.bound_vertex_array_id != 0 {
                if let Some(vao) = self.vertex_arrays.get_mut(&self.bound_vertex_array_id) {
                    vao.attribs[index].enabled = false;
                }
            }
        }
    }

    pub fn gl_check_framebuffer_status(&self, _target: u32) -> u32 {
        if self.bound_framebuffer_id != 0 {
            if let Some(fb) = self.framebuffers.get(&self.bound_framebuffer_id) {
                if fb.color_attachment_texture_id.is_some() {
                    0x8CD5 // GL_FRAMEBUFFER_COMPLETE
                } else {
                    0x8CD6 // GL_FRAMEBUFFER_INCOMPLETE_ATTACHMENT
                }
            } else {
                0x8CD6
            }
        } else {
            0x8CD5
        }
    }

    pub fn build_attrib_keys(&self) -> Vec<AttribLayoutKey> {
        let mut keys = Vec::new();

        for (loc, attrib) in self.vertex_attribs.iter().enumerate() {
            if attrib.enabled {
                let format_id = match attrib.attrib_type {
                    0x1406 => match attrib.size { // GL_FLOAT
                        1 => 1,
                        2 => 2,
                        3 => 3,
                        4 => 4,
                        _ => 3,
                    },
                    0x1401 => match attrib.size { // GL_UNSIGNED_BYTE
                        2 => if attrib.normalized { 12 } else { 10 },
                        4 => if attrib.normalized { 13 } else { 11 },
                        _ => 13,
                    },
                    0x1402 => match attrib.size { // GL_SHORT
                        2 => if attrib.normalized { 20 } else { 22 },
                        4 => if attrib.normalized { 21 } else { 23 },
                        _ => if attrib.normalized { 20 } else { 22 },
                    },
                    _ => 3,
                };

                let stride = if attrib.stride > 0 {
                    attrib.stride as u64
                } else {
                    (attrib.size as u64) * 4
                };

                keys.push(AttribLayoutKey {
                    slot: loc as u32,
                    offset: attrib.offset as u64,
                    stride,
                    format_id,
                });
            }
        }

        if keys.is_empty() {
            keys.push(AttribLayoutKey {
                slot: 0,
                offset: 0,
                stride: 12,
                format_id: 3,
            });
        }

        keys
    }

    pub fn gl_draw_arrays(&mut self, _mode: u32, first: u32, count: u32) {
        self.metrics.record_draw(count / 3);

        let target_view = if self.bound_framebuffer_id != 0 {
            if let Some(fb) = self.framebuffers.get(&self.bound_framebuffer_id) {
                if let Some(tex_id) = fb.color_attachment_texture_id {
                    self.textures.get(&tex_id).and_then(|t| t.wgpu_view.as_ref())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            self.default_render_target.wgpu_view.as_ref()
        };

        if let Some(view) = target_view {
            let attrib_keys = self.build_attrib_keys();

            let pipeline_result = self.pipeline_cache.get_or_create_pipeline(
                &self.device,
                self.current_program_id,
                self.surface_format,
                &attrib_keys,
                self.blend_enabled,
                self.depth_test_enabled,
                self.cull_face_enabled,
                self.depth_func,
            );

            let mut encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("glDrawArrays Encoder"),
                    });

            let depth_stencil_attachment = if self.depth_test_enabled && self.default_depth_target.is_some() {
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.default_depth_target.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                })
            } else {
                None
            };

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glDrawArrays Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_viewport(
                    self.viewport.x as f32,
                    self.viewport.y as f32,
                    self.viewport.width as f32,
                    self.viewport.height as f32,
                    0.0,
                    1.0,
                );

                if self.scissor_test_enabled {
                    render_pass.set_scissor_rect(
                        self.scissor.x.max(0) as u32,
                        self.scissor.y.max(0) as u32,
                        self.scissor.width,
                        self.scissor.height,
                    );
                }

                if let Ok(pipeline) = pipeline_result {
                    render_pass.set_pipeline(pipeline);
                }

                let bind_group_layout = self.pipeline_cache.get_or_create_main_bind_group_layout(&self.device);
                let (wgpu_view, sampler) = if let Some(tex) = self.textures.get(&self.bound_texture_2d_id) {
                    if let (Some(v), Some(s)) = (&tex.wgpu_view, &tex.wgpu_sampler) {
                        (v, s)
                    } else {
                        (&self.dummy_texture_view, &self.dummy_sampler)
                    }
                } else {
                    (&self.dummy_texture_view, &self.dummy_sampler)
                };

                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GLES Draw Bind Group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(wgpu_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.uniform_buffer.as_entire_binding(),
                        },
                    ],
                });
                render_pass.set_bind_group(0, &bind_group, &[]);

                // Bind all per-slot VBOs
                for (slot, attrib) in self.vertex_attribs.iter().enumerate() {
                    if attrib.enabled {
                        let vbo_id = if attrib.buffer_id != 0 {
                            attrib.buffer_id
                        } else {
                            self.bound_array_buffer_id
                        };
                        if let Some(vbo) = self.buffers.get(&vbo_id) {
                            if let Some(wgpu_buf) = &vbo.wgpu_buffer {
                                render_pass.set_vertex_buffer(slot as u32, wgpu_buf.slice(..));
                            }
                        }
                    }
                }

                if !self.vertex_attribs.iter().any(|a| a.enabled) {
                    if let Some(vbo) = self.buffers.get(&self.bound_array_buffer_id) {
                        if let Some(wgpu_buf) = &vbo.wgpu_buffer {
                            render_pass.set_vertex_buffer(0, wgpu_buf.slice(..));
                        }
                    }
                }

                render_pass.draw(first..(first + count), 0..1);
            }

            self.queue.submit(Some(encoder.finish()));
        }
    }

    pub fn gl_draw_elements(&mut self, _mode: u32, count: u32, type_: u32, offset: usize) {
        self.metrics.record_draw(count / 3);

        let target_view = if self.bound_framebuffer_id != 0 {
            if let Some(fb) = self.framebuffers.get(&self.bound_framebuffer_id) {
                if let Some(tex_id) = fb.color_attachment_texture_id {
                    self.textures.get(&tex_id).and_then(|t| t.wgpu_view.as_ref())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            self.default_render_target.wgpu_view.as_ref()
        };

        if let Some(view) = target_view {
            let attrib_keys = self.build_attrib_keys();

            let pipeline_result = self.pipeline_cache.get_or_create_pipeline(
                &self.device,
                self.current_program_id,
                self.surface_format,
                &attrib_keys,
                self.blend_enabled,
                self.depth_test_enabled,
                self.cull_face_enabled,
                self.depth_func,
            );

            let mut encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("glDrawElements Encoder"),
                    });

            let depth_stencil_attachment = if self.depth_test_enabled && self.default_depth_target.is_some() {
                Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.default_depth_target.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                })
            } else {
                None
            };

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glDrawElements Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: load_op_target(view),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                render_pass.set_viewport(
                    self.viewport.x as f32,
                    self.viewport.y as f32,
                    self.viewport.width as f32,
                    self.viewport.height as f32,
                    0.0,
                    1.0,
                );

                if self.scissor_test_enabled {
                    render_pass.set_scissor_rect(
                        self.scissor.x.max(0) as u32,
                        self.scissor.y.max(0) as u32,
                        self.scissor.width,
                        self.scissor.height,
                    );
                }

                if let Ok(pipeline) = pipeline_result {
                    render_pass.set_pipeline(pipeline);
                }

                let bind_group_layout = self.pipeline_cache.get_or_create_main_bind_group_layout(&self.device);
                let (wgpu_view, sampler) = if let Some(tex) = self.textures.get(&self.bound_texture_2d_id) {
                    if let (Some(v), Some(s)) = (&tex.wgpu_view, &tex.wgpu_sampler) {
                        (v, s)
                    } else {
                        (&self.dummy_texture_view, &self.dummy_sampler)
                    }
                } else {
                    (&self.dummy_texture_view, &self.dummy_sampler)
                };

                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GLES Draw Bind Group"),
                    layout: bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(wgpu_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.uniform_buffer.as_entire_binding(),
                        },
                    ],
                });
                render_pass.set_bind_group(0, &bind_group, &[]);

                // Bind all per-slot VBOs
                for (slot, attrib) in self.vertex_attribs.iter().enumerate() {
                    if attrib.enabled {
                        let vbo_id = if attrib.buffer_id != 0 {
                            attrib.buffer_id
                        } else {
                            self.bound_array_buffer_id
                        };
                        if let Some(vbo) = self.buffers.get(&vbo_id) {
                            if let Some(wgpu_buf) = &vbo.wgpu_buffer {
                                render_pass.set_vertex_buffer(slot as u32, wgpu_buf.slice(..));
                            }
                        }
                    }
                }

                if !self.vertex_attribs.iter().any(|a| a.enabled) {
                    if let Some(vbo) = self.buffers.get(&self.bound_array_buffer_id) {
                        if let Some(wgpu_buf) = &vbo.wgpu_buffer {
                            render_pass.set_vertex_buffer(0, wgpu_buf.slice(..));
                        }
                    }
                }

                let (index_format, elem_size) = if type_ == 0x1405 {
                    (wgpu::IndexFormat::Uint32, 4)
                } else {
                    (wgpu::IndexFormat::Uint16, 2)
                };

                if let Some(ibo) = self.buffers.get(&self.bound_element_array_buffer_id) {
                    if let Some(wgpu_buf) = &ibo.wgpu_buffer {
                        let start_byte = (offset * elem_size) as u64;
                        render_pass.set_index_buffer(wgpu_buf.slice(start_byte..), index_format);
                        render_pass.draw_indexed(0..count, 0, 0..1);
                    }
                }
            }

            self.queue.submit(Some(encoder.finish()));
        }
    }
}

fn load_op_target(_view: &wgpu::TextureView) -> wgpu::LoadOp<wgpu::Color> {
    wgpu::LoadOp::Load
}
