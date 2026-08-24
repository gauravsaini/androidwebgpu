use naga::back::wgsl::{self, WriterFlags};
use naga::front::spv::{Frontend, Options};
use thiserror::Error;

const SPIRV_MAGIC_LE: u32 = 0x07230203;
const SPIRV_MAGIC_BE: u32 = 0x03022307;

#[derive(Error, Debug)]
pub enum SpirvError {
    #[error("SPIR-V parse error: {0}")]
    SpirvParse(String),
    #[error("WGSL emit error: {0}")]
    WgslEmit(String),
    #[error("Invalid SPIR-V words length or alignment")]
    InvalidAlignment,
    #[error("Invalid SPIR-V magic number: {0:#010x}")]
    InvalidMagic(u32),
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
        if words.is_empty() {
            return Err(SpirvError::InvalidAlignment);
        }

        let magic = words[0];
        if magic != SPIRV_MAGIC_LE && magic != SPIRV_MAGIC_BE {
            return Err(SpirvError::InvalidMagic(magic));
        }

        let frontend = Frontend::new(words.iter().copied(), &self.options);
        let mut module = frontend
            .parse()
            .map_err(|e| SpirvError::SpirvParse(format!("{:?}", e)))?;

        // Find highest existing group index to avoid collision
        let mut max_group = 0u32;
        for (_, var) in module.global_variables.iter() {
            if let Some(binding) = &var.binding {
                if binding.group >= max_group {
                    max_group = binding.group + 1;
                }
            }
        }
        let push_group = max_group.max(3);

        // Remap PushConstant variables to uniform buffers at synthetic push_group binding 0
        for (_, var) in module.global_variables.iter_mut() {
            if var.space == naga::AddressSpace::PushConstant {
                var.space = naga::AddressSpace::Uniform;
                var.binding = Some(naga::ResourceBinding {
                    group: push_group,
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
        if bytes.len() < 4 || bytes.len() % 4 != 0 {
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
