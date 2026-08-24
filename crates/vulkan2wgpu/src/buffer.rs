use crate::types::*;
use wgpu::util::DeviceExt;

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
    pub mapped_ptr: Option<*mut u8>,
    pub dirty_ranges: Vec<VkMemoryRange>,
    pub bound_buffer_id: Option<u64>,
}

unsafe impl Send for VkDeviceMemory {}
unsafe impl Sync for VkDeviceMemory {}

impl VkDeviceMemory {
    pub fn new(id: u64, size: u64, memory_type_index: u32, property_flags: u32) -> Self {
        Self {
            id,
            size,
            memory_type_index,
            property_flags,
            shadow_buffer: vec![0u8; size as usize],
            mapped_ptr: None,
            dirty_ranges: Vec::new(),
            bound_buffer_id: None,
        }
    }

    pub fn map_memory(&mut self, offset: u64, size: u64) -> Result<*mut u8, i32> {
        if offset + size > self.size {
            return Err(VK_ERROR_OUT_OF_DEVICE_MEMORY);
        }
        let ptr = unsafe { self.shadow_buffer.as_mut_ptr().add(offset as usize) };
        self.mapped_ptr = Some(ptr);
        Ok(ptr)
    }

    pub fn unmap_memory(&mut self) {
        self.mapped_ptr = None;
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
        Self {
            id,
            size,
            usage,
            memory_id: None,
            memory_offset: 0,
            wgpu_buffer: None,
        }
    }

    pub fn create_wgpu_buffer(&mut self, device: &wgpu::Device, initial_data: Option<&[u8]>) {
        let mut wgpu_usage = wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC;

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
