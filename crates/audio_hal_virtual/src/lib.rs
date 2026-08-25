//! # audio_hal_virtual
//!
//! Virtual Android 13 AIDL Audio HAL (`android.hardware.audio.core.IModule`, `IStreamOut`, `IStreamIn`)
//! for AndroidWebGPU. Provides PCM audio output and input streams, master volume/mute controls,
//! port routing, and handle 0 ServiceManager registration.

pub mod audio_module;
pub mod devices_factory;
pub mod error;
pub mod stream_in;
pub mod stream_out;
pub mod types;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use audio_module::{
    imodule_codes, register_audio_service, AudioModuleProxy, AudioModuleService, IModule,
    IMODULE_DEFAULT_INSTANCE, IMODULE_DESCRIPTOR,
};
pub use devices_factory::{
    idevices_factory_codes, DevicesFactoryService, IDevicesFactory, IDEVICES_FACTORY_DESCRIPTOR,
};
pub use error::AudioHalError;
pub use stream_in::{istream_in_codes, IStreamIn, StreamIn, StreamInProxy, ISTREAM_IN_DESCRIPTOR};
pub use stream_out::{
    istream_out_codes, IStreamOut, StreamOut, StreamOutProxy, ISTREAM_OUT_DESCRIPTOR,
};
pub use types::{
    AudioChannelMask, AudioFormat, AudioPort, AudioRoute, OpenInputStreamArguments,
    OpenInputStreamResult, OpenOutputStreamArguments, OpenOutputStreamResult,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_audio_module_streams_and_controls() {
        let module = Arc::new(AudioModuleService::new());

        // 1. Volume and Mute
        assert_eq!(module.get_master_volume().unwrap(), 1.0);
        assert_eq!(module.get_master_mute().unwrap(), false);

        module.set_master_volume(0.75).unwrap();
        assert!((module.get_master_volume().unwrap() - 0.75).abs() < 1e-4);

        module.set_master_mute(true).unwrap();
        assert_eq!(module.get_master_mute().unwrap(), true);

        // 2. Open Output Stream (48kHz 16-bit Stereo)
        let out_args = OpenOutputStreamArguments {
            port_config_id: 1,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_mask: 2,
            format: AudioFormat::Pcm16Bit,
        };
        let out_res = module.open_output_stream(&out_args).expect("Open output stream failed");
        assert_eq!(out_res.sample_rate, 48000);
        assert_eq!(out_res.channel_count, 2);

        let stream_out = module.get_output_stream(out_res.stream_id).expect("Stream must exist");
        let pcm_data = vec![0x12, 0x34, 0x56, 0x78]; // 1 stereo 16-bit frame (4 bytes)
        let written = stream_out.write(&pcm_data, 1).expect("Write failed");
        assert_eq!(written, 1);
        assert_eq!(stream_out.frames_written(), 1);

        // 3. Open Input Stream (48kHz 16-bit Mono)
        let in_args = OpenInputStreamArguments {
            port_config_id: 2,
            buffer_size_frames: 480,
            sample_rate: 48000,
            channel_mask: 1,
            format: AudioFormat::Pcm16Bit,
        };
        let in_res = module.open_input_stream(&in_args).expect("Open input stream failed");
        let stream_in = module.get_input_stream(in_res.stream_id).expect("Stream must exist");

        // Feed mic data and read
        stream_in.feed_pcm(&[0xaa, 0xbb]); // 1 mono 16-bit frame (2 bytes)
        let mut read_buf = vec![0u8; 2];
        let read_frames = stream_in.read(&mut read_buf, 1).expect("Read failed");
        assert_eq!(read_frames, 1);
        assert_eq!(read_buf, vec![0xaa, 0xbb]);
    }
}
