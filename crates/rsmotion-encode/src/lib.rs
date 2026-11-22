pub mod audio;
pub mod concat;
pub mod error;
pub mod pipe;
pub mod probe;
pub mod still;
pub mod yuv;

pub use audio::{
    extract_audio_track, mix_and_mux_audio, mix_and_mux_audio_with_options, AudioMuxOptions,
    AudioTrackFrame,
};
pub use concat::concat_chunks;
pub use error::{EncodeError, Result};
pub use pipe::{EncodeOptions, EncodeStats, FfmpegPipeEncoder, InputPixFmt, RawFrame};
pub use probe::{probe_best_h264_encoder, ResolvedEncoder};
pub use still::{bgra_to_rgba, save_rgba_png, save_still_image};
pub use yuv::convert_rgba_or_bgra_to_nv12;

/// Generates a deterministic animated RGBA gradient buffer for encode benchmarks/tests.
pub fn generate_synthetic_gradient_frame(width: u32, height: u32, frame_idx: u32) -> RawFrame {
    let len = (width as usize) * (height as usize) * 4;
    let mut data = vec![0u8; len];
    let shift = (frame_idx * 7) as u8;
    for y in 0..height {
        let g = ((y * 255) / height.max(1)) as u8;
        let row_start = (y as usize) * (width as usize) * 4;
        for x in 0..width {
            let i = row_start + (x as usize) * 4;
            data[i] = (((x * 255) / width.max(1)) as u8).wrapping_add(shift);
            data[i + 1] = g;
            data[i + 2] = shift.wrapping_mul(3);
            data[i + 3] = 255;
        }
    }
    RawFrame {
        index: frame_idx,
        width,
        height,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsmotion_core::{Codec, HardwareAccel, PixelFormat};
    use std::process::Command;

    fn ffprobe_stream(path: &std::path::Path) -> (u32, u32, u32) {
        let out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-count_frames",
                "-show_entries",
                "stream=width,height,nb_read_frames",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .output()
            .expect("ffprobe must run");
        assert!(
            out.status.success(),
            "ffprobe failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8_lossy(&out.stdout);
        let parts: Vec<&str> = text.trim().split(',').collect();
        assert_eq!(parts.len(), 3, "unexpected ffprobe output: {text}");
        (
            parts[0].parse().unwrap(),
            parts[1].parse().unwrap(),
            parts[2].parse().unwrap(),
        )
    }

    #[test]
    fn probe_auto_h264_encoder_succeeds() {
        let enc = probe_best_h264_encoder(HardwareAccel::Auto).unwrap();
        assert!(!enc.ffmpeg_codec.is_empty());
    }

    #[test]
    fn encode_30_frames_auto_and_software() {
        let tmp = std::env::temp_dir().join(format!("rsmotion-enc-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        for (label, accel) in [
            ("auto", HardwareAccel::Auto),
            ("sw", HardwareAccel::Software),
        ] {
            let out_mp4 = tmp.join(format!("{label}.mp4"));
            let encoder = FfmpegPipeEncoder::spawn(EncodeOptions {
                width: 640,
                height: 360,
                fps: 30.0,
                output_path: out_mp4.clone(),
                codec: Codec::H264,
                crf: Some(23),
                pixel_format: PixelFormat::Yuv420p,
                input_pix_fmt: InputPixFmt::Rgba,
                hw_accel: accel,
                audio_input: None,
                muted: true,
                x264_preset: Some("ultrafast".to_string()),
                ring_capacity: 8,
                ..Default::default()
            })
            .unwrap();

            for i in 0..30 {
                encoder
                    .push_frame(generate_synthetic_gradient_frame(640, 360, i))
                    .unwrap();
            }
            let stats = encoder.finish().unwrap();
            assert_eq!(stats.frames_encoded, 30);

            let (w, h, nb_frames) = ffprobe_stream(&out_mp4);
            assert_eq!((w, h, nb_frames), (640, 360, 30));
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn concat_two_15_frame_chunks() {
        let tmp = std::env::temp_dir().join(format!("rsmotion-concat-test-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let c1 = tmp.join("c1.mp4");
        let c2 = tmp.join("c2.mp4");
        let merged = tmp.join("merged.mp4");

        for (chunk_idx, path) in [(0u32, &c1), (1u32, &c2)] {
            let encoder = FfmpegPipeEncoder::spawn(EncodeOptions {
                width: 320,
                height: 240,
                fps: 30.0,
                output_path: path.clone(),
                codec: Codec::H264,
                crf: Some(23),
                pixel_format: PixelFormat::Yuv420p,
                input_pix_fmt: InputPixFmt::Rgba,
                hw_accel: HardwareAccel::Software,
                audio_input: None,
                muted: true,
                x264_preset: Some("ultrafast".to_string()),
                ring_capacity: 4,
                ..Default::default()
            })
            .unwrap();
            for f in 0..15 {
                encoder
                    .push_frame(generate_synthetic_gradient_frame(
                        320,
                        240,
                        chunk_idx * 15 + f,
                    ))
                    .unwrap();
            }
            encoder.finish().unwrap();
        }

        concat_chunks(&[c1, c2], &merged).unwrap();
        let (w, h, nb_frames) = ffprobe_stream(&merged);
        assert_eq!((w, h, nb_frames), (320, 240, 30));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn encode_bgra_frames_and_mix_audio() {
        let tmp = std::env::temp_dir().join(format!("rsmotion-bgra-audio-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let wav_path = tmp.join("tone.wav");
        let wav_out = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=1",
                "-ac",
                "2",
            ])
            .arg(&wav_path)
            .output()
            .expect("ffmpeg must generate wav");
        assert!(wav_out.status.success());

        let video_only = tmp.join("video-only.mp4");
        let encoder = FfmpegPipeEncoder::spawn(EncodeOptions {
            width: 320,
            height: 240,
            fps: 30.0,
            output_path: video_only.clone(),
            codec: Codec::H264,
            crf: Some(23),
            pixel_format: PixelFormat::Yuv420p,
            input_pix_fmt: InputPixFmt::Bgra,
            hw_accel: HardwareAccel::Auto,
            audio_input: None,
            muted: true,
            x264_preset: Some("ultrafast".to_string()),
            ring_capacity: 4,
            ..Default::default()
        })
        .unwrap();

        for i in 0..30u32 {
            encoder
                .push_frame(generate_synthetic_gradient_frame(320, 240, i))
                .unwrap();
        }
        encoder.finish().unwrap();

        let tracks: Vec<AudioTrackFrame> = (0..30u32)
            .map(|f| AudioTrackFrame {
                id: "tone-1".to_string(),
                src: "tone.wav".to_string(),
                frame: f,
                volume: 0.2 + 0.8 * (f as f64 / 29.0),
                media_frame: f as f64,
                playback_rate: 1.0,
                audio_start_frame: 0.0,
            })
            .collect();

        let final_mp4 = tmp.join("final.mp4");
        let muxed = mix_and_mux_audio(&video_only, &final_mp4, &tracks, 30.0, 30, 48000, |_| {
            Some(wav_path.clone())
        })
        .unwrap();
        assert!(muxed);

        let (w, h, nb_frames) = ffprobe_stream(&final_mp4);
        assert_eq!((w, h, nb_frames), (320, 240, 30));

        let a_out = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(&final_mp4)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&a_out.stdout).trim(), "aac");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn encode_with_bitrate_colorspace_and_extract_audio_track() {
        use rsmotion_core::{AudioCodec, ColorSpace};

        let tmp = std::env::temp_dir().join(format!("rsmotion-enc-p10-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();

        let br_mp4 = tmp.join("bitrate-bt709.mp4");
        let encoder = FfmpegPipeEncoder::spawn(EncodeOptions {
            width: 320,
            height: 240,
            fps: 30.0,
            output_path: br_mp4.clone(),
            codec: Codec::H264,
            crf: None,
            pixel_format: PixelFormat::Yuv420p,
            input_pix_fmt: InputPixFmt::Rgba,
            hw_accel: HardwareAccel::Software,
            audio_input: None,
            muted: true,
            x264_preset: Some("ultrafast".to_string()),
            ring_capacity: 4,
            video_bitrate: Some("800k".to_string()),
            audio_bitrate: Some("128k".to_string()),
            audio_codec: Some(AudioCodec::Aac),
            color_space: ColorSpace::Bt709,
            prores_profile: None,
            number_of_gif_loops: None,
        })
        .unwrap();

        for i in 0..10 {
            encoder
                .push_frame(generate_synthetic_gradient_frame(320, 240, i))
                .unwrap();
        }
        encoder.finish().unwrap();
        assert_eq!(ffprobe_stream(&br_mp4), (320, 240, 10));

        let sample_video = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/public/sample-video.mp4");
        let extracted = tmp.join("extracted.m4a");
        extract_audio_track(&sample_video, &extracted).unwrap();
        let meta = std::fs::metadata(&extracted).unwrap();
        assert!(
            meta.len() > 1024,
            "extracted audio too small: {}",
            meta.len()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
