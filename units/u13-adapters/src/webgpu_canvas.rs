//! Track A: real WebGPU canvas adapter for the browser host.
//!
//! This is the U13 browser adapter the U8 docs point at: it executes U8
//! [`HostAction`](u8_gpu_host::HostAction)s against a real `<canvas>` using
//! the real [`VirtioGpuBridge`](virtio_gpu_bridge::VirtioGpuBridge) →
//! [`GlContext`](gles2wgpu::GlContext) (`gles2wgpu`) → browser WebGPU stack.
//! wasm32-only; the rest of this crate stays portable.
//!
//! ## Flow
//!
//! 1. [`WebGpuCanvas::new`] — grabs the canvas from the DOM, boots the
//!    bridge (which owns its WebGPU device via `GlContext::new`), uploads
//!    the golden-triangle shader + vertex buffer through the public GLES2
//!    entry points, then creates a `wgpu::Surface` for the canvas from a
//!    fresh `wgpu::Instance` and configures it with the **bridge's** device.
//!    (The WebGPU backend's `surface.configure` passes the device straight
//!    to the canvas context — any device from the same `navigator.gpu`
//!    works; verified in wgpu 24.0.5's `backend/webgpu.rs`.)
//! 2. [`WebGpuCanvas::execute_cmd`] — U8 `dispatch` of one [`GpuCmd`], then
//!    execution. `Submit3DWire` is re-encoded as a `VIRTIO_GPU_CMD_SUBMIT_3D`
//!    binary packet and fed to `process_binary_wire_command` (the only path
//!    that reaches the private `execute_submit_3d`; the typed
//!    `execute_command` no-ops `Submit3D`).
//! 3. [`WebGpuCanvas::present`] — texture-to-texture copy from the bridge's
//!    3D render target (`default_render_target`) into the surface texture,
//!    then present. Pure GPU, no readback. The render-target format follows
//!    the surface's negotiated format (see [`WebGpuCanvas::new`]).
//!
//! ## Honest bounds
//!
//! - The surface format is negotiated: `Rgba8UnormSrgb` is preferred (the
//!   bridge's native render-target format), otherwise the surface's first
//!   offered format wins and the bridge's render target is re-allocated to
//!   match before the triangle pipeline is built. `new` fails loudly if the
//!   surface offers no formats at all.
//! - `SignalFence` is an error here: Track A never issues fences
//!   (`VIRTIO_GPU_FLAG_FENCE` unset), so one indicates a bug upstream.
//! - `Unsupported` is an error, never swallowed.

use pathn_contracts::device::GpuCmd;
use u8_gpu_host::{dispatch, HostAction};
use virtio_gpu_bridge::VirtioGpuBridge;
use wasm_bindgen::JsCast;

/// Golden-triangle canvas size.
pub const CANVAS_W: u32 = 640;
/// Golden-triangle canvas height.
pub const CANVAS_H: u32 = 480;

const GL_VERTEX_SHADER: u32 = 0x8B31;
const GL_FRAGMENT_SHADER: u32 = 0x8B30;
const GL_ARRAY_BUFFER: u32 = 0x8892;
const GL_STATIC_DRAW: u32 = 0x88E4;
const GL_FLOAT: u32 = 0x1406;

/// Pass-through vertex shader (GLSL ES 1.00; the bridge's sanitizer rewrites
/// `attribute` for naga).
const VERT_SRC: &str =
    "attribute vec2 pos;\nvoid main() {\n    gl_Position = vec4(pos, 0.0, 1.0);\n}\n";
/// Solid-white fragment shader.
const FRAG_SRC: &str = "void main() {\n    gl_FragColor = vec4(1.0, 1.0, 1.0, 1.0);\n}\n";

/// One triangle in NDC: (-0.5,-0.5), (0.5,-0.5), (0.0,0.5).
const TRIANGLE_VERTS: [f32; 6] = [-0.5, -0.5, 0.5, -0.5, 0.0, 0.5];

/// Encode a `VIRTIO_GPU_CMD_SUBMIT_3D` binary packet around a raw opcode
/// stream: 24-byte ctrl hdr (type `0x0207`, flags 0, fence 0, `ctx_id`,
/// pad 0) + `size` u32 + pad u32 + stream bytes.
fn encode_submit_3d(ctx_id: u32, commands: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(32 + commands.len());
    p.extend_from_slice(&0x0207u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&0u64.to_le_bytes());
    p.extend_from_slice(&ctx_id.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&(commands.len() as u32).to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(commands);
    p
}

/// Upload the golden-triangle program + vertex buffer through the bridge's
/// public GLES2 entry points. Must run once before any `Submit3DWire`: the
/// mini opcode stream carries no shader state, and the bridge errors
/// (`"Program {} not found"`) when no program is bound.
fn setup_triangle(gl: &mut gles2wgpu::GlContext) -> Result<(), String> {
    let vs = gl.gl_create_shader(GL_VERTEX_SHADER);
    gl.gl_shader_source(vs, VERT_SRC);
    gl.gl_compile_shader(vs)?;
    let fs = gl.gl_create_shader(GL_FRAGMENT_SHADER);
    gl.gl_shader_source(fs, FRAG_SRC);
    gl.gl_compile_shader(fs)?;
    let prog = gl.gl_create_program();
    gl.gl_attach_shader(prog, vs);
    gl.gl_attach_shader(prog, fs);
    gl.gl_link_program(prog)?;
    gl.gl_use_program(prog);

    let bufs = gl.gl_gen_buffers(1);
    let vbo = *bufs.first().ok_or("gl_gen_buffers returned none")?;
    gl.gl_bind_buffer(GL_ARRAY_BUFFER, vbo);
    let mut bytes = Vec::with_capacity(TRIANGLE_VERTS.len() * 4);
    for v in TRIANGLE_VERTS {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    gl.gl_buffer_data(GL_ARRAY_BUFFER, &bytes, GL_STATIC_DRAW);
    // Also marks the attrib enabled (context.rs).
    gl.gl_vertex_attrib_pointer(0, 2, GL_FLOAT, false, 0, 0);
    Ok(())
}

/// The real WebGPU canvas adapter: owns the bridge and the canvas surface.
pub struct WebGpuCanvas {
    bridge: VirtioGpuBridge,
    surface: wgpu::Surface<'static>,
}

impl WebGpuCanvas {
    /// Boot the bridge, set up the triangle pipeline, bind `canvas_id` to a
    /// WebGPU surface. Fails loudly (never half-initialised) when the canvas
    /// is missing, WebGPU is unavailable, or the surface offers no formats.
    ///
    /// Format negotiation: the bridge natively renders `Rgba8UnormSrgb`;
    /// when the canvas surface does not offer it (SwiftShader offers
    /// `[Rgba8Unorm, Bgra8Unorm, Rgba16Float]`), the first offered format is
    /// used instead and the bridge's render target is re-allocated to match
    /// *before* the triangle pipeline is built, so every GLES2 render pass
    /// the bridge creates later uses the negotiated format.
    pub async fn new(canvas_id: &str) -> Result<Self, String> {
        let window = web_sys::window().ok_or("WebGpuCanvas: no window")?;
        let document = window.document().ok_or("WebGpuCanvas: no document")?;
        let el = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| format!("WebGpuCanvas: no canvas #{canvas_id}"))?;
        let canvas: web_sys::HtmlCanvasElement = el
            .dyn_into()
            .map_err(|_| format!("WebGpuCanvas: #{canvas_id} is not a canvas"))?;
        canvas.set_width(CANVAS_W);
        canvas.set_height(CANVAS_H);

        let mut bridge = VirtioGpuBridge::new(CANVAS_W, CANVAS_H)
            .await
            .map_err(|e| format!("WebGpuCanvas: bridge init failed: {e}"))?;

        // Surface from a fresh instance; configured with the bridge's device
        // (same navigator.gpu — the web backend passes the device straight
        // to the canvas context).
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| format!("WebGpuCanvas: create_surface failed: {e:?}"))?;
        // Format negotiation only needs *an* adapter for this surface.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("WebGpuCanvas: no WebGPU adapter for the canvas")?;
        let caps = surface.get_capabilities(&adapter);
        // Prefer the bridge's native sRGB format; fall back to the surface's
        // first offered format (SwiftShader has no sRGB surface format).
        let format = if caps.formats.contains(&wgpu::TextureFormat::Rgba8UnormSrgb) {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            caps.formats
                .first()
                .copied()
                .ok_or("WebGpuCanvas: canvas surface offers no texture formats")?
        };
        if format != bridge.gl_context.surface_format {
            // Re-allocate the 3D render target in the negotiated format
            // before setup_triangle builds the pipeline: every render pass
            // gles2wgpu creates keys off `surface_format`.
            bridge.gl_context.default_render_target.allocate_2d(
                &bridge.gl_context.device,
                CANVAS_W,
                CANVAS_H,
                format,
            );
            bridge.gl_context.surface_format = format;
        }
        setup_triangle(&mut bridge.gl_context)?;
        surface.configure(
            &bridge.gl_context.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
                format,
                width: CANVAS_W,
                height: CANVAS_H,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![],
            },
        );

        Ok(Self { bridge, surface })
    }

    /// U8 dispatch + execution of one [`GpuCmd`].
    pub fn execute_cmd(&mut self, cmd: &GpuCmd) -> Result<(), String> {
        self.execute(dispatch(cmd))
    }

    /// Execute one [`HostAction`] on the bridge.
    pub fn execute(&mut self, action: HostAction) -> Result<(), String> {
        match action {
            HostAction::Submit3DWire { ctx_id, commands } => {
                let packet = encode_submit_3d(ctx_id, &commands);
                let _resp = self.bridge.process_binary_wire_command(&packet);
                Ok(())
            }
            HostAction::BridgeCommand(cmd) => {
                let _resp = self.bridge.execute_command(cmd);
                Ok(())
            }
            HostAction::SignalFence { id } => Err(format!(
                "WebGpuCanvas: unexpected SignalFence({id}); Track A never \
                 issues fences — refusing to silently drop it"
            )),
            HostAction::Unsupported { reason, .. } => Err(format!(
                "WebGpuCanvas: unsupported GPU command ({reason}) — no \
                 backend path exists"
            )),
        }
    }

    /// Blit the bridge's 3D render target to the canvas surface and present.
    /// Both are the negotiated surface format at 640×480, so this is a plain
    /// texture-to-texture copy on the GPU.
    ///
    /// NOTE (headless-Chromium Dawn bug, verified 2026-09-30): calling
    /// `get_current_texture()` on a canvas WebGPU context loses the device
    /// (`"A valid external Instance reference no longer exists"`); every
    /// later `map_async` fails. Headless acceptance therefore uses
    /// `execute_pending` + `readback` and never presents. This stays for
    /// headed browsers, where the surface path works.
    pub fn present(&mut self) -> Result<(), String> {
        let gl = &mut self.bridge.gl_context;
        let src = gl
            .default_render_target
            .wgpu_texture
            .as_ref()
            .ok_or("WebGpuCanvas: no 3D render target")?;
        let output = self
            .surface
            .get_current_texture()
            .map_err(|e| format!("WebGpuCanvas: get_current_texture: {e:?}"))?;
        let mut encoder = gl
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pathn-canvas-blit"),
            });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: src,
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
                width: CANVAS_W,
                height: CANVAS_H,
                depth_or_array_layers: 1,
            },
        );
        gl.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Read back the bridge's 3D render target — the exact texture the
    /// guest's `Submit3D` drew into — as 640×480 RGBA8 bytes.
    ///
    /// This is the ground-truth pixel assertion path: headless Chromium's
    /// `--screenshot` does not composite WebGPU canvases (verified: a
    /// pure-JS WebGPU triangle submits cleanly yet screenshots black), and
    /// `canvas.toDataURL()` reads transparent black after `present()`.
    /// A GPU→CPU copy of the real render target is unaffected by either
    /// compositor quirk. 640·4 = 2560 bytes/row satisfies WebGPU's 256-byte
    /// `bytes_per_row` alignment.
    pub async fn readback(&self) -> Result<js_sys::Uint8Array, String> {
        let gl = &self.bridge.gl_context;
        let src = gl
            .default_render_target
            .wgpu_texture
            .as_ref()
            .ok_or("WebGpuCanvas: no 3D render target")?;
        let bytes_per_row: u32 = CANVAS_W * 4;
        let staging = gl.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pathn-readback"),
            size: (bytes_per_row * CANVAS_H) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gl
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pathn-readback-copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(CANVAS_H),
                },
            },
            wgpu::Extent3d {
                width: CANVAS_W,
                height: CANVAS_H,
                depth_or_array_layers: 1,
            },
        );
        gl.queue.submit(std::iter::once(encoder.finish()));
        // On the web backend the map callback is driven by the browser's own
        // WebGPU implementation — no manual device.poll needed.
        let slice = staging.slice(..);
        let mapped = js_sys::Promise::new(&mut |resolve, reject| {
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let js = wasm_bindgen::JsValue::NULL;
                match r {
                    Ok(()) => {
                        let _ = resolve.call0(&js);
                    }
                    Err(e) => {
                        let _ =
                            reject.call1(&js, &wasm_bindgen::JsValue::from_str(&format!("{e:?}")));
                    }
                }
            });
        });
        wasm_bindgen_futures::JsFuture::from(mapped)
            .await
            .map_err(|e| format!("WebGpuCanvas: readback map failed: {e:?}"))?;
        let data = slice.get_mapped_range();
        let out = js_sys::Uint8Array::new_with_length(data.len() as u32);
        out.copy_from(&data);
        drop(data);
        staging.unmap();
        Ok(out)
    }
}
