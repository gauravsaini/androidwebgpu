//! Host-Side Camera Stream Bridge connecting WebRTC/Webcam Feeds to Virtual Camera HAL.

use crate::buffer_pool::CameraBufferPool;
use crate::frame_generator::{CameraFrameGenerator, FramePattern};
use camera_hal_virtual::{CameraDeviceSessionService, PixelFormat};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraHostMode {
    SyntheticGenerator,
    WebRtcStream,
    HostWebcam,
}

pub struct CameraHostBridge {
    session: Arc<CameraDeviceSessionService>,
    buffer_pool: Arc<CameraBufferPool>,
    generator: Mutex<CameraFrameGenerator>,
    mode: Mutex<CameraHostMode>,
    frame_sequence: AtomicU32,
}

impl CameraHostBridge {
    pub fn new(session: Arc<CameraDeviceSessionService>) -> Self {
        Self {
            session,
            buffer_pool: Arc::new(CameraBufferPool::new(CameraBufferPool::DEFAULT_POOL_CAPACITY)),
            generator: Mutex::new(CameraFrameGenerator::new(FramePattern::ColorBars)),
            mode: Mutex::new(CameraHostMode::SyntheticGenerator),
            frame_sequence: AtomicU32::new(1),
        }
    }

    pub fn set_mode(&self, mode: CameraHostMode) {
        *self.mode.lock().unwrap() = mode;
    }

    pub fn set_pattern(&self, pattern: FramePattern) {
        self.generator.lock().unwrap().set_pattern(pattern);
    }

    pub fn buffer_pool(&self) -> &Arc<CameraBufferPool> {
        &self.buffer_pool
    }

    /// Inject an incoming frame from browser `getUserMedia` / WebRTC video track.
    pub fn inject_frame(
        &self,
        stream_id: i32,
        width: u32,
        height: u32,
        format: PixelFormat,
        data: &[u8],
    ) -> bool {
        let mut handle = self.buffer_pool.acquire(width, height, format);
        if handle.data.len() == data.len() {
            handle.data.copy_from_slice(data);
            self.session.set_stream_frame_data(stream_id, &handle.data);
            self.buffer_pool.release(handle);
            true
        } else {
            self.buffer_pool.release(handle);
            false
        }
    }

    /// Advance synthetic frame generator and update stream frame cache in session.
    pub fn tick_frame(&self, stream_id: i32, width: u32, height: u32, format: PixelFormat) -> u32 {
        let frame_num = self.frame_sequence.fetch_add(1, Ordering::Relaxed);
        let mut handle = self.buffer_pool.acquire(width, height, format);

        {
            let mut gen = self.generator.lock().unwrap();
            gen.generate_frame(width, height, format, &mut handle.data);
        }

        self.session.set_stream_frame_data(stream_id, &handle.data);
        self.buffer_pool.release(handle);
        frame_num
    }
}
