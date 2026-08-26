use crate::binary::{BinaryWireParser, DecodedVirtioCommand};
use crate::command::{CommandResponse, GpuCommand};
use crate::protocol::*;
use aidl_compat::IBinder;
use binder_handle_bridge::HandleBridge;
use binder_routing::RoutingPolicy;
use gles2wgpu::GlContext;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use surfaceflinger_gpu_service::SurfaceComposerService;
use virtio_binder::VirtioBinderDevice;
use vulkan2wgpu::VkDevice;

pub use inputflinger_rs::InputManagerService;

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
    pub vk_device: Option<VkDevice>,
    pub resources: HashMap<u32, HostResource2D>,
    pub scanouts: HashMap<u32, Scanout>,
    pub next_texture_id: u32,
    pub binder_device: Arc<VirtioBinderDevice>,
    pub handle_bridge: Arc<Mutex<HandleBridge>>,
    pub routing_policy: RoutingPolicy,
    pub surface_composer: Option<Arc<SurfaceComposerService>>,
}

impl VirtioGpuBridge {
    pub async fn new(width: u32, height: u32) -> Result<Self, String> {
        let gl_context = GlContext::new(width, height).await?;
        let dev = Arc::clone(&gl_context.device);
        let q = Arc::clone(&gl_context.queue);
        let vk_device = Some(VkDevice::with_device_and_queue(Arc::clone(&dev), Arc::clone(&q)));

        let binder_device = Arc::new(VirtioBinderDevice::new());
        let handle_bridge = Arc::new(Mutex::new(HandleBridge::new()));
        let mut routing_policy = RoutingPolicy::new_default_local();

        // 1. SurfaceComposerService (Handle 1) - unified device sharing gl_context.device & queue
        let surface_composer = {
            let sf = Arc::new(SurfaceComposerService::with_handle_bridge(
                Arc::clone(&dev),
                Arc::clone(&q),
                width,
                height,
                Arc::clone(&handle_bridge),
            ));

            binder_device.register_service(1, Arc::clone(&sf) as Arc<dyn IBinder>);
            handle_bridge
                .lock()
                .unwrap()
                .register_service_with_handle(
                    100,
                    1,
                    SurfaceComposerService::DESCRIPTOR,
                    Arc::clone(&sf) as Arc<dyn IBinder>,
                )
                .ok();
            routing_policy.allow_host_offload(SurfaceComposerService::DESCRIPTOR);
            routing_policy.allow_host_offload(surfaceflinger_gpu_service::GraphicBufferProducerService::DESCRIPTOR);

            Some(sf)
        };

        // 2. InputManagerService from inputflinger_rs (Handle 2)
        let input_service = Arc::new(inputflinger_rs::InputManagerService::new());
        binder_device.register_service(2, Arc::clone(&input_service) as Arc<dyn IBinder>);
        handle_bridge
            .lock()
            .unwrap()
            .register_service_with_handle(
                100,
                2,
                inputflinger_rs::IINPUT_MANAGER_DESCRIPTOR,
                Arc::clone(&input_service) as Arc<dyn IBinder>,
            )
            .ok();
        routing_policy.allow_host_offload(inputflinger_rs::IINPUT_MANAGER_DESCRIPTOR);

        // 3. WindowManagerService from wms_rs (Handle 3)
        let wms_service = if let Some(ref sf) = surface_composer {
            let surface_bridge = Arc::new(wms_rs::SurfaceBridge::with_compositor(Arc::clone(sf)));
            Arc::new(wms_rs::WindowManagerService::with_surface_bridge(surface_bridge))
        } else {
            Arc::new(wms_rs::WindowManagerService::new())
        };
        binder_device.register_service(3, Arc::clone(&wms_service) as Arc<dyn IBinder>);
        handle_bridge
            .lock()
            .unwrap()
            .register_service_with_handle(
                100,
                3,
                wms_rs::IWINDOW_MANAGER_DESCRIPTOR,
                Arc::clone(&wms_service) as Arc<dyn IBinder>,
            )
            .ok();
        routing_policy.allow_host_offload(wms_rs::IWINDOW_MANAGER_DESCRIPTOR);
        routing_policy.allow_host_offload(wms_rs::IWINDOW_SESSION_DESCRIPTOR);

        // 4. PackageManagerService from pms_rs (Handle 5)
        let pms_service = Arc::new(pms_rs::PackageManagerService::new());
        for (pkg_name, app_name, act_name) in &[
            ("org.fdroid.fdroid", "F-Droid", "org.fdroid.fdroid.views.main.MainActivity"),
            ("org.mozilla.firefox", "Firefox", "org.mozilla.firefox.App"),
            ("com.android.chrome", "Chrome", "com.google.android.apps.chrome.Main"),
            ("com.android.settings", "Settings", "com.android.settings.SettingsActivity"),
            ("com.android.terminal", "Terminal", "com.android.terminal.TerminalActivity"),
            ("com.android.files", "Files", "com.android.documentsui.files.FilesActivity"),
            ("com.android.glbenchmark", "3D Arcade", "com.android.glbenchmark.MainActivity"),
        ] {
            let mut pkg = pms_rs::types::PackageInfo {
                package_name: pkg_name.to_string(),
                version_code: 1,
                version_name: Some("14.0.0".to_string()),
                ..Default::default()
            };
            let app_info = pms_rs::types::ApplicationInfo {
                package_name: pkg_name.to_string(),
                name: Some(app_name.to_string()),
                label: Some(app_name.to_string()),
                enabled: true,
                ..Default::default()
            };
            pkg.application_info = Some(app_info);
            let act = pms_rs::types::ActivityInfo {
                package_name: pkg_name.to_string(),
                name: act_name.to_string(),
                label: Some(app_name.to_string()),
                exported: true,
                enabled: true,
                intent_filters: vec![pms_rs::types::IntentFilter {
                    actions: vec!["android.intent.action.MAIN".to_string()],
                    categories: vec!["android.intent.category.LAUNCHER".to_string()],
                    ..Default::default()
                }],
                ..Default::default()
            };
            pkg.activities.push(act);
            pms_service.install_package_info(pkg, None);
        }
        binder_device.register_service(5, Arc::clone(&pms_service) as Arc<dyn IBinder>);
        handle_bridge
            .lock()
            .unwrap()
            .register_service_with_handle(
                100,
                5,
                pms_rs::IPACKAGE_MANAGER_DESCRIPTOR,
                Arc::clone(&pms_service) as Arc<dyn IBinder>,
            )
            .ok();
        routing_policy.allow_host_offload(pms_rs::IPACKAGE_MANAGER_DESCRIPTOR);

        // 5. ActivityManagerService from ams_rs (Handle 4)
        let pms_client = Arc::new(pms_rs::PackageManagerClient::new(
            aidl_compat::pointer::SpIBinder::from_arc(Arc::clone(&pms_service) as Arc<dyn IBinder>),
        ));
        let zygote = Arc::new(zygote_client::socket::ZygoteClient::new_path("/dev/socket/zygote"));
        let ams_service = Arc::new(ams_rs::ActivityManagerService::new(pms_client, zygote));
        binder_device.register_service(4, Arc::clone(&ams_service) as Arc<dyn IBinder>);
        handle_bridge
            .lock()
            .unwrap()
            .register_service_with_handle(
                100,
                4,
                ams_rs::IACTIVITY_MANAGER_DESCRIPTOR,
                Arc::clone(&ams_service) as Arc<dyn IBinder>,
            )
            .ok();
        routing_policy.allow_host_offload(ams_rs::IACTIVITY_MANAGER_DESCRIPTOR);

        // 6. GraphicBufferProducer (Handles 10, 20, 30)
        for &h in &[10u32, 20u32, 30u32] {
            let producer = Arc::new(surfaceflinger_gpu_service::GraphicBufferProducerService::new(
                h as u64,
                Arc::clone(&dev),
                Arc::clone(&q),
            ));
            if let Some(ref sf) = surface_composer {
                sf.register_producer(h as u64, &format!("Surface_{}", h), Arc::clone(&producer), width, height);
            }
            binder_device.register_service(h, Arc::clone(&producer) as Arc<dyn IBinder>);
            handle_bridge
                .lock()
                .unwrap()
                .register_service_with_handle(
                    100,
                    h,
                    surfaceflinger_gpu_service::GraphicBufferProducerService::DESCRIPTOR,
                    Arc::clone(&producer) as Arc<dyn IBinder>,
                )
                .ok();
        }
        routing_policy.allow_host_offload(surfaceflinger_gpu_service::GraphicBufferProducerService::DESCRIPTOR);

        Ok(Self {
            gl_context,
            vk_device,
            resources: HashMap::new(),
            scanouts: HashMap::new(),
            next_texture_id: 1,
            binder_device,
            handle_bridge,
            routing_policy,
            surface_composer,
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

                        let r_u8 = (r * 255.0).clamp(0.0, 255.0) as u8;
                        let g_u8 = (g * 255.0).clamp(0.0, 255.0) as u8;
                        let b_u8 = (b * 255.0).clamp(0.0, 255.0) as u8;
                        let a_u8 = (a * 255.0).clamp(0.0, 255.0) as u8;

                        for res in self.resources.values_mut() {
                            for chunk in res.backing_data.chunks_exact_mut(4) {
                                chunk[0] = r_u8;
                                chunk[1] = g_u8;
                                chunk[2] = b_u8;
                                chunk[3] = a_u8;
                            }
                        }
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
                        let count = u32::from_le_bytes(cmd_payload[4..8].try_into().unwrap());
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
                VIRTGPU_VK_CMD_QUEUE_SUBMIT => {
                    // num_cbs: u32, [cb_id: u64]...
                    if cmd_payload.len() >= 4 {
                        let num_cbs = u32::from_le_bytes(cmd_payload[0..4].try_into().unwrap()) as usize;
                        let mut cb_ids = Vec::with_capacity(num_cbs);
                        let mut off = 4;
                        for _ in 0..num_cbs {
                            if off + 8 <= cmd_payload.len() {
                                let cb_id = u64::from_le_bytes(cmd_payload[off..off + 8].try_into().unwrap());
                                cb_ids.push(cb_id);
                                off += 8;
                            }
                        }
                        if let Some(vk) = &mut self.vk_device {
                            let _ = vk.vk_queue_submit(&cb_ids);
                        }
                    }
                }
                VIRTGPU_VK_CMD_BEGIN_RENDERING => {
                    // cb_id: u64, color_view_id: u64, depth_view_id: u64, r: f32, g: f32, b: f32, a: f32, clear_depth: f32
                    if cmd_payload.len() >= 44 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let color_view = u64::from_le_bytes(cmd_payload[8..16].try_into().unwrap());
                        let depth_view = u64::from_le_bytes(cmd_payload[16..24].try_into().unwrap());
                        let r = f32::from_le_bytes(cmd_payload[24..28].try_into().unwrap());
                        let g = f32::from_le_bytes(cmd_payload[28..32].try_into().unwrap());
                        let b = f32::from_le_bytes(cmd_payload[32..36].try_into().unwrap());
                        let a = f32::from_le_bytes(cmd_payload[36..40].try_into().unwrap());
                        let clear_depth = f32::from_le_bytes(cmd_payload[40..44].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_begin_rendering(
                                cb_id,
                                if color_view == 0 { None } else { Some(color_view) },
                                if depth_view == 0 { None } else { Some(depth_view) },
                                [r, g, b, a],
                                clear_depth,
                            );
                        }
                    }
                }
                VIRTGPU_VK_CMD_END_RENDERING => {
                    // cb_id: u64
                    if cmd_payload.len() >= 8 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_end_rendering(cb_id);
                        }
                    }
                }
                VIRTGPU_VK_CMD_BIND_PIPELINE => {
                    // cb_id: u64, pipeline_id: u64
                    if cmd_payload.len() >= 16 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let pipeline_id = u64::from_le_bytes(cmd_payload[8..16].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_bind_pipeline(cb_id, pipeline_id);
                        }
                    }
                }
                VIRTGPU_VK_CMD_BIND_VERTEX_BUFFERS => {
                    // cb_id: u64, first_binding: u32, count: u32, [buf_id: u64, offset: u64]...
                    if cmd_payload.len() >= 16 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let first_binding = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let count = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap()) as usize;
                        let mut buffer_ids = Vec::with_capacity(count);
                        let mut offsets = Vec::with_capacity(count);
                        let mut off = 16;
                        for _ in 0..count {
                            if off + 16 <= cmd_payload.len() {
                                let b_id = u64::from_le_bytes(cmd_payload[off..off + 8].try_into().unwrap());
                                let b_off = u64::from_le_bytes(cmd_payload[off + 8..off + 16].try_into().unwrap());
                                buffer_ids.push(b_id);
                                offsets.push(b_off);
                                off += 16;
                            }
                        }
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_bind_vertex_buffers(cb_id, first_binding, buffer_ids, offsets);
                        }
                    }
                }
                VIRTGPU_VK_CMD_BIND_INDEX_BUFFER => {
                    // cb_id: u64, buffer_id: u64, offset: u64, index_type: u32
                    if cmd_payload.len() >= 28 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let buffer_id = u64::from_le_bytes(cmd_payload[8..16].try_into().unwrap());
                        let offset = u64::from_le_bytes(cmd_payload[16..24].try_into().unwrap());
                        let index_type = u32::from_le_bytes(cmd_payload[24..28].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_bind_index_buffer(cb_id, buffer_id, offset, index_type);
                        }
                    }
                }
                VIRTGPU_VK_CMD_DRAW => {
                    // cb_id: u64, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32
                    if cmd_payload.len() >= 24 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let vertex_count = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let instance_count = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        let first_vertex = u32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        let first_instance = u32::from_le_bytes(cmd_payload[20..24].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_draw(cb_id, vertex_count, instance_count, first_vertex, first_instance);
                        }
                    }
                }
                VIRTGPU_VK_CMD_DRAW_INDEXED => {
                    // cb_id: u64, index_count: u32, instance_count: u32, first_index: u32, vertex_offset: i32, first_instance: u32
                    if cmd_payload.len() >= 28 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let index_count = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let instance_count = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        let first_index = u32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        let vertex_offset = i32::from_le_bytes(cmd_payload[20..24].try_into().unwrap());
                        let first_instance = u32::from_le_bytes(cmd_payload[24..28].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_draw_indexed(cb_id, index_count, instance_count, first_index, vertex_offset, first_instance);
                        }
                    }
                }
                VIRTGPU_VK_CMD_SET_VIEWPORT => {
                    // cb_id: u64, x: f32, y: f32, width: f32, height: f32, min_depth: f32, max_depth: f32
                    if cmd_payload.len() >= 32 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let x = f32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let y = f32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        let width = f32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        let height = f32::from_le_bytes(cmd_payload[20..24].try_into().unwrap());
                        let min_depth = f32::from_le_bytes(cmd_payload[24..28].try_into().unwrap());
                        let max_depth = f32::from_le_bytes(cmd_payload[28..32].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_set_viewport(cb_id, x, y, width, height, min_depth, max_depth);
                        }
                    }
                }
                VIRTGPU_VK_CMD_SET_SCISSOR => {
                    // cb_id: u64, x: i32, y: i32, width: u32, height: u32
                    if cmd_payload.len() >= 24 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let x = i32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let y = i32::from_le_bytes(cmd_payload[12..16].try_into().unwrap());
                        let width = u32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        let height = u32::from_le_bytes(cmd_payload[20..24].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_set_scissor(cb_id, x, y, width, height);
                        }
                    }
                }
                VIRTGPU_VK_CMD_BIND_DESCRIPTOR_SETS => {
                    // cb_id: u64, layout_id: u64, first_set: u32, count: u32, [ds_id: u64]...
                    if cmd_payload.len() >= 24 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let layout_id = u64::from_le_bytes(cmd_payload[8..16].try_into().unwrap());
                        let first_set = u32::from_le_bytes(cmd_payload[16..20].try_into().unwrap());
                        let count = u32::from_le_bytes(cmd_payload[20..24].try_into().unwrap()) as usize;
                        let mut ds_ids = Vec::with_capacity(count);
                        let mut off = 24;
                        for _ in 0..count {
                            if off + 8 <= cmd_payload.len() {
                                let ds_id = u64::from_le_bytes(cmd_payload[off..off + 8].try_into().unwrap());
                                ds_ids.push(ds_id);
                                off += 8;
                            }
                        }
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_bind_descriptor_sets(cb_id, layout_id, first_set, &ds_ids);
                        }
                    }
                }
                VIRTGPU_VK_CMD_PUSH_CONSTANTS => {
                    // cb_id: u64, offset: u32, size: u32, [data: u8]...
                    if cmd_payload.len() >= 16 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let offset = u32::from_le_bytes(cmd_payload[8..12].try_into().unwrap());
                        let size = u32::from_le_bytes(cmd_payload[12..16].try_into().unwrap()) as usize;
                        if 16 + size <= cmd_payload.len() {
                            let data = &cmd_payload[16..16 + size];
                            if let Some(vk) = &mut self.vk_device {
                                vk.vk_cmd_push_constants(cb_id, offset, data);
                            }
                        }
                    }
                }
                VIRTGPU_VK_CMD_COPY_IMAGE_TO_BUFFER => {
                    // cb_id: u64, src_image_id: u64, dst_buffer_id: u64, width: u32, height: u32
                    if cmd_payload.len() >= 32 {
                        let cb_id = u64::from_le_bytes(cmd_payload[0..8].try_into().unwrap());
                        let src_image_id = u64::from_le_bytes(cmd_payload[8..16].try_into().unwrap());
                        let dst_buffer_id = u64::from_le_bytes(cmd_payload[16..24].try_into().unwrap());
                        let width = u32::from_le_bytes(cmd_payload[24..28].try_into().unwrap());
                        let height = u32::from_le_bytes(cmd_payload[28..32].try_into().unwrap());
                        if let Some(vk) = &mut self.vk_device {
                            vk.vk_cmd_copy_image_to_buffer(cb_id, src_image_id, dst_buffer_id, width, height);
                        }
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

            GpuCommand::TransferToHost3D {
                resource_id,
                x,
                y,
                z: _,
                width,
                height,
                depth: _,
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

            GpuCommand::SubmitVulkan { command_buffer_ids } => {
                if let Some(vk) = &mut self.vk_device {
                    let _ = vk.vk_queue_submit(&command_buffer_ids);
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_OK_NODATA,
                        fence_id: 0,
                        payload: Vec::new(),
                    }
                } else {
                    CommandResponse {
                        status: VIRTIO_GPU_RESP_ERR_UNSPEC,
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

            GpuCommand::ResourceFlush {
                resource_id,
                x,
                y,
                width,
                height,
            } => {
                if let Some(res) = self.resources.get(&resource_id) {
                    let bpp = 4;
                    let res_w = res.width as usize;
                    let flush_x = x as usize;
                    let flush_y = y as usize;
                    let flush_w = width as usize;
                    let flush_h = height as usize;

                    for scanout in self.scanouts.values_mut() {
                        if scanout.resource_id == resource_id {
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
                            scanout.damage_rect = Some([x, y, width, height]);
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

    pub fn process_binder_packet(&self, packet: &[u8]) -> Vec<u8> {
        match self.binder_device.process_packet(packet) {
            Ok(resp) => resp,
            Err(_) => {
                virtio_binder::VirtioBinderResponse::error(
                    0,
                    aidl_compat::STATUS_BAD_VALUE,
                    binder_rt::wire::BR_FAILED_REPLY as i32,
                )
                .serialize()
            }
        }
    }
}
