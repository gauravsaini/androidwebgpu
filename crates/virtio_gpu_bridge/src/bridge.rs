use crate::binary::{BinaryWireParser, DecodedVirtioCommand};
use crate::command::{CommandResponse, GpuCommand};
use crate::protocol::*;
use gles2wgpu::GlContext;
use std::collections::HashMap;

pub struct HostResource2D {
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub texture_id: u32,
    pub backing_data: Vec<u8>,
}

pub struct Scanout {
    pub scanout_id: u32,
    pub resource_id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub fb_data: Vec<u8>,
    pub damage_rect: Option<[u32; 4]>,
}

pub struct VirtioGpuBridge {
    pub gl_context: GlContext,
    pub resources: HashMap<u32, HostResource2D>,
    pub scanouts: HashMap<u32, Scanout>,
    pub next_texture_id: u32,
}

impl VirtioGpuBridge {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let gl_context = GlContext::new(width, height).await?;
        Ok(Self {
            gl_context,
            resources: HashMap::new(),
            scanouts: HashMap::new(),
            next_texture_id: 1,
        })
    }

    pub fn process_binary_wire_command(&mut self, packet: &[u8]) -> Vec<u8> {
        let decoded = match BinaryWireParser::parse_command(packet) {
            Ok(cmd) => cmd,
            Err(_) => {
                return BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                    0,
                    0,
                )
            }
        };

        match decoded {
            DecodedVirtioCommand::GetDisplayInfo(hdr) => {
                let mut resp = VirtioGpuRespDisplayInfo {
                    hdr: VirtioGpuCtrlHdr {
                        type_: VIRTIO_GPU_RESP_OK_DISPLAY_INFO,
                        flags: 0,
                        fence_id: hdr.fence_id,
                        ctx_id: 0,
                        padding: 0,
                    },
                    pmodes: [VirtioGpuDisplayOne::default(); 16],
                };
                resp.pmodes[0].enabled = 1;
                resp.pmodes[0].r = VirtioGpuRect {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 720,
                };
                BinaryWireParser::encode_response(&resp)
            }
            DecodedVirtioCommand::ResourceCreate2d(cmd) => {
                let tex_id = self.next_texture_id;
                self.next_texture_id += 1;

                let size = (cmd.width * cmd.height * 4) as usize;
                self.resources.insert(
                    cmd.resource_id,
                    HostResource2D {
                        resource_id: cmd.resource_id,
                        format: cmd.format,
                        width: cmd.width,
                        height: cmd.height,
                        texture_id: tex_id,
                        backing_data: vec![0u8; size],
                    },
                );

                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::ResourceCreate3d(cmd) => {
                let tex_id = self.next_texture_id;
                self.next_texture_id += 1;

                let size = (cmd.width * cmd.height * cmd.depth.max(1) * 4) as usize;
                self.resources.insert(
                    cmd.resource_id,
                    HostResource2D {
                        resource_id: cmd.resource_id,
                        format: cmd.format,
                        width: cmd.width,
                        height: cmd.height,
                        texture_id: tex_id,
                        backing_data: vec![0u8; size],
                    },
                );

                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::ResourceUnref(hdr, res_id) => {
                if let Some(res) = self.resources.remove(&res_id) {
                    self.gl_context.gl_delete_textures(&[res.texture_id]);
                }
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::SetScanout(cmd) => {
                let fb_size = (cmd.r.width * cmd.r.height * 4) as usize;
                self.scanouts.insert(
                    cmd.scanout_id,
                    Scanout {
                        scanout_id: cmd.scanout_id,
                        resource_id: cmd.resource_id,
                        x: cmd.r.x,
                        y: cmd.r.y,
                        width: cmd.r.width,
                        height: cmd.r.height,
                        fb_data: vec![0u8; fb_size],
                        damage_rect: Some([cmd.r.x, cmd.r.y, cmd.r.width, cmd.r.height]),
                    },
                );
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::ResourceFlush(cmd) => {
                if let Some(res) = self.resources.get(&cmd.resource_id) {
                    let bpp = 4;
                    let res_w = res.width as usize;
                    let flush_x = cmd.r.x as usize;
                    let flush_y = cmd.r.y as usize;
                    let flush_w = cmd.r.width as usize;
                    let flush_h = cmd.r.height as usize;

                    for scanout in self.scanouts.values_mut() {
                        if scanout.resource_id == cmd.resource_id {
                            let scan_w = scanout.width as usize;
                            let scan_h = scanout.height as usize;
                            let max_h = flush_h.min(scan_h.saturating_sub(flush_y));
                            let max_w = flush_w.min(scan_w.saturating_sub(flush_x));

                            for row in 0..max_h {
                                let src_off = ((flush_y + row) * res_w + flush_x) * bpp;
                                let dst_off = ((flush_y + row) * scan_w + flush_x) * bpp;
                                let row_bytes = max_w * bpp;
                                if src_off + row_bytes <= res.backing_data.len()
                                    && dst_off + row_bytes <= scanout.fb_data.len()
                                {
                                    scanout.fb_data[dst_off..dst_off + row_bytes]
                                        .copy_from_slice(&res.backing_data[src_off..src_off + row_bytes]);
                                }
                            }
                            scanout.damage_rect = Some([cmd.r.x, cmd.r.y, cmd.r.width, cmd.r.height]);
                        }
                    }
                }
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    if cmd.hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0 {
                        VIRTIO_GPU_FLAG_FENCE
                    } else {
                        0
                    },
                )
            }
            DecodedVirtioCommand::TransferToHost2d(cmd, payload) => {
                if let Some(res) = self.resources.get_mut(&cmd.resource_id) {
                    let tex_id = res.texture_id;
                    let bpp = 4;
                    let full_width = res.width as usize;
                    let sub_w = cmd.r.width as usize;
                    let sub_h = cmd.r.height as usize;
                    let dst_x = cmd.r.x as usize;
                    let dst_y = cmd.r.y as usize;

                    // Copy subrect into backing_data
                    if !payload.is_empty() {
                        for row in 0..sub_h {
                            let src_offset = row * sub_w * bpp;
                            let dst_offset = ((dst_y + row) * full_width + dst_x) * bpp;
                            if src_offset + sub_w * bpp <= payload.len()
                                && dst_offset + sub_w * bpp <= res.backing_data.len()
                            {
                                res.backing_data[dst_offset..dst_offset + sub_w * bpp]
                                    .copy_from_slice(&payload[src_offset..src_offset + sub_w * bpp]);
                            }
                        }
                    }

                    self.gl_context.gl_bind_texture(0x0DE1, tex_id);
                    self.gl_context.gl_tex_image_2d(
                        0x0DE1,
                        0,
                        0x1908,
                        res.width,
                        res.height,
                        0,
                        0x1908,
                        0x1401,
                        Some(&res.backing_data),
                    );

                    BinaryWireParser::encode_header_response(
                        VIRTIO_GPU_RESP_OK_NODATA,
                        cmd.hdr.fence_id,
                        0,
                    )
                } else {
                    BinaryWireParser::encode_header_response(
                        VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
                        cmd.hdr.fence_id,
                        0,
                    )
                }
            }
            DecodedVirtioCommand::TransferToHost3d(cmd, payload) => {
                if let Some(res) = self.resources.get_mut(&cmd.resource_id) {
                    let tex_id = res.texture_id;
                    let bpp = 4;
                    let full_width = res.width as usize;
                    let sub_w = cmd.box_.w as usize;
                    let sub_h = cmd.box_.h as usize;
                    let dst_x = cmd.box_.x as usize;
                    let dst_y = cmd.box_.y as usize;

                    if !payload.is_empty() {
                        for row in 0..sub_h {
                            let src_offset = row * sub_w * bpp;
                            let dst_offset = ((dst_y + row) * full_width + dst_x) * bpp;
                            if src_offset + sub_w * bpp <= payload.len()
                                && dst_offset + sub_w * bpp <= res.backing_data.len()
                            {
                                res.backing_data[dst_offset..dst_offset + sub_w * bpp]
                                    .copy_from_slice(&payload[src_offset..src_offset + sub_w * bpp]);
                            }
                        }
                    }

                    self.gl_context.gl_bind_texture(0x0DE1, tex_id);
                    self.gl_context.gl_tex_image_2d(
                        0x0DE1,
                        0,
                        0x1908,
                        res.width,
                        res.height,
                        0,
                        0x1908,
                        0x1401,
                        Some(&res.backing_data),
                    );

                    BinaryWireParser::encode_header_response(
                        VIRTIO_GPU_RESP_OK_NODATA,
                        cmd.hdr.fence_id,
                        0,
                    )
                } else {
                    BinaryWireParser::encode_header_response(
                        VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
                        cmd.hdr.fence_id,
                        0,
                    )
                }
            }
            DecodedVirtioCommand::CtxCreate(cmd) => {
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::CtxDestroy(hdr) => {
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::CtxAttachResource(cmd) => {
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::CtxDetachResource(cmd) => {
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    0,
                )
            }
            DecodedVirtioCommand::GetCapsetInfo(cmd) => {
                let mut resp = VirtioGpuRespCapsetInfo {
                    hdr: VirtioGpuCtrlHdr {
                        type_: VIRTIO_GPU_RESP_OK_CAPSET_INFO,
                        flags: 0,
                        fence_id: cmd.hdr.fence_id,
                        ctx_id: 0,
                        padding: 0,
                    },
                    capset_id: cmd.capset_index,
                    capset_max_version: 2,
                    capset_max_size: 512,
                    padding: 0,
                };
                resp.hdr.type_ = VIRTIO_GPU_RESP_OK_CAPSET_INFO;
                bytemuck::bytes_of(&resp).to_vec()
            }
            DecodedVirtioCommand::Submit3d(cmd, buf) => {
                self.execute_submit_3d(&buf);
                BinaryWireParser::encode_header_response(
                    VIRTIO_GPU_RESP_OK_NODATA,
                    cmd.hdr.fence_id,
                    if cmd.hdr.flags & VIRTIO_GPU_FLAG_FENCE != 0 {
                        VIRTIO_GPU_FLAG_FENCE
                    } else {
                        0
                    },
                )
            }
            _ => BinaryWireParser::encode_header_response(
                VIRTIO_GPU_RESP_ERR_UNSPEC,
                0,
                0,
            ),
        }
    }

    fn execute_submit_3d(&mut self, buf: &[u8]) {
        let mut cursor = 0;
        while cursor + 8 <= buf.len() {
            let opcode = u32::from_le_bytes(buf[cursor..cursor + 4].try_into().unwrap());
            let len = u32::from_le_bytes(buf[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
            cursor += 8;

            if cursor + len > buf.len() {
                break;
            }

            let cmd_payload = &buf[cursor..cursor + len];
            match opcode {
                0x01 => {
                    // CLEAR (mask: u32, r: f32, g: f32, b: f32, a: f32)
                    if cmd_payload.len() >= 20 {
                        let mask = u32::from_le_bytes(cmd_payload[0..4].try_into().unwrap());
                        let r = f32::from_le_bytes(cmd_payload[4..8].try_into().unwrap());
                        let g = f32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let b = f32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        let a = f32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        self.gl_context.gl_clear_color(r, g, b, a);
                        self.gl_context.gl_clear(mask);
                    }
                }
                0x02 => {
                    // DRAW_ARRAYS (mode: u32, first: u32, count: u32)
                    if cmd_payload.len() >= 12 {
                        let mode = u32::from_le_bytes(cmd_payload[0..4].try_into().unwrap());
                        let first = u32::from_le_bytes(cmd_payload[4..8].try_into().unwrap());
                        let count = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        self.gl_context.gl_draw_arrays(mode, first, count);
                    }
                }
                0x03 => {
                    // DRAW_ELEMENTS (mode: u32, count: u32, type: u32, offset: u32)
                    if cmd_payload.len() >= 16 {
                        let mode = u32::from_le_bytes(cmd_payload[0..4].try_into().unwrap());
                        let count = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let type_ = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let offset = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap()) as usize;
                        self.gl_context.gl_draw_elements(mode, count, type_, offset);
                    }
                }
                0x04 => {
                    // VIEWPORT (x: i32, y: i32, w: u32, h: u32)
                    if cmd_payload.len() >= 16 {
                        let x = i32::from_le_bytes(cmd_payload[0..4].try_into().unwrap());
                        let y = i32::from_le_bytes(cmd_payload[4..8].try_into().unwrap());
                        let w = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let h = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        self.gl_context.gl_viewport(x, y, w, h);
                    }
                }
                _ => {}
            }

            cursor += len;
        }
    }

    pub fn execute_command(&mut self, command: GpuCommand) -> CommandResponse {
        match command {
            GpuCommand::CreateResource2D {
                resource_id,
                format,
                width,
                height,
            } => {
                let tex_id = self.next_texture_id;
                self.next_texture_id += 1;

                self.resources.insert(
                    resource_id,
                    HostResource2D {
                        resource_id,
                        format,
                        width,
                        height,
                        texture_id: tex_id,
                        backing_data: vec![0u8; (width * height * 4) as usize],
                    },
                );

                CommandResponse {
                    status: VIRTIO_GPU_RESP_OK_NODATA,
                    fence_id: 0,
                    payload: Vec::new(),
                }
            }

            GpuCommand::TransferToHost2D {
                resource_id,
                x,
                y,
                width,
                height,
                data,
                ..
            } => {
                if let Some(res) = self.resources.get_mut(&resource_id) {
                    let tex_id = res.texture_id;
                    let full_width = res.width as usize;
                    let bpp = 4;

                    for row in 0..height as usize {
                        let src_offset = row * (width as usize) * bpp;
                        let dst_offset = (((y as usize) + row) * full_width + (x as usize)) * bpp;
                        if src_offset + (width as usize) * bpp <= data.len()
                            && dst_offset + (width as usize) * bpp <= res.backing_data.len()
                        {
                            res.backing_data[dst_offset..dst_offset + (width as usize) * bpp]
                                .copy_from_slice(&data[src_offset..src_offset + (width as usize) * bpp]);
                        }
                    }

                    self.gl_context.gl_bind_texture(0x0DE1, tex_id);
                    self.gl_context.gl_tex_image_2d(
                        0x0DE1,
                        0,
                        0x1908,
                        res.width,
                        res.height,
                        0,
                        0x1908,
                        0x1401,
                        Some(&res.backing_data),
                    );

                    CommandResponse {
                        status: VIRTIO_GPU_RESP_OK_NODATA,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                } else {
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                }
            }

            GpuCommand::SetScanout {
                scanout_id,
                resource_id,
                x,
                y,
                width,
                height,
            } => {
                self.scanouts.insert(
                    scanout_id,
                    Scanout {
                        scanout_id,
                        resource_id,
                        x,
                        y,
                        width,
                        height,
                        fb_data: vec![0u8; (width * height * 4) as usize],
                        damage_rect: Some([x, y, width, height]),
                    },
                );
                CommandResponse {
                    status: VIRTIO_GPU_RESP_OK_NODATA,
                    fence_id: 0,
                    payload: Vec::new(),
                }
            }

            GpuCommand::ResourceFlush { resource_id, .. } => {
                if let Some(res) = self.resources.get(&resource_id) {
                    for scanout in self.scanouts.values_mut() {
                        if scanout.resource_id == resource_id {
                            let min_len = scanout.fb_data.len().min(res.backing_data.len());
                            scanout.fb_data[0..min_len].copy_from_slice(&res.backing_data[0..min_len]);
                        }
                    }
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_OK_NODATA,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                } else {
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                }
            }

            GpuCommand::UnrefResource { resource_id } => {
                if let Some(res) = self.resources.remove(&resource_id) {
                    self.gl_context.gl_delete_textures(&[res.texture_id]);
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_OK_NODATA,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                } else {
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                }
            }

            _ => CommandResponse {
                status: VIRTIO_GPU_RESP_OK_NODATA,
                fence_id: 0,
                payload: Vec::new(),
            },
        }
    }

    pub fn get_scanout_framebuffer(&self, scanout_id: u32) -> Option<Vec<u8>> {
        self.scanouts.get(&scanout_id).map(|s| s.fb_data.clone())
    }

    pub fn get_scanout_damage(&self, scanout_id: u32) -> Option<[u32; 4]> {
        self.scanouts.get(&scanout_id).and_then(|s| s.damage_rect)
    }

    pub fn clear_scanout_damage(&mut self, scanout_id: u32) {
        if let Some(s) = self.scanouts.get_mut(&scanout_id) {
            s.damage_rect = None;
        }
    }
}
