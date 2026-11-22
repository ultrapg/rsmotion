# Phase 4 Report: Native Tier 1 Media (`rsmotion-media` + `<OffthreadVideo>` / `<Video>` / `<Audio>`)

## 1. Summary

Phase 4 eliminates Chrome and local HTTP proxy overhead for video and audio compositions in Tier 1 by introducing `crates/rsmotion-media` and connecting `<OffthreadVideo>`, `<Video>`, and `<Audio>` end-to-end across `rsmotion-js`, `rsmotion-layout`, `rsmotion-raster`, and `rsmotion-cli`.

Across all **11 Tier 1 fixtures** (`660` total frames, `55` golden keyframes), Tier 1 GPU achieves **`28.96 fps` overall (`3.28×` faster than Remotion `4.0.545` baseline at `8.82 fps`)** with **`globalMinSsim = 0.98455`** and **`254.2 MB` Peak RSS** (`0` swaps, zero Chrome processes):
- **`video-overlay`:** **`20.32 fps`** (`3.56×` vs `5.71 fps` baseline), **`minSsim = 0.99870`**, `avgSsim = 0.99885`, **`hasAudio = true`**, `vid=266.1ms` (`4.4 ms/frame` for 60 decoded frames).
- **`audio-mix`:** **`25.29 fps`** (`2.06×` vs `12.30 fps` baseline), **`minSsim = 0.99824`**, `avgSsim = 0.99848`, **`hasAudio = true`**.

## 2. Phase 4 Architecture

- **`crates/rsmotion-media` (`uri.rs`, `probe.rs`, `decoder.rs`, `error.rs`):**
  - Internal `rsmotion-video://frame?src=...&time=...&transparent=...` URI formatter and parser + local bundle path resolver (`resolve_media_path`).
  - `probe_media` wraps `ffprobe` once per media asset to cache dimensions, FPS (`r_frame_rate`/`avg_frame_rate`), duration, `nb_frames`, codec name, and audio/video stream presence.
  - `VideoDecoderPool` maintains persistent streaming FFmpeg raw RGBA pipes for sequential forward decoding (no per-frame process respawn), only seeking/respawning on backward jumps or skips `> 12` frames, backed by a bounded `64 MB` FIFO RGBA frame cache and `Drop`-safe child cleanup.
- **`crates/rsmotion-js` (`ops.rs`, `host.js`, `runtime.rs`):**
  - Added synchronous `op_probe_media` op and enabled `window.remotion_proxyPort = 1`.
  - Upgraded `EventTarget` (`{ once: true }`), added `RSMBlob` and `URL.createObjectURL` / `URL.revokeObjectURL`, and intercepted `/proxy` `fetch()` calls directly in JS so `<OffthreadVideo>` metadata resolves instantaneously in V8 with zero network/HTTP server traffic.
  - Added `HTMLMediaElement` `videoWidth`/`videoHeight`/`duration`/`currentTime`/`requestVideoFrameCallback`/`seeked` emulation for `<video>` and `<audio>` tags, rewriting local `<img blob:...>` and `<video>` frame URLs into `rsmotion-video://frame?...` inside `__rsmotion_snapshot` while leaving only remote unresolvable videos flagged (`remote-video`).
- **`crates/rsmotion-layout` & `crates/rsmotion-raster`:**
  - Added `LayoutContent::VideoFrame`, `MeasurePayload::Video`, and `<video>`/`<audio>` UA defaults in `rsmotion-layout`.
  - Integrated `VideoDecoderPool` into `SceneRenderer`, hashing `(src, quantized time_secs, transparent)` in `hash_layout_tree`, wrapping decoded RGBA frames via `skia_safe::images::raster_from_data`, and rendering with full CSS `object-fit` and `border-radius` clipping on GPU/CPU Skia.
- **`crates/rsmotion-cli` (`src/tier1.rs`, `src/main.rs`):**
  - Collected `scene.audio_assets` across frames in `render_media_tier1`, remapping timeline frames to sequential output indices and mixing/muxing multi-track PCM + AAC via `rsmotion_encode::mix_and_mux_audio`.
  - Extended `Tier1RenderReport` and `Tier1BenchFixtureRow` with `hasAudio`, `videoDecodeMs`, and `audioMs`, and verified audio streams in `benchmark --tier 1` (`muted: false`).

## 3. Verification & Benchmark Results

### A. Still Frame Fidelity Checks (`video-overlay` & `audio-mix`)
- `video-overlay` frame `0`: **`SSIM = 0.99870`**, `PSNR = 40.00 dB`, `unsupported_features = []`
- `video-overlay` frame `30`: **`SSIM = 0.99895`**, `PSNR = 40.12 dB`, `unsupported_features = []`
- `audio-mix` frame `30`: **`SSIM = 0.99851`**, `PSNR = 37.51 dB`, `unsupported_features = []`

### B. 11-Fixture Tier 1 GPU Benchmark (`660` Frames, `/tmp/rsmotion-bench-p4-gpu`)
Measured via `/usr/bin/time -v ./target/release/rsmotion benchmark --tier 1 --raster gpu --min-ssim 0.97 --fixtures text-basic,text-animated,captions,image-slideshow,transforms-opacity,gradients-shadows,svg-shapes,chart-bars,video-overlay,audio-mix,fonts-custom --out-dir /tmp/rsmotion-bench-p4-gpu`:

| Fixture | Baseline FPS | Tier 1 GPU FPS | Speedup | Audio | Vid Decode (60f) | Audio Mux | Draw (60f) | Min SSIM (5 kf) | Avg SSIM |
|---|---:|---:|---:|---|---:|---:|---:|---:|---:|
| `text-basic` | 10.22 | 26.94 | 2.63× | false | 0.0 ms | 0.0 ms | 85.1 ms | 0.99374 | 0.99529 |
| `text-animated` | 10.05 | 31.89 | 3.17× | false | 0.0 ms | 0.0 ms | 166.0 ms | 0.99624 | 0.99706 |
| `captions` | 8.84 | 30.49 | 3.45× | false | 0.0 ms | 0.0 ms | 44.5 ms | 0.98981 | 0.98996 |
| `image-slideshow` | 8.06 | 37.14 | 4.61× | false | 0.0 ms | 0.0 ms | 115.5 ms | 0.99735 | 0.99833 |
| `transforms-opacity` | 8.85 | 40.08 | 4.53× | false | 0.0 ms | 0.0 ms | 55.8 ms | 0.99983 | 0.99991 |
| `gradients-shadows` | 6.27 | 31.01 | 4.95× | false | 0.0 ms | 0.0 ms | 162.6 ms | 0.98608 | 0.98801 |
| `svg-shapes` | 9.71 | 41.82 | 4.31× | false | 0.0 ms | 0.0 ms | 141.2 ms | 0.99967 | 0.99967 |
| `chart-bars` | 9.86 | 19.86 | 2.02× | false | 0.0 ms | 0.1 ms | 2,090.9 ms | 0.99897 | 0.99914 |
| `video-overlay` | 5.71 | 20.32 | 3.56× | true | 266.1 ms | 498.3 ms | 176.4 ms | 0.99870 | 0.99885 |
| `audio-mix` | 12.30 | 25.29 | 2.06× | true | 0.0 ms | 597.4 ms | 35.4 ms | 0.99824 | 0.99848 |
| `fonts-custom` | 12.55 | 31.81 | 2.54× | false | 0.0 ms | 0.0 ms | 41.8 ms | 0.98455 | 0.98459 |
| **Total / Avg** | **8.82** | **28.96** | **3.28×** | **2/2** | **266.1 ms** | **1,095.7 ms** | **3,115.2 ms** | **0.98455** | **—** |

- **Peak RSS:** `260,264 KB` (`254.2 MB`), `Swaps: 0`, elapsed wall time `32.61 s` (including `ffprobe` & 55 keyframe diffs), encoder `h264_vaapi`, GPU `Mesa Intel(R) Iris(R) Graphics 540 (SKL GT3)`.

### C. CPU Raster Comparison & Pure CLI Render Checks
- **CPU (`--raster cpu`) benchmark on media fixtures:**
  - `video-overlay`: `8.55 fps` (`7,021.5 ms`, `1.50×`, `draw=3,037.3 ms`, `minSsim=0.99871`, `audio=true`) — GPU compositing/scaling is **`17.2×` faster** than CPU Skia on `video-overlay` (`176.4 ms` vs `3,037.3 ms`).
  - `audio-mix`: `18.76 fps` (`3,199.1 ms`, `1.52×`, `draw=360.8 ms`, `minSsim=0.99826`, `audio=true`).
- **Standalone cold `rsmotion render --force-tier 1 --raster gpu`:**
  - `video-overlay`: `5,102.7 ms` (`11.76 fps`, `has_audio=true`)
  - `audio-mix`: `3,010.5 ms` (`19.93 fps`, `has_audio=true`)

### D. Audio Stream & Gain Envelope Verification (`ffprobe` + `ffmpeg volumedetect`)
- `ffprobe` on `video-overlay.mp4` & `audio-mix.mp4`: `1920x1080`, `60` H.264 frames (`2.000s`), AAC stereo `48000 Hz` (`2.000s`, `94` frames).
- `video-overlay.mp4`: `mean_volume: -24.1 dB`, `max_volume: -19.2 dB`.
- `audio-mix.mp4` full track: `mean_volume: -28.0 dB`, `max_volume: -20.9 dB`.
- `audio-mix.mp4` envelope segments (verifying `0.2 -> 1.0 -> 0.2` frame volume curve):
  - `0.0–0.3s`: `mean_volume: -34.2 dB`, `max_volume: -28.6 dB`
  - `0.85–1.15s`: `mean_volume: -24.6 dB`, `max_volume: -20.9 dB`
  - `1.7–2.0s`: `mean_volume: -34.0 dB`, `max_volume: -28.6 dB`

## 4. Quality & Hygiene Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: passed (`20 passed; 0 failed` across `rsmotion-core` (4), `rsmotion-encode` (4), `rsmotion-js` (2), `rsmotion-layout` (2), `rsmotion-media` (3), `rsmotion-raster` (4), `rsmotion-scene` (1))
- Process hygiene (`pgrep -af 'chrome-headless-shell|ffmpeg'`): 0 leftover child processes

## 5. Next Steps

Proceed to **Phase 5 (`rsmotion-analyze` & Automatic Tier 1/2/3 Dispatch)**: static + V8 probe classification so default `rsmotion render` / `still` / `compositions` route automatically (`unsupported-3d` -> Tier 2, remaining 11 fixtures -> Tier 1) and implement `rsmotion explain` / `--explain`.
