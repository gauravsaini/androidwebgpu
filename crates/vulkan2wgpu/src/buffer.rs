use crate::types::*;
use wgpu::util::DeviceExt;

const MAX_SAFE_BUFFER_SIZE: u64 = 256 * 1024 * 1024; // 256 MB safety cap

#[derive(Debug, Clone)]
pub struct VkMemoryRange {
    pub offset: u64,
    pub size: u64,
}

pub struct VkDeviceMemory {
    pub id: u64,
    pub size: u64,
    pub memory_type_index: u32,
    pub property_flags: u32,
    pub shadow_buffer: Vec<u8>,
    pub dirty_ranges: Vec<VkMemoryRange>,
    pub bound_buffer_id: Option<u64>,
}

impl VkDeviceMemory {
    pub fn try_new(id: u64, size: u64, memory_type_index: u32, property_flags: u32) -> Result<Self, i32> {
        if size > MAX_SAFE_BUFFER_SIZE {
            return Err(VK_ERROR_OUT_OF_DEVICE_MEMORY);
        }
        let size_usize = size as usize;
        let mut shadow_buffer = Vec::new();
        if shadow_buffer.try_reserve_exact(size_usize).is_err() {
            return Err(VK_ERROR_OUT_OF_DEVICE_MEMORY);
        }
        shadow_buffer.resize(size_usize, 0);

        Ok(Self {
            id,
            size,
            memory_type_index,
            property_flags,
            shadow_buffer,
            dirty_ranges: Vec::new(),
            bound_buffer_id: None,
        })
    }

    pub fn new(id: u64, size: u64, memory_type_index: u32, property_flags: u32) -> Self {
        Self::try_new(id, size, memory_type_index, property_flags).unwrap_or_else(|_| {
            let capped = size.min(1024 * 1024);
            Self {
                id,
                size: capped,
                memory_type_index,
                property_flags,
                shadow_buffer: vec![0u8; capped as usize],
                dirty_ranges: Vec::new(),
                bound_buffer_id: None,
            }
        })
    }

    pub fn write_memory(&mut self, offset: u64, data: &[u8]) -> Result<(), i32> {
        let start = offset as usize;
        let end = start + data.len();
        if end > self.shadow_buffer.len() {
            return Err(VK_ERROR_OUT_OF_DEVICE_MEMORY);
        }
        self.shadow_buffer[start..end].copy_from_slice(data);
        self.dirty_ranges.push(VkMemoryRange {
            offset,
            size: data.len() as u64,
        });
        Ok(())
    }

    pub fn read_memory(&self, offset: u64, size: u64) -> Result<&[u8], i32> {
        let start = offset as usize;
        let end = start + size as usize;
        if end > self.shadow_buffer.len() {
            return Err(VK_ERROR_OUT_OF_DEVICE_MEMORY);
        }
        Ok(&self.shadow_buffer[start..end])
    }

    pub fn flush_range(&mut self, offset: u64, size: u64) {
        self.dirty_ranges.push(VkMemoryRange { offset, size });
    }
}

pub struct VkBuffer {
    pub id: u64,
    pub size: u64,
    pub usage: u32,
    pub memory_id: Option<u64>,
    pub memory_offset: u64,
    pub wgpu_buffer: Option<wgpu::Buffer>,
}

impl VkBuffer {
    pub fn new(id: u64, size: u64, usage: u32) -> Self {
        let capped_size = size.min(MAX_SAFE_BUFFER_SIZE);
        Self {
            id,
            size: capped_size,
            usage,
            memory_id: None,
            memory_offset: 0,
            wgpu_buffer: None,
        }
    }

    pub fn create_wgpu_buffer(&mut self, device: &wgpu::Device, initial_data: Option<&[u8]>) {
        let mut wgpu_usage = if self.usage == VK_BUFFER_USAGE_TRANSFER_DST_BIT {
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST
        } else {
            wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC
        };

        if (self.usage & VK_BUFFER_USAGE_VERTEX_BUFFER_BIT) != 0 {
            wgpu_usage |= wgpu::BufferUsages::VERTEX;
        }
        if (self.usage & VK_BUFFER_USAGE_INDEX_BUFFER_BIT) != 0 {
            wgpu_usage |= wgpu::BufferUsages::INDEX;
        }
        if (self.usage & VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT) != 0 {
            wgpu_usage |= wgpu::BufferUsages::UNIFORM;
        }
        if (self.usage & VK_BUFFER_USAGE_STORAGE_BUFFER_BIT) != 0 {
            wgpu_usage |= wgpu::BufferUsages::STORAGE;
        }
        if (self.usage & VK_BUFFER_USAGE_INDIRECT_BUFFER_BIT) != 0 {
            wgpu_usage |= wgpu::BufferUsages::INDIRECT;
        }

        let w_buf = if let Some(data) = initial_data {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("VkBuffer_{}", self.id)),
                contents: data,
                usage: wgpu_usage,
            })
        } else {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("VkBuffer_{}", self.id)),
                size: self.size.max(16),
                usage: wgpu_usage,
                mapped_at_creation: false,
            })
        };

        self.wgpu_buffer = Some(w_buf);
    }
}
