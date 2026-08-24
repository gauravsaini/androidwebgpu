use naga::back::wgsl::{self, WriterFlags};
use naga::front::glsl::{Frontend, Options};
use naga::ShaderStage;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShaderError {
    #[error("GLSL compile error: {0}")]
    GlslParse(String),
    #[error("WGSL emit error: {0}")]
    WgslEmit(String),
}

pub struct ShaderTranslator {
    options_vert: Options,
    options_frag: Options,
}

impl ShaderTranslator {
    pub fn new() -> Self {
        Self {
            options_vert: Options::from(ShaderStage::Vertex),
            options_frag: Options::from(ShaderStage::Fragment),
        }
    }

    pub fn sanitize_glsl(&self, glsl_source: &str, stage: ShaderStage) -> String {
        let mut lines = Vec::new();
        let mut has_version = false;
        let has_gl_fragcolor = glsl_source.contains("gl_FragColor");
        let mut uniform_members = Vec::new();

        for line in glsl_source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("#version") {
                lines.push("#version 450".to_string());
                has_version = true;
            } else if trimmed.starts_with("#extension") {
                lines.push(format!("// {}", line));
            } else if trimmed.starts_with("uniform ") && !trimmed.contains("sampler") && !trimmed.contains("{") {
                // Collect loose uniform variables to pack into std140 uniform block at binding 2
                let member = trimmed.trim_start_matches("uniform ").trim();
                uniform_members.push(format!("    {}", member));
            } else {
                let mut processed = line
                    .replace("precision mediump float;", "")
                    .replace("precision highp float;", "")
                    .replace("precision lowp float;", "")
                    .replace("precision mediump int;", "")
                    .replace("precision highp int;", "")
                    .replace("precision lowp int;", "")
                    .replace("samplerExternalOES", "sampler2D")
                    .replace("texture2D(", "texture(");

                if stage == ShaderStage::Vertex {
                    if processed.trim_start().starts_with("attribute ") {
                        processed = processed.replacen("attribute ", "in ", 1);
                    } else if processed.trim_start().starts_with("varying ") {
                        processed = processed.replacen("varying ", "out ", 1);
                    }
                } else if stage == ShaderStage::Fragment {
                    if processed.trim_start().starts_with("varying ") {
                        processed = processed.replacen("varying ", "in ", 1);
                    }
                    if has_gl_fragcolor {
                        processed = processed.replace("gl_FragColor", "_out_gl_fragcolor");
                    }
                }

                lines.push(processed);
            }
        }

        if !has_version {
            lines.insert(0, "#version 450".to_string());
        }

        if stage == ShaderStage::Fragment && has_gl_fragcolor {
            lines.insert(1, "layout(location = 0) out vec4 _out_gl_fragcolor;".to_string());
        }

        // Insert packed std140 uniform block at binding 2 if uniforms were present
        if !uniform_members.is_empty() {
            let block = format!(
                "layout(std140, set = 0, binding = 2) uniform UniformBlock {{\n{}\n}};",
                uniform_members.join("\n")
            );
            lines.insert(if stage == ShaderStage::Fragment && has_gl_fragcolor { 2 } else { 1 }, block);
        }

        lines.join("\n")
    }

    pub fn translate_vertex(&self, glsl_source: &str) -> Result<String, ShaderError> {
        self.translate(glsl_source, ShaderStage::Vertex)
    }

    pub fn translate_fragment(&self, glsl_source: &str) -> Result<String, ShaderError> {
        self.translate(glsl_source, ShaderStage::Fragment)
    }

    pub fn translate(&self, glsl_source: &str, stage: ShaderStage) -> Result<String, ShaderError> {
        let mut frontend = Frontend::default();
        let opts = match stage {
            ShaderStage::Vertex => &self.options_vert,
            ShaderStage::Fragment => &self.options_frag,
            _ => &self.options_vert,
        };

        let sanitized_source = self.sanitize_glsl(glsl_source, stage);

        let module = frontend
            .parse(opts, &sanitized_source)
            .map_err(|errors| ShaderError::GlslParse(format!("{:?}", errors)))?;

        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| ShaderError::GlslParse(format!("Validation failed: {:?}", e)))?;

        let wgsl = wgsl::write_string(&module, &info, WriterFlags::empty())
            .map_err(|e| ShaderError::WgslEmit(format!("{:?}", e)))?;

        Ok(wgsl)
    }
}

impl Default for ShaderTranslator {
    fn default() -> Self {
        Self::new()
    }
}
