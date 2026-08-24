pub mod buffer;
pub mod context;
pub mod framebuffer;
pub mod pipeline;
pub mod shader;
pub mod texture;

pub use context::GlContext;
pub use shader::ShaderTranslator;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_translation_vertex() {
        let translator = ShaderTranslator::new();
        let glsl_vert = r#"#version 300 es
layout(location = 0) in vec3 aPosition;
void main() {
    gl_Position = vec4(aPosition, 1.0);
}
"#;
        let result = translator.translate_vertex(glsl_vert);
        assert!(result.is_ok(), "Failed to translate vertex shader: {:?}", result.err());
        let wgsl = result.unwrap();
        assert!(wgsl.contains("main") || wgsl.contains("@vertex"));
    }

    #[test]
    fn test_shader_translation_fragment() {
        let translator = ShaderTranslator::new();
        let glsl_frag = r#"#version 300 es
precision mediump float;
out vec4 fragColor;
void main() {
    fragColor = vec4(1.0, 0.0, 0.0, 1.0);
}
"#;
        let result = translator.translate_fragment(glsl_frag);
        assert!(result.is_ok(), "Failed to translate fragment shader: {:?}", result.err());
        let wgsl = result.unwrap();
        assert!(wgsl.contains("main") || wgsl.contains("@fragment"));
    }

    #[test]
    fn test_gl_buffer_set_data() {
        let mut buf = buffer::GlBuffer::new(1, 0x8892);
        let sample = [1u8, 2, 3, 4];
        buf.set_data(&sample);
        assert_eq!(buf.data, sample);
        assert_eq!(buf.size, 4);
        assert!(buf.dirty);
    }
}
