pub struct GlBuffer {
    pub id: u32,
    pub target: u32,
    pub size: usize,
    pub data: Vec<u8>,
    pub wgpu_buffer: Option<wgpu::Buffer>,
    pub dirty: bool,
}

impl GlBuffer {
    pub fn new(id: u32, target: u32) -> Self {
        Self {
            id,
            target,
            size: 0,
            data: Vec::new(),
            wgpu_buffer: None,
            dirty: false,
        }
    }

    pub fn set_data(&mut self, data: &[u8]) {
        self.data = data.to_vec();
        self.size = data.len();
        self.dirty = true;
    }

    pub fn sync_to_wgpu(&mut self, device: &wgpu::Device, usage: wgpu::BufferUsages) {
        if self.dirty || self.wgpu_buffer.is_none() {
            if !self.data.is_empty() {
                use wgpu::util::DeviceExt;
                let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("GL_Buffer_{}", self.id)),
                    contents: &self.data,
                    usage,
                });
                self.wgpu_buffer = Some(buffer);
                self.dirty = false;
            }
        }
    }
}
