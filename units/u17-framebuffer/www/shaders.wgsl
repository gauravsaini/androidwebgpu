// WGSL Fullscreen Quad Texture Shaders for Framebuffer Display
// Sampling a 640x480 rgba8unorm texture onto canvas

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var pos = array<vec2f, 6>(
        vec2f(-1.0, -1.0),
        vec2f( 1.0, -1.0),
        vec2f(-1.0,  1.0),
        vec2f(-1.0,  1.0),
        vec2f( 1.0, -1.0),
        vec2f( 1.0,  1.0)
    );

    var uv = array<vec2f, 6>(
        vec2f(0.0, 1.0),
        vec2f(1.0, 1.0),
        vec2f(0.0, 0.0),
        vec2f(0.0, 0.0),
        vec2f(1.0, 1.0),
        vec2f(1.0, 0.0)
    );

    var out: VertexOutput;
    out.position = vec4f(pos[vertex_index], 0.0, 1.0);
    out.uv = uv[vertex_index];
    return out;
}

@group(0) @binding(0) var fb_sampler: sampler;
@group(0) @binding(1) var fb_texture: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(fb_texture, fb_sampler, in.uv);
}
