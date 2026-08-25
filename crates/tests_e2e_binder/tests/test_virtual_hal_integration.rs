//! End-to-end integration test verifying Virtual AIDL Sensors HAL and Virtual AIDL Audio HAL
//! interaction with host bridges, AIDL Proxies, and mock ServiceManager.

use aidl_compat::traits::Interface;
use audio_hal_virtual::{
    AudioFormat, AudioModuleProxy, AudioModuleService, IModule, IStreamIn, IStreamOut,
    OpenInputStreamArguments, OpenOutputStreamArguments, StreamInProxy, StreamOutProxy,
};
use audio_host_rs::{AudioHostBridge, MicPattern};
use sensor_host_rs::SensorHostBridge;
use sensors_hal_virtual::{
    Event, ISensors, OperationMode, SensorType, SensorsHalService, SensorsProxy,
};
use std::sync::Arc;

#[test]
fn test_sensors_and_audio_hal_system_services_e2e() {
    // 1. Initialize Virtual Sensors HAL and Host Bridge
    let sensors_service = Arc::new(SensorsHalService::new());
    let sensors_proxy = SensorsProxy::new(sensors_service.as_binder());
    let sensor_bridge = SensorHostBridge::new(Arc::clone(&sensors_service));

    // Enumerate sensors via AIDL proxy
    let sensor_list = sensors_proxy.get_sensors_list().unwrap();
    assert_eq!(sensor_list.len(), 2);
    assert_eq!(sensor_list[0].sensor_handle, 1);
    assert_eq!(sensor_list[0].sensor_type, SensorType::Accelerometer);
    assert_eq!(sensor_list[1].sensor_handle, 2);
    assert_eq!(sensor_list[1].sensor_type, SensorType::Gyroscope);

    // Activate & batch sensors
    sensors_proxy.batch(1, 10_000_000, 0).unwrap(); // 100 Hz
    sensors_proxy.activate(1, true).unwrap();
    sensors_proxy.batch(2, 20_000_000, 0).unwrap(); // 50 Hz
    sensors_proxy.activate(2, true).unwrap();

    // Stream host motion ticks
    for i in 1..=5 {
        sensor_bridge.tick(i * 10_000_000);
    }
    let events = sensors_service.poll_events(10);
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| e.sensor_handle == 1));

    // Data injection mode
    sensors_proxy.set_operation_mode(OperationMode::DataInjection).unwrap();
    let injected = Event::new_accelerometer(1, 999_999, 1.23, 9.81, -0.45);
    sensors_proxy.inject_sensor_data(&injected).unwrap();
    let polled = sensors_service.poll_events(5);
    assert_eq!(polled.len(), 1);
    assert_eq!(polled[0].timestamp, 999_999);
    assert!((polled[0].x() - 1.23).abs() < 1e-4);

    // 2. Initialize Virtual Audio HAL and WebAudio Bridge
    let audio_service = Arc::new(AudioModuleService::new());
    let audio_proxy = AudioModuleProxy::new(audio_service.as_binder());
    let audio_bridge = AudioHostBridge::new(Arc::clone(&audio_service));

    // Set volume and verify
    audio_proxy.set_master_volume(0.65).unwrap();
    assert!((audio_proxy.get_master_volume().unwrap() - 0.65).abs() < 1e-4);

    // Open Output Stream (Playback)
    let out_args = OpenOutputStreamArguments {
        port_config_id: 1,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 2,
        format: AudioFormat::Pcm16Bit,
    };
    let out_res = audio_proxy.open_output_stream(&out_args).unwrap();
    let stream_out_binder = out_res.stream.unwrap();
    let stream_out_proxy = StreamOutProxy::new(stream_out_binder);

    // Write PCM buffer
    let pcm_sample: i16 = 2000;
    let mut out_buffer = Vec::with_capacity(1920);
    for _ in 0..480 {
        out_buffer.extend(&pcm_sample.to_le_bytes()); // Left
        out_buffer.extend(&pcm_sample.to_le_bytes()); // Right
    }
    let written = stream_out_proxy.write(&out_buffer, 480).unwrap();
    assert_eq!(written, 480);

    // Process through host bridge and read back
    audio_bridge.process_output_pcm(&out_buffer);
    let mut host_read_buf = vec![0u8; 1920];
    let host_read = audio_bridge.read_playback_pcm(&mut host_read_buf);
    assert_eq!(host_read, 1920);
    let scaled_sample = i16::from_le_bytes([host_read_buf[0], host_read_buf[1]]);
    assert_eq!(scaled_sample, (2000.0 * 0.65) as i16);

    // Open Input Stream (Microphone Capture)
    let in_args = OpenInputStreamArguments {
        port_config_id: 2,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 1,
        format: AudioFormat::Pcm16Bit,
    };
    let in_res = audio_proxy.open_input_stream(&in_args).unwrap();
    let stream_in_binder = in_res.stream.unwrap();
    let stream_in_proxy = StreamInProxy::new(stream_in_binder);

    audio_bridge.mic_source().set_pattern(MicPattern::SineTone(440.0));
    audio_bridge.pump_mic_input(in_res.stream_id, 480);

    let mut in_buffer = vec![0u8; 960];
    let read_mic_frames = stream_in_proxy.read(&mut in_buffer, 480).unwrap();
    assert_eq!(read_mic_frames, 480);
    assert!(in_buffer.iter().any(|&b| b != 0));
}
