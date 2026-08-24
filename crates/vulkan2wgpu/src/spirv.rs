use naga::back::wgsl::{self, WriterFlags};
use naga::front::spv::{Frontend, Options};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpirvError {
    #[error("SPIR-V parse error: {0}")]
    SpirvParse(String),
    #[error("WGSL emit error: {0}")]
    WgslEmit(String),
    #[error("Invalid SPIR-V words length or alignment")]
    InvalidAlignment,
}

pub struct SpirvTranslator {
    options: Options,
}

impl SpirvTranslator {
    pub fn new() -> Self {
        Self {
            options: Options::default(),
        }
    }

    pub fn translate_spirv_words(&self, words: &[u32]) -> Result<String, SpirvError> {
        let frontend = Frontend::new(words.iter().copied(), &self.options);
        let mut module = frontend
            .parse()
            .map_err(|e| SpirvError::SpirvParse(format!("{:?}", e)))?;

        // Remap PushConstant variables to uniform buffers at group 3 binding 0
        for (_, var) in module.global_variables.iter_mut() {
            if var.space == naga::AddressSpace::PushConstant {
                var.space = naga::AddressSpace::Uniform;
                var.binding = Some(naga::ResourceBinding {
                    group: 3,
                    binding: 0,
                });
            }
        }

        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| SpirvError::SpirvParse(format!("SPIR-V validation error: {:?}", e)))?;

        let wgsl = wgsl::write_string(&module, &info, WriterFlags::empty())
            .map_err(|e| SpirvError::WgslEmit(format!("{:?}", e)))?;

        Ok(wgsl)
    }

    pub fn translate_spirv_bytes(&self, bytes: &[u8]) -> Result<String, SpirvError> {
        if bytes.len() % 4 != 0 {
            return Err(SpirvError::InvalidAlignment);
        }

        let words: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        self.translate_spirv_words(&words)
    }
}

impl Default for SpirvTranslator {
    fn default() -> Self {
        Self::new()
    }
}
