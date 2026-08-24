pub struct GlFramebuffer {
    pub id: u32,
    pub color_attachment_texture_id: Option<u32>,
    pub depth_attachment_texture_id: Option<u32>,
    pub width: u32,
    pub height: u32,
}

impl GlFramebuffer {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            color_attachment_texture_id: None,
            depth_attachment_texture_id: None,
            width: 0,
            height: 0,
        }
    }

    pub fn attach_texture_2d(&mut self, attachment: u32, texture_id: u32) {
        match attachment {
            0x8CE0 => { // GL_COLOR_ATTACHMENT0
                self.color_attachment_texture_id = Some(texture_id);
            }
            0x8D00 => { // GL_DEPTH_ATTACHMENT
                self.depth_attachment_texture_id = Some(texture_id);
            }
            _ => {}
        }
    }
}
