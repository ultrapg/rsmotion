# Phase 1 Report: Core Types, FFmpeg Pipe Encoder, Skia GPU/CPU Spike & SSIM Diff

## 1. Summary

Phase 1 implemented the foundational Rust workspace (`rsmotion-core`, `rsmotion-encode`, `rsmotion-raster`, `rsmotion-cli`), verified prebuilt `skia-safe` 0.157.0 linking without C++ compilation, probed hardware video encoding on the Intel Skylake-U host, stood up headless EGL/OpenGL ES 3.2 GPU rasterization on Intel Iris 540, and built the windowed RGB SSIM `diff` tool.

## 2. Encoder Probe & `spike-encode` Results (100 frames, 1920×1080 RGBA pipe)

| Requested | Selected Encoder | Status | Wall (ms) | FPS | ms/Frame | Notes |
|---|---|---|---:|---:|---:|---|
| `auto` | `h264_vaapi` | Pass (ffprobe `1920x1080x100`) | 2378.7 | 42.04 | 23.79 | Uses `/dev/dri/renderD128` (`VAEntrypointEncSliceLP`), `-vf format=nv12,hwupload`, `-qp 20` |
| `qsv` | `h264_qsv` | Unavailable | - | - | - | `Error creating a MFX session: -9` |
| `software` | `libx264` | Pass (ffprobe `1920x1080x100`) | 14642.0 | 6.83 | 146.42 | `-crf 20 -preset medium` |

- **Speedup:** `h264_vaapi` is **6.16× faster** than `libx264` (`medium`) on this machine.
- **Pipe ceiling observation:** Pushing 100 uncompressed 1920×1080 RGBA frames (`829.4 MB` total) over `pipe:0` plus CPU `format=nv12,hwupload` caps `h264_vaapi` at ~42 fps (compared to the initial `<0.5s` goal). Producing **NV12 directly** (or DMA-BUF/YUV conversion in Rust) is logged for Phase 6 optimization to bypass FFmpeg's CPU swscale RGBA→NV12 pass.

## 3. Skia Raster Spike: CPU vs Headless EGL GPU (100 frames, 1920×1080)

Scene rendered per frame: animated linear gradient background, 4 rounded cards with `drop_shadow`, embedded decoded PNG (`sample-image.png`), rotating vector `Path`, and multi-line Skia `Paragraph` text.

### Hardware GL Verification
- `egl_platform`: `EGL_PLATFORM_SURFACELESS_MESA`
- `GL_VENDOR`: `Intel`
- `GL_RENDERER`: `Mesa Intel(R) Iris(R) Graphics 540 (SKL GT3)`
- `GL_VERSION`: `OpenGL ES 3.2 Mesa 26.2.7 - kisak-mesa PPA`
- Note: Mesa surfaceless EGL requires `EGL_SURFACE_TYPE = EGL_PBUFFER_BIT` in `eglChooseConfig` (omitting `SURFACE_TYPE` defaults to `EGL_WINDOW_BIT` and returns no configs).

### 3a. Pure Raster + Readback (No Encode)

| Backend | Draw Avg (ms) | Readback Avg (ms) | Total (ms) | Effective FPS |
|---|---:|---:|---:|---:|
| CPU (`surfaces::raster`, `RGBA8888`) | 58.53 | 0.70 | 5939.2 | 16.84 |
| GPU (Intel Iris 540, `render_target`) | 2.45 | 6.09 | 867.5 | 115.27 |

- GPU raster + readback is **6.85× faster** than CPU Skia (`115.27 fps` vs `16.84 fps`).
- **Frame 30 CPU vs GPU visual parity (`compare_rgba`):**
  - **SSIM:** `0.99941`
  - **PSNR:** `55.99 dB`
  - `max_channel_diff`: `55` (anti-aliasing edge differences), `diff_pixel_ratio`: `0.1641`

### 3b. End-to-End Raster + Readback + VAAPI Encode (`h264_vaapi`)

| Run | Backend | Draw Avg (ms) | Readback Avg (ms) | Encode Push Avg (ms) | Total (ms) | Effective FPS | Peak RSS (`time -v`) |
|---|---|---:|---:|---:|---:|---:|---:|
| Isolated | `cpu` | 62.20 | 0.89 | 0.90 | 6447.7 | 15.51 | 108.2 MB (`110,808 KB`) |
| Isolated | `gpu` | 9.89 | 8.21 | 5.27 | 2417.6 | 41.36 | 174.8 MB (`178,988 KB`) |
| Combined (`both`, warm GPU) | `cpu` | 56.10 | 0.91 | 0.83 | 5852.7 | 17.09 | 159.6 MB (`163,440 KB`) |
| Combined (`both`, warm GPU) | `gpu` | 4.70 | 7.87 | 4.18 | 1794.3 | 55.73 | 159.6 MB (`163,440 KB`) |

- Both `/tmp/spike-raster-cpu/spike-cpu.mp4` and `/tmp/spike-raster-gpu/spike-gpu.mp4` verified via `ffprobe` as `1920,1080,100`.
- Notice that even with simultaneous GPU rendering + VAAPI hardware encoding on the Skylake-U iGPU, Peak RSS remained **174.8 MB** (well below the 5.5 GiB ceiling) with zero stability issues, and end-to-end throughput reached **41.4–55.7 fps** (4.7–6.3× faster than Remotion's 8.79 fps baseline average).

## 4. `rsmotion diff` Verification

- `frame_0000.png` vs `frame_0000.png` (`tests/golden/text-basic`): `ssim = 1.0`, `psnr = null` (`inf`), `max_channel_diff = 0`, `diff_pixel_ratio = 0.0`.
- `frame_0000.png` vs `frame_0059.png` (`tests/golden/text-basic`): `ssim = 0.97291`, `psnr = 29.52 dB`, `max_channel_diff = 70`, `diff_pixel_ratio = 0.02060`; heatmap PNG generated (`/tmp/spike-diff-heatmap.png`, 70 KB).

## 5. Quality Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: passed (`10 passed; 0 failed`: 4 `rsmotion-core`, 3 `rsmotion-encode`, 3 `rsmotion-raster`)

## 6. Verdict for Phase 3 Default Raster Backend

- **Recommendation: `--raster=gpu` (EGL surfaceless on Intel Iris 540) as the default (`RasterBackend::Auto -> GPU with CPU fallback`).**
- Rationale:
  1. Pure draw time drops from **~58.5 ms** (CPU) to **2.45–9.89 ms** (GPU) with shadows/gradients/images/text.
  2. Even including GL readback (`6.1–8.2 ms`), GPU + VAAPI reaches **41.4–55.7 fps** vs CPU + VAAPI **15.5–17.1 fps**, and shifts workload off the 4 CPU cores (which Phase 3 will need for V8 + Taffy layout).
  3. Memory stays tiny (`~175 MB` Peak RSS) and visual agreement with CPU Skia is `SSIM = 0.99941`.
  4. `--raster=cpu` stays fully implemented as deterministic reference and fallback if EGL initialization fails in a restricted environment.
