//! # audio_host_rs
//!
//! Host-Side WebAudio Playback Bridge (`AudioContext`, `AudioWorkletNode` ring buffer) and
//! MediaStream Microphone Capture Bridge for AndroidWebGPU Audio HAL.

pub mod bridge;
pub mod mic_source;
pub mod ring_buffer;

pub use bridge::AudioHostBridge;
pub use mic_source::{MicPattern, MicStreamSource};
pub use ring_buffer::{AudioBufferStats, AudioRingBuffer, DEFAULT_RING_BUFFER_CAPACITY_BYTES};

#[cfg(test)]
mod tests {
    use super::*;
    use audio_hal_virtual::{
        AudioFormat, AudioModuleService, IModule, IStreamIn, OpenInputStreamArguments,
        OpenOutputStreamArguments,
    };
    use std::sync::Arc;

    #[test]
    fn test_audio_host_bridge_playback_and_capture() {
        let module = Arc::new(AudioModuleService::new());
        let bridge = AudioHostBridge::new(Arc::clone(&module));

        // 1. Output Playback Test
        let out_args = OpenOutputStreamArguments {
            port_config_id: 1,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_mask: 2,
            format: AudioFormat::Pcm16Bit,
        };
        let out_res = module.open_output_stream(&out_args).unwrap();
        let _stream_out = module.get_output_stream(out_res.stream_id).unwrap();

        // Feed some PCM data into bridge
        let test_pcm = vec![0x10, 0x20, 0x30, 0x40];
        let processed = bridge.process_output_pcm(&test_pcm);
        assert_eq!(processed, 4);

        let mut read_buf = vec![0u8; 4];
        let read = bridge.read_playback_pcm(&mut read_buf);
        assert_eq!(read, 4);
        assert_eq!(read_buf, test_pcm);

        // 2. Microphone Capture Test
        let in_args = OpenInputStreamArguments {
            port_config_id: 2,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_mask: 1,
            format: AudioFormat::Pcm16Bit,
        };
        let in_res = module.open_input_stream(&in_args).unwrap();
        let stream_in = module.get_input_stream(in_res.stream_id).unwrap();

        // Pump 10 frames from mic source into stream_in
        let pumped = bridge.pump_mic_input(in_res.stream_id, 10);
        assert_eq!(pumped, 10);

        let mut mic_buf = vec![0u8; 20]; // 10 frames * 1 channel * 2 bytes
        let frames_read = stream_in.read(&mut mic_buf, 10).unwrap();
        assert_eq!(frames_read, 10);
        // Non-zero mic samples
        assert!(mic_buf.iter().any(|&b| b != 0));
    }
}
