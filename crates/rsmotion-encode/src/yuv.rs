/// Converts packed RGBA8888 or BGRA8888 pixels directly into planar NV12 (`Y` + interleaved `UV`).
///
/// Uses BT.601 studio-swing coefficients (`Y: 16..=235`, `UV: 16..=240`) matching FFmpeg's default
/// `swscale` RGB-to-YUV conversion. Requires even `width` and `height`.
pub fn convert_rgba_or_bgra_to_nv12(
    src: &[u8],
    dst: &mut [u8],
    width: u32,
    height: u32,
    is_bgra: bool,
) {
    let w = width as usize;
    let h = height as usize;
    debug_assert_eq!(w % 2, 0);
    debug_assert_eq!(h % 2, 0);
    debug_assert_eq!(src.len(), w * h * 4);
    debug_assert_eq!(dst.len(), (w * h * 3) / 2);

    let y_len = w * h;
    let (y_plane, uv_plane) = dst.split_at_mut(y_len);
    let stride = w * 4;
    let (r_i, b_i) = if is_bgra { (2, 0) } else { (0, 2) };

    for py in 0..(h / 2) {
        let y0 = py * 2;
        let y1 = y0 + 1;
        let src0 = &src[y0 * stride..(y0 + 1) * stride];
        let src1 = &src[y1 * stride..(y1 + 1) * stride];
        let (dst_y0, dst_y1) = y_plane[y0 * w..(y1 + 1) * w].split_at_mut(w);
        let dst_uv = &mut uv_plane[py * w..(py + 1) * w];

        for px in 0..(w / 2) {
            let x0 = px * 2;
            let x1 = x0 + 1;
            let p00 = &src0[x0 * 4..x0 * 4 + 4];
            let p01 = &src0[x1 * 4..x1 * 4 + 4];
            let p10 = &src1[x0 * 4..x0 * 4 + 4];
            let p11 = &src1[x1 * 4..x1 * 4 + 4];

            let (r00, g00, b00) = (i32::from(p00[r_i]), i32::from(p00[1]), i32::from(p00[b_i]));
            let (r01, g01, b01) = (i32::from(p01[r_i]), i32::from(p01[1]), i32::from(p01[b_i]));
            let (r10, g10, b10) = (i32::from(p10[r_i]), i32::from(p10[1]), i32::from(p10[b_i]));
            let (r11, g11, b11) = (i32::from(p11[r_i]), i32::from(p11[1]), i32::from(p11[b_i]));

            dst_y0[x0] = (((66 * r00 + 129 * g00 + 25 * b00 + 128) >> 8) + 16).clamp(16, 235) as u8;
            dst_y0[x1] = (((66 * r01 + 129 * g01 + 25 * b01 + 128) >> 8) + 16).clamp(16, 235) as u8;
            dst_y1[x0] = (((66 * r10 + 129 * g10 + 25 * b10 + 128) >> 8) + 16).clamp(16, 235) as u8;
            dst_y1[x1] = (((66 * r11 + 129 * g11 + 25 * b11 + 128) >> 8) + 16).clamp(16, 235) as u8;

            let r_avg = (r00 + r01 + r10 + r11 + 2) >> 2;
            let g_avg = (g00 + g01 + g10 + g11 + 2) >> 2;
            let b_avg = (b00 + b01 + b10 + b11 + 2) >> 2;

            dst_uv[x0] =
                (((-38 * r_avg - 74 * g_avg + 112 * b_avg + 128) >> 8) + 128).clamp(16, 240) as u8;
            dst_uv[x1] =
                (((112 * r_avg - 94 * g_avg - 18 * b_avg + 128) >> 8) + 128).clamp(16, 240) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_black_and_white_rgba_and_bgra_to_nv12() {
        // 2x2 black RGBA -> Y=16, U=128, V=128
        let black = [0u8, 0, 0, 255].repeat(4 * 4);
        let mut nv12 = vec![0u8; 4 * 4 * 3 / 2];
        convert_rgba_or_bgra_to_nv12(&black, &mut nv12, 4, 4, false);
        assert!(nv12[..16].iter().all(|&y| y == 16));
        assert!(nv12[16..].iter().all(|&uv| uv == 128));

        // 2x2 white BGRA -> Y=235, U=128, V=128
        let white = [255u8, 255, 255, 255].repeat(4);
        let mut nv12_w = vec![0u8; 6];
        convert_rgba_or_bgra_to_nv12(&white, &mut nv12_w, 2, 2, true);
        assert_eq!(&nv12_w[..4], &[235, 235, 235, 235]);
        assert_eq!(&nv12_w[4..], &[128, 128]);
    }
}
