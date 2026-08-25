//! Audio Host Bridge coordinating WebAudio playback graph and microphone capture source.

use crate::mic_source::MicStreamSource;
use crate::ring_buffer::AudioRingBuffer;
use audio_hal_virtual::{AudioModuleService, IModule, IStreamIn};
use std::sync::Arc;

pub struct AudioHostBridge {
    module: Arc<AudioModuleService>,
    playback_ring_buffer: Arc<AudioRingBuffer>,
    mic_source: Arc<MicStreamSource>,
}

impl AudioHostBridge {
    pub fn new(module: Arc<AudioModuleService>) -> Self {
        Self {
            module,
            playback_ring_buffer: Arc::new(AudioRingBuffer::new()),
            mic_source: Arc::new(MicStreamSource::new()),
        }
    }

    pub fn with_custom_components(
        module: Arc<AudioModuleService>,
        playback_ring_buffer: Arc<AudioRingBuffer>,
        mic_source: Arc<MicStreamSource>,
    ) -> Self {
        Self {
            module,
            playback_ring_buffer,
            mic_source,
        }
    }

    pub fn module(&self) -> &Arc<AudioModuleService> {
        &self.module
    }

    pub fn playback_ring_buffer(&self) -> &Arc<AudioRingBuffer> {
        &self.playback_ring_buffer
    }

    pub fn mic_source(&self) -> &Arc<MicStreamSource> {
        &self.mic_source
    }

    /// Process output stream buffer and route through master volume & mute into playback ring buffer.
    pub fn process_output_pcm(&self, raw_pcm: &[u8]) -> usize {
        let is_muted = self.module.get_master_mute().unwrap_or(false);
        let volume = self.module.get_master_volume().unwrap_or(1.0);

        if is_muted || volume <= 0.0 {
            // Write silence
            let silence = vec![0u8; raw_pcm.len()];
            self.playback_ring_buffer.write(&silence)
        } else if (volume - 1.0).abs() < 1e-4 {
            // Direct write
            self.playback_ring_buffer.write(raw_pcm)
        } else {
            // Apply volume scaling for 16-bit PCM
            let mut scaled = Vec::with_capacity(raw_pcm.len());
            for chunk in raw_pcm.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                let scaled_sample = ((sample as f32) * volume).clamp(-32768.0, 32767.0) as i16;
                let bytes = scaled_sample.to_le_bytes();
                scaled.push(bytes[0]);
                scaled.push(bytes[1]);
            }
            self.playback_ring_buffer.write(&scaled)
        }
    }

    /// Pump microphone capture samples into the specified input stream.
    pub fn pump_mic_input(&self, stream_id: i32, frame_count: u32) -> usize {
        if let Some(stream_in) = self.module.get_input_stream(stream_id) {
            let sample_rate = stream_in.get_sample_rate().unwrap_or(48000);
            let channel_count = stream_in.get_channel_count().unwrap_or(1);
            let samples = self.mic_source.generate_pcm_16bit(
                frame_count as usize,
                sample_rate,
                channel_count,
            );
            stream_in.feed_pcm(&samples);
            frame_count as usize
        } else {
            0
        }
    }

    /// Drain PCM bytes from playback ring buffer for host WebAudio playback (e.g. AudioWorklet).
    pub fn read_playback_pcm(&self, out: &mut [u8]) -> usize {
        self.playback_ring_buffer.read(out)
    }
}
