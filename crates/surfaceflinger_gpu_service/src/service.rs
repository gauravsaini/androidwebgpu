//! Host-side SurfaceFlinger GPU Compositor Service (`android.gui.ISurfaceComposer`).

use crate::buffer_queue::GraphicBufferProducerService;
use crate::layer_translator::{ComposerState, LayerState, LayerTranslator};
use aidl_compat::{
    DeathRecipient, IBinder, Parcel, Parcelable, Remotable, Result as AidlResult, Status,
    TransactionCode, TransactionFlags, DUMP_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION,
    STATUS_BAD_VALUE, STATUS_NAME_NOT_FOUND,
};
use binder_handle_bridge::HandleBridge;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use webgpu_compositor::{BlendMode, CompositionLayer, WebGpuCompositor};
use webgpu_swapchain::WebGpuSwapchain;

/// AOSP `ISurfaceComposer` transaction opcodes.
pub mod isurfacecomposer_codes {
    pub const CREATE_CONNECTION: u32 = 1002;
    pub const CREATE_SURFACE: u32 = 1006;
    pub const DESTROY_SURFACE: u32 = 1007;
    pub const GET_BUILT_IN_DISPLAY: u32 = 1008;
    pub const GET_DISPLAY_INFO: u32 = 1010;
    pub const SET_TRANSACTION_STATE: u32 = 1020;
    pub const BOOT_FINISHED: u32 = 1025;
}

/// Errors returned by the SurfaceFlinger GPU service.
#[derive(Debug, Error)]
pub enum CompositorServiceError {
    #[error("Surface ID {0} not found")]
    SurfaceNotFound(u64),
    #[error("GPU swapchain readback error: {0}")]
    SwapchainError(String),
    #[error("AIDL error: {0}")]
    Aidl(String),
}

/// Display metrics and capability information returned by `GET_DISPLAY_INFO`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub density: f32,
    pub orientation: u32,
    pub secure: bool,
}

use serde::{Deserialize, Serialize};

impl Default for DisplayInfo {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps: 120.0,
            density: 2.0,
            orientation: 0,
            secure: false,
        }
    }
}

impl Parcelable for DisplayInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel.write_u32(self.width).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.height).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.fps).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_f32(self.density).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_u32(self.orientation).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel.write_bool(self.secure).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.width = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.height = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.fps = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.density = parcel.read_f32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.orientation = parcel.read_u32(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.secure = parcel.read_bool(offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

/// Metadata and handle references for an active offloaded surface.
pub struct SurfaceHandle {
    pub surface_id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub producer: Arc<GraphicBufferProducerService>,
}

/// Core host-side service receiving offloaded Binder IPC transactions and rendering via WebGPU.
pub struct SurfaceComposerService {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    compositor: Arc<Mutex<WebGpuCompositor>>,
    swapchain: Arc<Mutex<WebGpuSwapchain>>,
    handle_bridge: Option<Arc<Mutex<HandleBridge>>>,
    surfaces: Arc<Mutex<HashMap<u64, SurfaceHandle>>>,
    next_surface_id: AtomicU64,
    boot_finished: AtomicBool,
    display_width: u32,
    display_height: u32,
    frame_count: AtomicU64,
}

unsafe impl Send for SurfaceComposerService {}
unsafe impl Sync for SurfaceComposerService {}

impl SurfaceComposerService {
    /// Canonical AIDL interface descriptor for SurfaceComposer.
    pub const DESCRIPTOR: &'static str = "android.gui.ISurfaceComposer";

    /// Create a new SurfaceComposer service instance.
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Self {
        let compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let swapchain = WebGpuSwapchain::new(&device, width, height, wgpu::TextureFormat::Rgba8Unorm);

        Self {
            device,
            queue,
            compositor: Arc::new(Mutex::new(compositor)),
            swapchain: Arc::new(Mutex::new(swapchain)),
            handle_bridge: None,
            surfaces: Arc::new(Mutex::new(HashMap::new())),
            next_surface_id: AtomicU64::new(1),
            boot_finished: AtomicBool::new(false),
            display_width: width,
            display_height: height,
            frame_count: AtomicU64::new(0),
        }
    }

    /// Construct service with an attached `HandleBridge` for cross-VM handle management.
    pub fn with_handle_bridge(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
        handle_bridge: Arc<Mutex<HandleBridge>>,
    ) -> Self {
        let mut svc = Self::new(device, queue, width, height);
        svc.handle_bridge = Some(handle_bridge);
        svc
    }

    /// Create a new surface layer and attach an `IGraphicBufferProducer`.
    pub fn create_surface(
        &self,
        name: &str,
        width: u32,
        height: u32,
        _flags: u32,
    ) -> Result<SurfaceHandle, CompositorServiceError> {
        let surface_id = self.next_surface_id.fetch_add(1, Ordering::SeqCst);
        let producer = Arc::new(GraphicBufferProducerService::new(
            surface_id,
            Arc::clone(&self.device),
            Arc::clone(&self.queue),
        ));

        let initial_layer = CompositionLayer::new_color(
            surface_id,
            name,
            [-1.0, -1.0, 2.0, 2.0],
            0,
            [0.0, 0.0, 0.0, 1.0],
        );

        {
            let mut comp = self.compositor.lock().unwrap();
            comp.add_or_update_layer(initial_layer);
        }

        let handle = SurfaceHandle {
            surface_id,
            name: name.to_string(),
            width,
            height,
            producer,
        };

        self.surfaces.lock().unwrap().insert(surface_id, SurfaceHandle {
            surface_id: handle.surface_id,
            name: handle.name.clone(),
            width: handle.width,
            height: handle.height,
            producer: Arc::clone(&handle.producer),
        });

        Ok(handle)
    }

    /// Update multi-layer transaction states in a single batch.
    pub fn set_transaction_state(
        &self,
        updates: Vec<ComposerState>,
        _flags: u32,
    ) -> Result<(), CompositorServiceError> {
        let mut comp = self.compositor.lock().unwrap();
        let surfaces = self.surfaces.lock().unwrap();

        for update in updates {
            let surface_id = update.surface_id;
            let producer_view = surfaces
                .get(&surface_id)
                .and_then(|s| s.producer.acquire_latest_texture_view());

            if let Some(existing_layer) = comp.layers.get_mut(&surface_id) {
                LayerTranslator::apply_state_update(
                    existing_layer,
                    &update.state,
                    self.display_width,
                    self.display_height,
                );
                if producer_view.is_some() {
                    existing_layer.texture_view = producer_view;
                }
            } else {
                let layer = LayerTranslator::translate_to_composition_layer(
                    &update.state,
                    self.display_width,
                    self.display_height,
                    producer_view,
                );
                comp.add_or_update_layer(layer);
            }
        }
        Ok(())
    }

    /// Set solid color quad on a surface directly.
    pub fn set_surface_color(
        &self,
        surface_id: u64,
        color: [f32; 4],
        bounds: [f32; 4],
        z_order: i32,
    ) -> Result<(), CompositorServiceError> {
        let mut state = LayerState::new(surface_id, "ColorLayer");
        state.set_color(color);
        state.set_bounds_ndc(bounds);
        state.set_z_order(z_order);
        state.set_blend_mode(BlendMode::Premultiplied);

        let composer_state = ComposerState::new(surface_id, state);
        self.set_transaction_state(vec![composer_state], 0)
    }

    /// Execute composition pass and present frame to swapchain.
    pub fn compose_and_present(&self) -> Result<u64, CompositorServiceError> {
        let mut comp = self.compositor.lock().unwrap();
        let mut sc = self.swapchain.lock().unwrap();

        // Refresh texture views from producers if new buffers are queued
        {
            let surfaces = self.surfaces.lock().unwrap();
            for (id, surface) in surfaces.iter() {
                if let Some(view) = surface.producer.acquire_latest_texture_view() {
                    if let Some(layer) = comp.layers.get_mut(id) {
                        layer.texture_view = Some(view);
                    }
                }
            }
        }

        let target_view = sc.get_current_texture_view();
        comp.compose(
            &self.device,
            &self.queue,
            target_view,
            Some(wgpu::Color::BLACK),
        );

        let frame_id = sc.present();
        self.frame_count.store(frame_id, Ordering::SeqCst);
        Ok(frame_id)
    }

    /// Read back the rendered frame pixels from the active swapchain target.
    pub async fn readback_pixels(&self) -> Result<Vec<u8>, CompositorServiceError> {
        let (output_buffer, width, height, bytes_per_row) = {
            let mut comp = self.compositor.lock().unwrap();
            let sc = self.swapchain.lock().unwrap();

            // Refresh texture views from producers if new buffers are queued
            {
                let surfaces = self.surfaces.lock().unwrap();
                for (id, surface) in surfaces.iter() {
                    if let Some(view) = surface.producer.acquire_latest_texture_view() {
                        if let Some(layer) = comp.layers.get_mut(id) {
                            layer.texture_view = Some(view);
                        }
                    }
                }
            }

            let target_view = sc.get_current_texture_view();
            comp.compose(
                &self.device,
                &self.queue,
                target_view,
                Some(wgpu::Color::BLACK),
            );

            let u32_size = std::mem::size_of::<u32>() as u32;
            let bytes_per_row = (u32_size * sc.width + 255) & !255;
            let buffer_size = (bytes_per_row * sc.height) as u64;

            let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SurfaceComposer Readback Buffer"),
                size: buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SurfaceComposer Readback Encoder"),
            });

            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: sc.get_current_texture(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &output_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(sc.height),
                    },
                },
                wgpu::Extent3d {
                    width: sc.width,
                    height: sc.height,
                    depth_or_array_layers: 1,
                },
            );

            self.queue.submit(Some(encoder.finish()));
            (output_buffer, sc.width, sc.height, bytes_per_row)
        }; // MutexGuards on comp and sc are dropped here

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });

        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| CompositorServiceError::SwapchainError(format!("Channel error: {:?}", e)))?
            .map_err(|e| CompositorServiceError::SwapchainError(format!("Buffer map error: {:?}", e)))?;

        let view = buffer_slice.get_mapped_range();
        let mut dense_pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let row_start = (y * bytes_per_row) as usize;
            let row_end = row_start + (width * 4) as usize;
            dense_pixels.extend_from_slice(&view[row_start..row_end]);
        }
        drop(view);
        output_buffer.unmap();
        Ok(dense_pixels)
    }

    /// Destroy an existing surface layer.
    pub fn destroy_surface(&self, surface_id: u64) -> Result<(), CompositorServiceError> {
        let removed = self.surfaces.lock().unwrap().remove(&surface_id);
        if removed.is_some() {
            let mut comp = self.compositor.lock().unwrap();
            comp.remove_layer(surface_id);
            Ok(())
        } else {
            Err(CompositorServiceError::SurfaceNotFound(surface_id))
        }
    }

    /// Check boot finished status.
    pub fn is_boot_finished(&self) -> bool {
        self.boot_finished.load(Ordering::SeqCst)
    }

    /// Set boot finished state.
    pub fn set_boot_finished(&self, finished: bool) {
        self.boot_finished.store(finished, Ordering::SeqCst);
    }

    /// Return display information.
    pub fn get_display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.display_width,
            height: self.display_height,
            fps: 120.0,
            density: 2.0,
            orientation: 0,
            secure: false,
        }
    }

    /// Return buffer producer for a given surface ID.
    pub fn get_surface_producer(
        &self,
        surface_id: u64,
    ) -> Option<Arc<GraphicBufferProducerService>> {
        self.surfaces
            .lock()
            .unwrap()
            .get(&surface_id)
            .map(|s| Arc::clone(&s.producer))
    }

    /// Return active layer count in the compositor.
    pub fn get_layer_count(&self) -> usize {
        self.compositor.lock().unwrap().layers.len()
    }

    /// Return display width and height.
    pub fn display_dimensions(&self) -> (u32, u32) {
        (self.display_width, self.display_height)
    }
}

impl Remotable for SurfaceComposerService {
    fn get_class_descriptor() -> &'static str {
        Self::DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            isurfacecomposer_codes::CREATE_SURFACE => {
                let mut offset = 0;
                let name = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?.unwrap_or_else(|| "Surface".to_string());
                let width = data.read_u32(&mut offset).unwrap_or(64);
                let height = data.read_u32(&mut offset).unwrap_or(64);
                let _format = data.read_i32(&mut offset).unwrap_or(1);
                let flags = data.read_u32(&mut offset).unwrap_or(0);

                let handle = self
                    .create_surface(&name, width, height, flags)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_u64(handle.surface_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                // If handle bridge is present, register producer service handle
                if let Some(bridge) = &self.handle_bridge {
                    let b = bridge.lock().unwrap();
                    let client_id = 1; // Default client id
                    let handle_id = b.register_service(
                        client_id,
                        GraphicBufferProducerService::DESCRIPTOR,
                        Arc::clone(&handle.producer) as Arc<dyn IBinder>,
                    );
                    reply.write_u32(handle_id).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                } else {
                    reply.write_u32(handle.surface_id as u32).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            isurfacecomposer_codes::SET_TRANSACTION_STATE => {
                let mut offset = 0;
                let count = data.read_i32(&mut offset).unwrap_or(0);
                let mut updates = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    let mut state = ComposerState::new(0, LayerState::default());
                    state.read_from_parcel_at(data, &mut offset)?;
                    updates.push(state);
                }
                let flags = data.read_u32(&mut offset).unwrap_or(0);

                self.set_transaction_state(updates, flags)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            isurfacecomposer_codes::BOOT_FINISHED => {
                self.set_boot_finished(true);
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            isurfacecomposer_codes::GET_DISPLAY_INFO => {
                let info = self.get_display_info();
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                info.write_to_parcel(reply)?;
                Ok(())
            }
            isurfacecomposer_codes::DESTROY_SURFACE => {
                let mut offset = 0;
                let surface_id = data.read_u64(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.destroy_surface(surface_id)
                    .map_err(|_| Status::from_status(STATUS_NAME_NOT_FOUND))?;
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for SurfaceComposerService {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => Ok(()),
            INTERFACE_TRANSACTION => {
                reply.write_utf16(Some(Self::DESCRIPTOR)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            DUMP_TRANSACTION => Ok(()),
            _ => self.on_transact(code, data, reply),
        }
    }

    fn is_binder_alive(&self) -> bool {
        true
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(Self::DESCRIPTOR)
    }
}
