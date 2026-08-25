//! Integration test for AudioHostBridge PCM streaming and WebAudio simulation.

use audio_hal_virtual::{
    AudioFormat, AudioModuleService, IModule, IStreamIn, OpenInputStreamArguments,
    OpenOutputStreamArguments,
};
use audio_host_rs::{AudioHostBridge, MicPattern};
use std::sync::Arc;

#[test]
fn test_audio_host_pcm_playback_and_volume_processing() {
    let module = Arc::new(AudioModuleService::new());
    let bridge = AudioHostBridge::new(Arc::clone(&module));

    // 1. Open output stream
    let out_args = OpenOutputStreamArguments {
        port_config_id: 1,
        buffer_size_frames: 240,
        sample_rate: 48000,
        channel_mask: 2,
        format: AudioFormat::Pcm16Bit,
    };
    let out_res = module.open_output_stream(&out_args).unwrap();
    let _stream_out = module.get_output_stream(out_res.stream_id).unwrap();

    // Create 100 16-bit stereo frames (400 bytes) with a non-zero sample (e.g. 1000)
    let sample: i16 = 1000;
    let sample_bytes = sample.to_le_bytes();
    let mut raw_pcm = Vec::with_capacity(400);
    for _ in 0..100 {
        raw_pcm.push(sample_bytes[0]);
        raw_pcm.push(sample_bytes[1]);
        raw_pcm.push(sample_bytes[0]);
        raw_pcm.push(sample_bytes[1]);
    }

    // Set 50% master volume
    module.set_master_volume(0.5).unwrap();
    bridge.process_output_pcm(&raw_pcm);

    let mut read_pcm = vec![0u8; 400];
    bridge.read_playback_pcm(&mut read_pcm);

    let read_sample = i16::from_le_bytes([read_pcm[0], read_pcm[1]]);
    assert_eq!(read_sample, 500, "50% volume scaling of 1000 should be 500");

    // 2. Mute test
    module.set_master_mute(true).unwrap();
    bridge.process_output_pcm(&raw_pcm);

    let mut mute_pcm = vec![0u8; 400];
    bridge.read_playback_pcm(&mut mute_pcm);
    assert!(mute_pcm.iter().all(|&b| b == 0), "Muted audio must produce silence");
}

#[test]
fn test_audio_host_mic_tone_generation() {
    let module = Arc::new(AudioModuleService::new());
    let bridge = AudioHostBridge::new(Arc::clone(&module));

    bridge.mic_source().set_pattern(MicPattern::SineTone(1000.0));

    let in_args = OpenInputStreamArguments {
        port_config_id: 2,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 1,
        format: AudioFormat::Pcm16Bit,
    };
    let in_res = module.open_input_stream(&in_args).unwrap();
    let stream_in = module.get_input_stream(in_res.stream_id).unwrap();

    bridge.pump_mic_input(in_res.stream_id, 480);

    let mut read_buf = vec![0u8; 960];
    let read_frames = stream_in.read(&mut read_buf, 480).unwrap();
    assert_eq!(read_frames, 480);

    // Verify sinusoidal waveform has positive and negative peaks
    let mut has_positive = false;
    let mut has_negative = false;
    for chunk in read_buf.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
        if sample > 1000 {
            has_positive = true;
        }
        if sample < -1000 {
            has_negative = true;
        }
    }
    assert!(has_positive && has_negative, "Mic tone must oscillate around zero");
}
