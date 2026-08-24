#[derive(Clone, Debug)]
pub struct VkClearColorValue {
    pub float32: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct VkClearDepthStencilValue {
    pub depth: f32,
    pub stencil: u32,
}

#[derive(Clone, Debug)]
pub enum VkClearValue {
    Color(VkClearColorValue),
    DepthStencil(VkClearDepthStencilValue),
}

#[derive(Clone, Debug)]
pub struct VkRenderingAttachmentInfo {
    pub image_view_id: u64,
    pub load_op: u32,  // 0 = Load, 1 = Clear, 2 = DontCare
    pub store_op: u32, // 0 = Store, 1 = DontCare
    pub clear_value: Option<VkClearValue>,
}

#[derive(Clone, Debug)]
pub enum VkCommand {
    BeginRendering {
        color_attachments: Vec<VkRenderingAttachmentInfo>,
        depth_attachment: Option<VkRenderingAttachmentInfo>,
    },
    EndRendering,
    BindPipeline {
        pipeline_id: u64,
    },
    BindDescriptorSets {
        first_set: u32,
        descriptor_set_ids: Vec<u64>,
        dynamic_offsets: Vec<u32>,
    },
    PushConstants {
        offset: u32,
        data: Vec<u8>,
    },
    BindVertexBuffers {
        first_binding: u32,
        buffer_ids: Vec<u64>,
        offsets: Vec<u64>,
    },
    BindIndexBuffer {
        buffer_id: u64,
        offset: u64,
        index_type: u32, // 0 = Uint16, 1 = Uint32
    },
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    },
    Dispatch {
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    },
    SetViewport {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        min_depth: f32,
        max_depth: f32,
    },
    SetScissor {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    CopyImageToBuffer {
        image_id: u64,
        buffer_id: u64,
        width: u32,
        height: u32,
    },
    PipelineBarrier,
}

#[derive(Clone, Debug)]
pub struct VkCommandBuffer {
    pub id: u64,
    pub is_recording: bool,
    pub commands: Vec<VkCommand>,
}

impl VkCommandBuffer {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            is_recording: false,
            commands: Vec::new(),
        }
    }

    pub fn begin(&mut self) {
        self.commands.clear();
        self.is_recording = true;
    }

    pub fn end(&mut self) {
        self.is_recording = false;
    }

    pub fn record(&mut self, cmd: VkCommand) {
        if self.is_recording {
            self.commands.push(cmd);
        }
    }
}
