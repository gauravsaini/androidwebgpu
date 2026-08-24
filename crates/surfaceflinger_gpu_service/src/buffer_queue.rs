//! `IGraphicBufferProducer` service implementation and buffer queue management.

use aidl_compat::{
    DeathRecipient, IBinder, Parcel, Remotable, Result as AidlResult, Status, StatusCode,
    TransactionCode, TransactionFlags, DUMP_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION,
    STATUS_BAD_VALUE, STATUS_INVALID_OPERATION,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Standard AOSP `IGraphicBufferProducer` transaction opcodes.
pub mod igraphicbufferproducer_codes {
    pub const REQUEST_BUFFER: u32 = 1;
    pub const SET_BUFFER_COUNT: u32 = 2;
    pub const DEQUEUE_BUFFER: u32 = 3;
    pub const DETACH_BUFFER: u32 = 4;
    pub const ATTACH_BUFFER: u32 = 5;
    pub const QUEUE_BUFFER: u32 = 6;
    pub const CANCEL_BUFFER: u32 = 7;
    pub const QUERY: u32 = 8;
    pub const CONNECT: u32 = 9;
    pub const DISCONNECT: u32 = 10;
    pub const SET_MAX_DEQUEUED_BUFFER_COUNT: u32 = 11;
    pub const ALLOCATE_BUFFERS: u32 = 12;
}

/// Buffer queue specific error types.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BufferQueueError {
    #[error("Slot index {0} is out of bounds or invalid")]
    InvalidSlot(i32),
    #[error("Buffer slot {0} is currently in use or not dequeued")]
    SlotInUse(i32),
    #[error("No free buffer slots available in queue")]
    NoFreeSlots,
    #[error("Texture dimension or payload mismatch")]
    DimensionMismatch,
    #[error("AIDL error: {0:?}")]
    Aidl(StatusCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    Free,
    Dequeued,
    Queued,
    Acquired,
}

/// Metadata and GPU texture resources associated with a single buffer queue slot.
pub struct GraphicBufferSlot {
    pub slot_id: i32,
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub state: SlotState,
    pub texture: Option<wgpu::Texture>,
    pub texture_view: Option<wgpu::TextureView>,
    pub generation: u32,
}

/// Host-side implementation of `android.gui.IGraphicBufferProducer`.
pub struct GraphicBufferProducerService {
    surface_id: u64,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    slots: Mutex<HashMap<i32, GraphicBufferSlot>>,
    max_slots: i32,
    connected: AtomicBool,
    last_queued_slot: Mutex<Option<i32>>,
    generation_counter: AtomicU32,
}

unsafe impl Send for GraphicBufferProducerService {}
unsafe impl Sync for GraphicBufferProducerService {}

impl GraphicBufferProducerService {
    /// Canonical AIDL interface descriptor.
    pub const DESCRIPTOR: &'static str = "android.gui.IGraphicBufferProducer";

    /// Construct a new graphic buffer producer service attached to a surface.
    pub fn new(surface_id: u64, device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
        let mut initial_slots = HashMap::new();
        let max_slots = 16;
        for slot in 0..max_slots {
            initial_slots.insert(
                slot,
                GraphicBufferSlot {
                    slot_id: slot,
                    width: 0,
                    height: 0,
                    format: 1, // HAL_PIXEL_FORMAT_RGBA_8888
                    state: SlotState::Free,
                    texture: None,
                    texture_view: None,
                    generation: 0,
                },
            );
        }

        Self {
            surface_id,
            device,
            queue,
            slots: Mutex::new(initial_slots),
            max_slots,
            connected: AtomicBool::new(false),
            last_queued_slot: Mutex::new(None),
            generation_counter: AtomicU32::new(1),
        }
    }

    /// Return maximum slot capacity of this buffer queue.
    pub fn max_slots(&self) -> i32 {
        self.max_slots
    }

    /// Return the surface ID owning this buffer queue.
    pub fn surface_id(&self) -> u64 {
        self.surface_id
    }

    /// Connect client to this buffer producer.
    pub fn connect(&self) -> Result<(), BufferQueueError> {
        self.connected.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Disconnect client.
    pub fn disconnect(&self) -> Result<(), BufferQueueError> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Dequeue an available buffer slot, reallocating texture if dimensions changed.
    pub fn dequeue_buffer(
        &self,
        width: u32,
        height: u32,
        format: u32,
    ) -> Result<i32, BufferQueueError> {
        let mut slots = self.slots.lock().unwrap();

        // Find first free slot or first non-acquired slot
        let target_slot = slots
            .values()
            .find(|s| s.state == SlotState::Free)
            .map(|s| s.slot_id)
            .or_else(|| {
                slots
                    .values()
                    .find(|s| s.state != SlotState::Dequeued && s.state != SlotState::Acquired)
                    .map(|s| s.slot_id)
            })
            .ok_or(BufferQueueError::NoFreeSlots)?;

        let slot_ref = slots.get_mut(&target_slot).unwrap();
        slot_ref.state = SlotState::Dequeued;

        if slot_ref.width != width
            || slot_ref.height != height
            || slot_ref.format != format
            || slot_ref.texture.is_none()
        {
            let w = width.max(1);
            let h = height.max(1);
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Surface_{}_BufferSlot_{}", self.surface_id, target_slot)),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            slot_ref.width = width;
            slot_ref.height = height;
            slot_ref.format = format;
            slot_ref.texture = Some(texture);
            slot_ref.texture_view = Some(texture_view);
        }

        slot_ref.generation = self.generation_counter.fetch_add(1, Ordering::SeqCst);
        Ok(target_slot)
    }

    /// Upload raw RGBA pixel data to a dequeued slot and mark it queued.
    pub fn queue_buffer_data(
        &self,
        slot: i32,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), BufferQueueError> {
        let mut slots = self.slots.lock().unwrap();
        let slot_ref = slots.get_mut(&slot).ok_or(BufferQueueError::InvalidSlot(slot))?;

        if slot_ref.state != SlotState::Dequeued {
            return Err(BufferQueueError::SlotInUse(slot));
        }

        let w = width.max(1);
        let h = height.max(1);
        let expected_len = (w * h * 4) as usize;
        if data.len() < expected_len {
            return Err(BufferQueueError::DimensionMismatch);
        }

        if slot_ref.width != width || slot_ref.height != height || slot_ref.texture.is_none() {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("Surface_{}_BufferSlot_{}", self.surface_id, slot)),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            slot_ref.width = width;
            slot_ref.height = height;
            slot_ref.texture = Some(texture);
            slot_ref.texture_view = Some(texture_view);
        }

        if let Some(tex) = &slot_ref.texture {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &data[0..expected_len],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }

        slot_ref.state = SlotState::Queued;
        *self.last_queued_slot.lock().unwrap() = Some(slot);
        Ok(())
    }

    /// Fill a slot with a solid RGBA color and mark queued.
    pub fn queue_buffer_color(
        &self,
        slot: i32,
        color_rgba: [u8; 4],
        width: u32,
        height: u32,
    ) -> Result<(), BufferQueueError> {
        let w = width.max(1);
        let h = height.max(1);
        let count = (w * h) as usize;
        let mut pixels = Vec::with_capacity(count * 4);
        for _ in 0..count {
            pixels.extend_from_slice(&color_rgba);
        }
        self.queue_buffer_data(slot, &pixels, w, h)
    }

    /// Cancel a previously dequeued buffer.
    pub fn cancel_buffer(&self, slot: i32) {
        let mut slots = self.slots.lock().unwrap();
        if let Some(slot_ref) = slots.get_mut(&slot) {
            if slot_ref.state == SlotState::Dequeued {
                slot_ref.state = SlotState::Free;
            }
        }
    }

    /// Acquire the most recently queued `wgpu::TextureView` for compositor presentation.
    pub fn acquire_latest_texture_view(&self) -> Option<wgpu::TextureView> {
        let last_slot = *self.last_queued_slot.lock().unwrap();
        if let Some(slot) = last_slot {
            let mut slots = self.slots.lock().unwrap();
            if let Some(slot_ref) = slots.get_mut(&slot) {
                if let Some(tex) = &slot_ref.texture {
                    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                    slot_ref.state = SlotState::Acquired;
                    return Some(view);
                }
            }
        }
        None
    }
}

impl Remotable for GraphicBufferProducerService {
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
            igraphicbufferproducer_codes::CONNECT => {
                self.connect().map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_i32(0).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?; // QueueBufferOutput status
                Ok(())
            }
            igraphicbufferproducer_codes::DISCONNECT => {
                self.disconnect().map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            igraphicbufferproducer_codes::DEQUEUE_BUFFER => {
                let mut offset = 0;
                let w = data.read_u32(&mut offset).unwrap_or(64);
                let h = data.read_u32(&mut offset).unwrap_or(64);
                let format = data.read_u32(&mut offset).unwrap_or(1);
                match self.dequeue_buffer(w, h, format) {
                    Ok(slot) => {
                        reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply.write_i32(slot).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply.write_i32(0).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?; // Fence fd (-1 / None)
                    }
                    Err(_) => {
                        reply.write_status(&Status::from_status(STATUS_INVALID_OPERATION)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            igraphicbufferproducer_codes::QUEUE_BUFFER => {
                let mut offset = 0;
                let slot = data.read_i32(&mut offset).unwrap_or(0);
                let width = data.read_u32(&mut offset).unwrap_or(64);
                let height = data.read_u32(&mut offset).unwrap_or(64);
                let bytes = data.read_byte_vec(&mut offset).unwrap_or(None);

                let res = if let Some(buf) = bytes {
                    self.queue_buffer_data(slot, &buf, width, height)
                } else {
                    // Fallback to white buffer
                    self.queue_buffer_color(slot, [255, 255, 255, 255], width, height)
                };

                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                        reply.write_i32(0).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?; // Output status
                    }
                    Err(_) => {
                        reply.write_status(&Status::from_status(STATUS_BAD_VALUE)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            igraphicbufferproducer_codes::CANCEL_BUFFER => {
                let mut offset = 0;
                let slot = data.read_i32(&mut offset).unwrap_or(0);
                self.cancel_buffer(slot);
                reply.write_status(&Status::ok()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for GraphicBufferProducerService {
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
