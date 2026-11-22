pub mod codec;
pub mod config;
pub mod error;
pub mod frame_range;

pub use codec::{
    validate_even_dimensions, AudioCodec, Codec, ColorSpace, HardwareAccel, ImageFormat,
    PixelFormat, ProResProfile, RasterBackend, Tier,
};
pub use config::VideoConfig;
pub use error::{Result, RsmotionError};
pub use frame_range::FrameRange;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_range_parses_single_and_ranges() {
        let r1: FrameRange = "0-59".parse().unwrap();
        assert_eq!(r1, FrameRange { start: 0, end: 59 });
        assert_eq!(r1.len(), 60);

        let r2: FrameRange = "15".parse().unwrap();
        assert_eq!(r2, FrameRange { start: 15, end: 15 });
        assert_eq!(r2.len(), 1);

        let r3: FrameRange = " 10 - 20 ".parse().unwrap();
        assert_eq!(r3, FrameRange { start: 10, end: 20 });

        assert!("20-10".parse::<FrameRange>().is_err());
        assert!("abc".parse::<FrameRange>().is_err());
        assert!("-5".parse::<FrameRange>().is_err());
        assert!("".parse::<FrameRange>().is_err());
    }

    #[test]
    fn frame_range_validates_duration_and_stride() {
        let r: FrameRange = "0-10".parse().unwrap();
        assert_eq!(r.resolve_frames(11, 1).unwrap().len(), 11);
        assert_eq!(r.resolve_frames(11, 3).unwrap(), vec![0, 3, 6, 9]);
        assert!(r.resolve_frames(10, 1).is_err());
        assert!(r.resolve_frames(11, 0).is_err());
    }

    #[test]
    fn codec_defaults_and_crf_validation() {
        assert_eq!(Codec::H264.default_crf(), Some(18));
        assert_eq!(Codec::H265.default_crf(), Some(23));
        assert_eq!(Codec::Vp8.default_crf(), Some(9));
        assert_eq!(Codec::Vp9.default_crf(), Some(28));
        assert_eq!(Codec::ProRes.default_crf(), None);
        assert_eq!(Codec::H264.extension(), "mp4");
        assert_eq!(Codec::Vp9.extension(), "webm");
        assert!(Codec::H264.requires_even_dimensions());
        assert!(!Codec::Vp9.requires_even_dimensions());

        assert!(Codec::H264.validate_crf(18).is_ok());
        assert!(Codec::H264.validate_crf(0).is_err());
        assert!(Codec::H264.validate_crf(52).is_err());
        assert!(Codec::H265.validate_crf(0).is_ok());
    }

    #[test]
    fn even_dimensions_validation() {
        assert_eq!(
            validate_even_dimensions(1920, 1080, Codec::H264).unwrap(),
            (1920, 1080)
        );
        assert_eq!(
            validate_even_dimensions(1921, 1081, Codec::H264).unwrap(),
            (1920, 1080)
        );
        assert_eq!(
            validate_even_dimensions(1921, 1081, Codec::Vp9).unwrap(),
            (1921, 1081)
        );
        assert!(validate_even_dimensions(0, 1080, Codec::H264).is_err());
        assert!(validate_even_dimensions(1, 1080, Codec::H264).is_err());
    }
}
