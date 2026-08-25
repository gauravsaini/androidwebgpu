//! # media_host_rs
//!
//! Framework-level `IMediaCodecService` (`"android.media.IMediaCodecService"` / `"media.codec"`)
//! and WebCodecs `VideoDecoder` Bridge for AndroidWebGPU.
//!
//! Provides H.264 / H.265 bitstream parsing, input/output buffer queuing, decoded frame processing,
//! WebGPU surface rendering bridge, and ServiceManager registration.

pub mod codec_instance;
pub mod codec_service;
pub mod error;
pub mod types;
pub mod webcodecs_bridge;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use codec_instance::{
    imedia_codec_codes, CodecState, IMediaCodec, MediaCodecBinder, MediaCodecProxy,
    MediaCodecServiceInstance, IMEDIA_CODEC_DESCRIPTOR,
};
pub use codec_service::{
    imedia_codec_service_codes, register_media_codec_service, IMediaCodecService,
    MediaCodecService, MediaCodecServiceBinder, MediaCodecServiceProxy,
    IMEDIA_CODEC_SERVICE_DESCRIPTOR, MEDIA_CODEC_SERVICE_NAME,
};
pub use error::MediaCodecError;
pub use types::{
    BufferInfo, MediaCodecInfo, MediaFormat, BUFFER_FLAG_CODEC_CONFIG, BUFFER_FLAG_END_OF_STREAM,
    BUFFER_FLAG_KEY_FRAME, BUFFER_FLAG_PARTIAL_FRAME, CONFIGURE_FLAG_ENCODE,
    INFO_OUTPUT_BUFFERS_CHANGED, INFO_OUTPUT_FORMAT_CHANGED, INFO_TRY_AGAIN_LATER,
};
pub use webcodecs_bridge::{
    BitstreamParser, DecodedVideoFrame, H264NaluType, H265NaluType, WebCodecsVideoDecoderBridge,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::Interface;
    use std::sync::Arc;

    #[test]
    fn test_media_codec_service_creation_and_codec_list() {
        let service = Arc::new(MediaCodecService::new());

        // 1. Get Codec List
        let list = service.get_codec_list().expect("Get codec list failed");
        assert!(list.len() >= 4);
        assert!(list.iter().any(|c| c.name == "c2.webcodecs.avc.decoder" && !c.is_encoder));
        assert!(list.iter().any(|c| c.name == "c2.webcodecs.hevc.decoder" && !c.is_encoder));

        // 2. Create Codec by Type
        let avc_decoder = service
            .create_codec_by_type("video/avc", false)
            .expect("Create AVC decoder failed");

        // 3. Configure and start
        let mut fmt = MediaFormat::new_video_format("video/avc", 1280, 720);
        fmt.set_string("color-format", "yuv420p");
        avc_decoder.configure(&fmt, None, 0).expect("Configure failed");
        avc_decoder.start().expect("Start failed");

        // 4. Verify output format
        let out_fmt = avc_decoder.get_output_format().expect("Get output format failed");
        assert_eq!(out_fmt.width, 1280);
        assert_eq!(out_fmt.height, 720);
        assert_eq!(out_fmt.mime, "video/avc");
    }

    #[test]
    fn test_media_codec_input_output_queue_and_h264_bitstream() {
        let service = Arc::new(MediaCodecService::new());
        let decoder = service
            .create_codec_by_name("c2.webcodecs.avc.decoder")
            .expect("Create codec failed");

        let fmt = MediaFormat::new_video_format("video/avc", 640, 480);
        decoder.configure(&fmt, None, 0).unwrap();
        decoder.start().unwrap();

        // 1. Dequeue input buffer
        let in_index = decoder.dequeue_input_buffer(0).expect("Dequeue input buffer failed");
        assert!(in_index >= 0);
        let in_idx = in_index as u32;

        // 2. Construct mock H.264 IDR NAL unit (Annex B start code 0x00000001 + 0x65 IDR slice header)
        let mut h264_frame = vec![0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00];
        h264_frame.resize(1024, 0xaa);

        decoder.set_input_buffer(in_idx, &h264_frame).unwrap();

        // 3. Queue input buffer with PTS = 33333 us (30fps frame 1)
        decoder
            .queue_input_buffer(in_idx, 0, h264_frame.len() as u32, 33_333, BUFFER_FLAG_KEY_FRAME)
            .expect("Queue input failed");

        // 4. Dequeue decoded output buffer
        let mut out_info = BufferInfo::default();
        let out_index = decoder
            .dequeue_output_buffer(&mut out_info, 0)
            .expect("Dequeue output buffer failed");
        assert!(out_index >= 0);
        let out_idx = out_index as u32;

        assert_eq!(out_info.presentation_time_us, 33_333);
        assert_eq!(out_info.size, 640 * 480 * 3 / 2);
        assert!(out_info.is_key_frame());

        // 5. Read output buffer
        let out_data = decoder.get_output_buffer(out_idx).expect("Get output buffer failed");
        assert_eq!(out_data.len(), 640 * 480 * 3 / 2);

        // 6. Release output buffer with render = true
        decoder.release_output_buffer(out_idx, true, 33_333_000).expect("Release output failed");
    }

    #[test]
    fn test_media_codec_service_registration() {
        let service = Arc::new(MediaCodecService::new());
        assert_eq!(MEDIA_CODEC_SERVICE_NAME, "media.codec");
        assert_eq!(IMEDIA_CODEC_SERVICE_DESCRIPTOR, "android.media.IMediaCodecService");
        let proxy = MediaCodecServiceProxy::new(service.as_binder());
        let list = proxy.get_codec_list().expect("Proxy get_codec_list failed");
        assert!(list.len() >= 4);
    }

    #[test]
    fn test_annex_b_nalu_parser() {
        let bitstream = vec![
            0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x1f, // SPS
            0x00, 0x00, 0x00, 0x01, 0x68, 0xce, 0x3c, 0x80, // PPS
            0x00, 0x00, 0x01, 0x65, 0x88, 0x84,             // IDR
        ];

        let nalus = BitstreamParser::find_annex_b_nalus(&bitstream);
        assert_eq!(nalus.len(), 3);
        assert!(BitstreamParser::is_h264_keyframe(&bitstream));
    }
}
