use std::path::Path;

use image::{ImageBuffer, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::error::{RasterError, Result};

/// Visual comparison metrics between a golden frame and a rendered frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffReport {
    pub ssim: f64,
    pub psnr: f64,
    pub max_channel_diff: u8,
    pub diff_pixel_ratio: f64,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy)]
struct BlockRect {
    x0: usize,
    y0: usize,
    bw: usize,
    bh: usize,
}

fn ssim_block_channel(a: &[u8], b: &[u8], width: usize, block: BlockRect, ch: usize) -> f64 {
    let n = (block.bw * block.bh) as f64;
    let mut sum_a = 0.0_f64;
    let mut sum_b = 0.0_f64;
    for dy in 0..block.bh {
        let row = (block.y0 + dy) * width;
        for dx in 0..block.bw {
            let idx = (row + block.x0 + dx) * 4 + ch;
            sum_a += f64::from(a[idx]);
            sum_b += f64::from(b[idx]);
        }
    }
    let mu_a = sum_a / n;
    let mu_b = sum_b / n;

    let mut var_a = 0.0_f64;
    let mut var_b = 0.0_f64;
    let mut cov_ab = 0.0_f64;
    for dy in 0..block.bh {
        let row = (block.y0 + dy) * width;
        for dx in 0..block.bw {
            let idx = (row + block.x0 + dx) * 4 + ch;
            let da = f64::from(a[idx]) - mu_a;
            let db = f64::from(b[idx]) - mu_b;
            var_a += da * da;
            var_b += db * db;
            cov_ab += da * db;
        }
    }

    let denom_n = if n > 1.0 { n - 1.0 } else { 1.0 };
    var_a /= denom_n;
    var_b /= denom_n;
    cov_ab /= denom_n;

    const C1: f64 = (0.01 * 255.0) * (0.01 * 255.0);
    const C2: f64 = (0.03 * 255.0) * (0.03 * 255.0);

    let num = (2.0 * mu_a * mu_b + C1) * (2.0 * cov_ab + C2);
    let den = (mu_a * mu_a + mu_b * mu_b + C1) * (var_a + var_b + C2);
    (num / den).clamp(-1.0, 1.0)
}

/// Computes windowed 8x8 RGB SSIM, PSNR, max channel diff, and differing-pixel ratio.
pub fn compare_rgba(golden: &[u8], actual: &[u8], width: u32, height: u32) -> Result<DiffReport> {
    let expected = (width as usize) * (height as usize) * 4;
    if golden.len() != expected {
        return Err(RasterError::InvalidBufferLength {
            expected,
            actual: golden.len(),
        });
    }
    if actual.len() != expected {
        return Err(RasterError::InvalidBufferLength {
            expected,
            actual: actual.len(),
        });
    }

    let w = width as usize;
    let h = height as usize;
    let pixel_count = w * h;

    let mut sq_err_sum = 0.0_f64;
    let mut max_channel_diff = 0u8;
    let mut diff_pixels = 0usize;

    for i in 0..pixel_count {
        let base = i * 4;
        let dr = golden[base].abs_diff(actual[base]);
        let dg = golden[base + 1].abs_diff(actual[base + 1]);
        let db = golden[base + 2].abs_diff(actual[base + 2]);
        let da = golden[base + 3].abs_diff(actual[base + 3]);

        let max_p = dr.max(dg).max(db).max(da);
        if max_p > 0 {
            diff_pixels += 1;
        }
        if max_p > max_channel_diff {
            max_channel_diff = max_p;
        }
        sq_err_sum += f64::from(dr) * f64::from(dr)
            + f64::from(dg) * f64::from(dg)
            + f64::from(db) * f64::from(db);
    }

    let mse = sq_err_sum / ((pixel_count as f64) * 3.0);
    let psnr = if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * ((255.0 * 255.0) / mse).log10()
    };

    let mut ssim_sum = 0.0_f64;
    let mut blocks = 0usize;
    let mut y = 0usize;
    while y < h {
        let bh = (h - y).min(8);
        let mut x = 0usize;
        while x < w {
            let bw = (w - x).min(8);
            let s_r = ssim_block_channel(
                golden,
                actual,
                w,
                BlockRect {
                    x0: x,
                    y0: y,
                    bw,
                    bh,
                },
                0,
            );
            let s_g = ssim_block_channel(
                golden,
                actual,
                w,
                BlockRect {
                    x0: x,
                    y0: y,
                    bw,
                    bh,
                },
                1,
            );
            let s_b = ssim_block_channel(
                golden,
                actual,
                w,
                BlockRect {
                    x0: x,
                    y0: y,
                    bw,
                    bh,
                },
                2,
            );
            ssim_sum += (s_r + s_g + s_b) / 3.0;
            blocks += 1;
            x += 8;
        }
        y += 8;
    }

    let ssim = if blocks > 0 {
        ssim_sum / (blocks as f64)
    } else {
        1.0
    };

    Ok(DiffReport {
        ssim,
        psnr,
        max_channel_diff,
        diff_pixel_ratio: (diff_pixels as f64) / (pixel_count.max(1) as f64),
        width,
        height,
    })
}

/// Writes an RGB diff heatmap PNG highlighting pixel differences over a dimmed grayscale background.
pub fn write_heatmap_png(
    golden: &[u8],
    actual: &[u8],
    width: u32,
    height: u32,
    out_path: &Path,
) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let mut img: RgbaImage = ImageBuffer::new(width, height);
    for (idx, pixel) in img.pixels_mut().enumerate() {
        let base = idx * 4;
        let dr = golden[base].abs_diff(actual[base]);
        let dg = golden[base + 1].abs_diff(actual[base + 1]);
        let db = golden[base + 2].abs_diff(actual[base + 2]);
        let delta = dr.max(dg).max(db);

        let lum = ((u16::from(golden[base]) * 77
            + u16::from(golden[base + 1]) * 150
            + u16::from(golden[base + 2]) * 29)
            >> 10) as u8;

        if delta == 0 {
            *pixel = Rgba([lum, lum, lum, 255]);
        } else {
            let intensity = delta.saturating_mul(4).max(80);
            let green = if delta > 32 {
                delta.saturating_sub(32).saturating_mul(2)
            } else {
                0
            };
            *pixel = Rgba([intensity, green, 0, 255]);
        }
    }

    img.save(out_path)?;
    Ok(())
}

/// Compares two PNG files and optionally writes a diff heatmap PNG.
pub fn compare_png_files(
    golden_path: &Path,
    actual_path: &Path,
    heatmap_out: Option<&Path>,
) -> Result<DiffReport> {
    let g_img = image::open(golden_path)?.to_rgba8();
    let a_img = image::open(actual_path)?.to_rgba8();

    let (gw, gh) = g_img.dimensions();
    let (aw, ah) = a_img.dimensions();
    if gw != aw || gh != ah {
        return Err(RasterError::DimensionMismatch { gw, gh, aw, ah });
    }

    let g_raw = g_img.as_raw();
    let a_raw = a_img.as_raw();
    let report = compare_rgba(g_raw, a_raw, gw, gh)?;

    if let Some(hm_path) = heatmap_out {
        write_heatmap_png(g_raw, a_raw, gw, gh, hm_path)?;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_pattern(w: u32, h: u32) -> Vec<u8> {
        let mut buf = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                buf[i] = ((x * 7 + y * 3) % 255) as u8;
                buf[i + 1] = ((x * 2 + y * 11) % 255) as u8;
                buf[i + 2] = ((x * 5 + y * 5) % 255) as u8;
                buf[i + 3] = 255;
            }
        }
        buf
    }

    #[test]
    fn identical_images_have_ssim_one() {
        let img = synthetic_pattern(64, 64);
        let rep = compare_rgba(&img, &img, 64, 64).unwrap();
        assert!(rep.ssim > 0.9999, "ssim={}", rep.ssim);
        assert!(rep.psnr.is_infinite());
        assert_eq!(rep.max_channel_diff, 0);
        assert_eq!(rep.diff_pixel_ratio, 0.0);
    }

    #[test]
    fn inverted_images_have_low_ssim() {
        let img = synthetic_pattern(64, 64);
        let inv: Vec<u8> = img
            .chunks_exact(4)
            .flat_map(|p| [255 - p[0], 255 - p[1], 255 - p[2], 255])
            .collect();
        let rep = compare_rgba(&img, &inv, 64, 64).unwrap();
        assert!(rep.ssim < 0.2, "ssim={}", rep.ssim);
    }

    #[test]
    fn slight_noise_gives_high_but_imperfect_ssim() {
        let img = synthetic_pattern(64, 64);
        let mut noisy = img.clone();
        for (idx, chunk) in noisy.chunks_exact_mut(4).enumerate() {
            let delta: i16 = if idx % 2 == 0 { 6 } else { -6 };
            chunk[0] = (i16::from(chunk[0]) + delta).clamp(0, 255) as u8;
            chunk[1] = (i16::from(chunk[1]) + delta).clamp(0, 255) as u8;
            chunk[2] = (i16::from(chunk[2]) + delta).clamp(0, 255) as u8;
        }
        let rep = compare_rgba(&img, &noisy, 64, 64).unwrap();
        assert!(
            rep.ssim > 0.85 && rep.ssim < 0.995,
            "unexpected ssim={}",
            rep.ssim
        );
    }
}
