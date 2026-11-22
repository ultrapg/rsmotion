pub mod asset_cache;
pub mod audio_data;
pub mod decoder;
pub mod error;
pub mod probe;
pub mod silence;
pub mod uri;

pub use asset_cache::{
    cache_assets_dir, cache_inline_bytes, decode_font_to_sfnt, fetch_remote_asset,
    is_remote_http_url, resolve_or_fetch_asset, CHROME_WOFF2_UA,
};
pub use audio_data::{
    decode_audio_bytes, decode_audio_file, wrap_aac_frame_adts, DecodedAudioPcm,
    StreamingAudioDecoder,
};
pub use decoder::{DecodedVideoFrame, VideoDecoderPool};
pub use error::{MediaError, Result};
pub use probe::{probe_media, MediaMetadata};
pub use silence::{get_silent_parts, SilentPart, SilentPartsResult};
pub use uri::{
    format_video_frame_uri, parse_video_frame_uri, percent_decode, resolve_media_path,
    VideoFrameRequest, VIDEO_FRAME_URI_PREFIX,
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fixtures_bundle_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/build")
    }

    #[test]
    fn video_frame_uri_roundtrip() {
        let uri =
            format_video_frame_uri("http://127.0.0.1:3000/public/sample-video.mp4", 1.25, true);
        assert!(uri.starts_with(VIDEO_FRAME_URI_PREFIX));
        let parsed = parse_video_frame_uri(&uri).unwrap();
        assert_eq!(parsed.src, "http://127.0.0.1:3000/public/sample-video.mp4");
        assert!((parsed.time_secs - 1.25).abs() < 1e-5);
        assert!(parsed.transparent);
        assert!(parse_video_frame_uri("http://127.0.0.1/img.png").is_none());
    }

    #[test]
    fn probes_sample_video_and_audio_fixture() {
        let dir = fixtures_bundle_dir();
        let vpath = resolve_media_path(&dir, "/public/sample-video.mp4").unwrap();
        let vmeta = probe_media(&vpath).unwrap();
        assert!(vmeta.has_video);
        assert!(vmeta.has_audio);
        assert_eq!(vmeta.width, 640);
        assert_eq!(vmeta.height, 360);
        assert!((vmeta.fps - 30.0).abs() < 0.01);
        assert!(vmeta.duration_secs >= 2.9);

        let apath = resolve_media_path(&dir, "sample-audio.wav").unwrap();
        let ameta = probe_media(&apath).unwrap();
        assert!(!ameta.has_video);
        assert!(ameta.has_audio);
        assert!(ameta.duration_secs >= 2.9);
    }

    #[test]
    fn decodes_sequential_frames_and_exact_seek_matches() {
        let dir = fixtures_bundle_dir();
        let mut seq_pool = VideoDecoderPool::new(&dir);
        let mut seq_f30 = None;
        for f in 0..=30 {
            let t = f64::from(f) / 30.0;
            let frame = seq_pool
                .decode_frame("/public/sample-video.mp4", t, false)
                .unwrap();
            assert_eq!(frame.width, 640);
            assert_eq!(frame.height, 360);
            assert_eq!(frame.frame_index, i64::from(f));
            assert_eq!(frame.rgba.len(), 640 * 360 * 4);
            assert!(frame.rgba.iter().any(|&b| b != 0 && b != 255));
            if f == 30 {
                seq_f30 = Some(frame);
            }
        }
        let seq_f30 = seq_f30.unwrap();

        let mut seek_pool = VideoDecoderPool::new(&dir);
        let direct_f30 = seek_pool
            .decode_frame("/public/sample-video.mp4", 1.0, false)
            .unwrap();
        assert_eq!(direct_f30.frame_index, 30);
        assert_eq!(direct_f30.rgba.len(), seq_f30.rgba.len());
        let diff_bytes = direct_f30
            .rgba
            .iter()
            .zip(seq_f30.rgba.iter())
            .filter(|(a, b)| a.abs_diff(**b) > 2)
            .count();
        let diff_ratio = (diff_bytes as f64) / (seq_f30.rgba.len() as f64);
        assert!(diff_ratio < 0.005, "seek frame 30 diff_ratio={diff_ratio}");
    }

    #[test]
    fn decodes_woff_passthrough_offline_guard_and_audio_wav() {
        let sfnt = b"\x00\x01\x00\x00testfont";
        assert_eq!(decode_font_to_sfnt(sfnt).as_deref(), Some(sfnt.as_slice()));
        assert!(decode_font_to_sfnt(b"wOF2corrupt").is_none());
        assert!(decode_font_to_sfnt(b"wOFFcorrupt").is_none());

        assert!(is_remote_http_url(
            "https://fonts.gstatic.com/s/roboto/v1/a.woff2"
        ));
        assert!(!is_remote_http_url("http://127.0.0.1:3000/public/a.png"));
        let err = fetch_remote_asset("https://example.invalid/uncached.woff2", true, None)
            .expect_err("offline cache miss should error");
        assert!(err.to_string().contains("offline"), "{err}");

        let wav = resolve_media_path(&fixtures_bundle_dir(), "sample-audio.wav").unwrap();
        let pcm = decode_audio_file(&wav, Some(44100)).unwrap();
        assert_eq!(pcm.sample_rate, 44100);
        assert!(pcm.channels >= 1);
        assert!(pcm.duration_secs >= 2.8);
        assert!(!pcm.interleaved_f32_le.is_empty());

        let adts = wrap_aac_frame_adts(&[0x01, 0x02, 0x03], 48000, 2);
        assert_eq!(adts.len(), 10);
        assert_eq!(adts[0], 0xFF);
        assert_eq!(adts[1], 0xF1);

        assert!(StreamingAudioDecoder::new("mp3", 44100, 2, &[]).is_some());
        assert!(StreamingAudioDecoder::new("opus", 48000, 2, &[]).is_none());
    }

    #[test]
    fn detects_silent_and_audible_parts_in_wav() {
        use std::io::Write;

        let sr = 16000u32;
        let total_secs = 3.5_f64;
        let total_samples = (sr as f64 * total_secs).round() as usize;
        let mut samples = vec![0i16; total_samples];
        for (i, s) in samples.iter_mut().enumerate() {
            let t = i as f64 / f64::from(sr);
            if t < 1.0 || t >= 2.5 {
                let amp = (t * 440.0 * std::f64::consts::TAU).sin() * 16000.0;
                *s = amp.round() as i16;
            }
        }

        let tmp = std::env::temp_dir().join(format!("rsmotion-silence-{}.wav", std::process::id()));
        {
            let mut f = std::fs::File::create(&tmp).unwrap();
            let data_bytes = (samples.len() * 2) as u32;
            let riff_size = 36u32 + data_bytes;
            f.write_all(b"RIFF").unwrap();
            f.write_all(&riff_size.to_le_bytes()).unwrap();
            f.write_all(b"WAVEfmt ").unwrap();
            f.write_all(&16u32.to_le_bytes()).unwrap();
            f.write_all(&1u16.to_le_bytes()).unwrap();
            f.write_all(&1u16.to_le_bytes()).unwrap();
            f.write_all(&sr.to_le_bytes()).unwrap();
            f.write_all(&(sr * 2).to_le_bytes()).unwrap();
            f.write_all(&2u16.to_le_bytes()).unwrap();
            f.write_all(&16u16.to_le_bytes()).unwrap();
            f.write_all(b"data").unwrap();
            f.write_all(&data_bytes.to_le_bytes()).unwrap();
            for s in samples {
                f.write_all(&s.to_le_bytes()).unwrap();
            }
        }

        let res = get_silent_parts(&tmp, -20.0, 0.8).unwrap();
        let _ = std::fs::remove_file(&tmp);

        assert!((res.duration_in_seconds - 3.5).abs() < 0.05);
        assert_eq!(res.silent_parts.len(), 1, "{res:?}");
        assert!(
            (res.silent_parts[0].start_in_seconds - 1.0).abs() < 0.15,
            "{:?}",
            res.silent_parts
        );
        assert!(
            (res.silent_parts[0].end_in_seconds - 2.5).abs() < 0.15,
            "{:?}",
            res.silent_parts
        );
        assert_eq!(res.audible_parts.len(), 2, "{:?}", res.audible_parts);
    }
}
