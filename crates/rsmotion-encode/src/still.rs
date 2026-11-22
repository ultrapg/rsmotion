use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageBuffer, RgbaImage};
use rsmotion_core::ImageFormat;

use crate::error::{EncodeError, Result};
use crate::pipe::InputPixFmt;

/// Writes an RGBA8888 pixel buffer to disk as a PNG file.
pub fn save_rgba_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() != expected {
        return Err(EncodeError::Image(format!(
            "save_rgba_png expected {expected} bytes for {width}x{height}, got {}",
            rgba.len()
        )));
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(path)?;
    let w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| EncodeError::Image(e.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| EncodeError::Image(e.to_string()))?;
    Ok(())
}

/// Converts BGRA8888 pixels into an RGBA8888 buffer.
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = vec![0u8; bgra.len()];
    for (dst, src) in rgba
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(bgra.as_chunks::<4>().0)
    {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
    rgba
}

/// Writes an RGBA8888 or BGRA8888 pixel buffer as PNG, JPEG, or WebP.
pub fn save_still_image(
    path: &Path,
    width: u32,
    height: u32,
    pixels: &[u8],
    input_fmt: InputPixFmt,
    out_fmt: ImageFormat,
    jpeg_quality: u8,
) -> Result<()> {
    let expected = (width as usize) * (height as usize) * 4;
    if pixels.len() != expected {
        return Err(EncodeError::Image(format!(
            "save_still_image expected {expected} bytes for {width}x{height}, got {}",
            pixels.len()
        )));
    }

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let owned_rgba;
    let rgba: &[u8] = match input_fmt {
        InputPixFmt::Rgba => pixels,
        InputPixFmt::Bgra => {
            owned_rgba = bgra_to_rgba(pixels);
            &owned_rgba
        }
        InputPixFmt::Nv12 => {
            return Err(EncodeError::Image(
                "save_still_image does not accept Nv12 pixel input".to_string(),
            ));
        }
    };

    match out_fmt {
        ImageFormat::Png => save_rgba_png(path, width, height, rgba),
        ImageFormat::Jpeg => {
            let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
            for px in rgba.as_chunks::<4>().0 {
                rgb.push(px[0]);
                rgb.push(px[1]);
                rgb.push(px[2]);
            }
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            let mut enc = JpegEncoder::new_with_quality(&mut writer, jpeg_quality.clamp(1, 100));
            enc.encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
                .map_err(|e| EncodeError::Image(e.to_string()))?;
            Ok(())
        }
        ImageFormat::Webp => {
            let img: RgbaImage =
                ImageBuffer::from_raw(width, height, rgba.to_vec()).ok_or_else(|| {
                    EncodeError::Image("failed to construct RGBA image buffer".to_string())
                })?;
            img.save_with_format(path, image::ImageFormat::WebP)
                .map_err(|e| EncodeError::Image(e.to_string()))?;
            Ok(())
        }
    }
}
