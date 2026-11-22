# Phase 2 Report: Tier 2 Rust-CDP Driver & `@rsmotion/renderer` Shim

## 1. Summary

Phase 2 built the complete Tier 2 Chrome DevTools driver (`crates/rsmotion-cdp`), extended `rsmotion-encode` for direct raw BGRA piping, multi-format still saving (PNG/JPEG/WebP), and multi-track PCM audio mixing + AAC muxing, wired real `compositions`, `still`, `render`, and `benchmark` subcommands into `rsmotion-cli`, and delivered the first working `@rsmotion/renderer` Node/TypeScript shim (`npm/renderer-shim/`).

All 12 fixtures (720 total frames) render cleanly in Tier 2 with **`SSIM >= 0.99982`** (`11/12` fixtures at exact `1.00000`) and verified AAC audio on `video-overlay` and `audio-mix`.

## 2. Tier 2 Architecture (`rsmotion-cdp`)

- **Browser launcher (`browser.rs`):** Auto-discovers `.remotion/chrome-headless-shell` (`158.0.8127.2`) or system Chrome, passes `--remotion-shared-memory-capture`, `--disable-features=...,IntensiveWakeUpThrottling,site-per-process,...`, `--font-render-hinting=none`, `--no-zygote`, `--ignore-gpu-blocklist`, isolates temp `--user-data-dir`, and guarantees cleanup on shutdown/drop.
- **Async CDP multiplexer (`client.rs`):** Direct WebSocket (`tokio-tungstenite`) JSON-RPC multiplexing across parallel page sessions without Puppeteer/Node IPC.
- **Embedded HTTP + `/proxy` server (`server.rs`):** Serves the Remotion bundle with CORS + HTTP `Range` support, and implements `/proxy?src=...&time=...` using FFmpeg 16-frame BMP batch extraction and a 48 MB frame cache.
- **Raw BGRA shared-memory capture (`shm.rs`):** Calls `Page.remotionCreateFramePool` and maps `/dev/shm/rmshm-*` via `memmap2`, passing BGRA buffers directly to FFmpeg `-pix_fmt bgra` + `h264_vaapi` (with PNG fallback if standard Chrome is used).
- **Single-roundtrip Remotion handshake (`handshake.rs`):** Reproduces `setPropsAndEnv`, `getStaticCompositions()`, `remotion_setBundleMode`, `remotion_setMediaEnabled`, `remotion_setFrame`, `remotion_renderReady`, `document.fonts.ready`, and `remotion_collectAssets()` inside a single `Runtime.evaluate(awaitPromise: true)` per frame; reuses the evaluation tab as worker 0.
- **Audio mixer (`rsmotion-encode/src/audio.rs`):** Decodes active `<Audio>` / `<OffthreadVideo>` streams to 48 kHz stereo `f32le` PCM, applies frame-accurate volume & playback-rate resampling, and muxes AAC into the final MP4.
- **`@rsmotion/renderer` shim (`npm/renderer-shim/`):** Exports `renderMedia`, `renderStill`, `selectComposition`, `getCompositions`, `ensureBrowser`, and `makeCancelSignal`, streaming `--json-progress` NDJSON events to `onProgress` and supporting cooperative cancellation.

## 3. Diagnostic Findings: GL Backends, Concurrency & Chrome Ceiling

### 3a. GL Backend & Concurrency Exploration (pre-final flags)

| Fixture | GL / Concurrency | Wall (ms) | FPS | Capture Mode | Frame 30 SSIM |
|---|---|---:|---:|---|---:|
| `gradients-shadows` | `angle-egl`, c=2 | 10046.3 | 5.97 | `shm-bgra` | 1.00000 |
| `gradients-shadows` | `swangle`, c=2 | 10733.3 | 5.59 | `shm-bgra` | 1.00000 |
| `gradients-shadows` | `angle`, c=2 | 6588.6 | 9.11 | `shm-bgra` | - |
| `gradients-shadows` | `none`, c=2 | 7759.8 | 7.73 | `shm-bgra` | - |
| `text-basic` | `angle`, c=2 | 5567.6 | 10.78 | `shm-bgra` | - |
| `text-basic` | `none`, c=2 | 4925.1 | 12.18 | `shm-bgra` | - |
| `audio-mix` | `angle-egl`, c=1 | 7177.9 | 8.36 | `shm-bgra` | - |
| `audio-mix` | `angle-egl`, c=2 | 6087.5 | 9.86 | `shm-bgra` | - |
| `audio-mix` | `angle-egl`, c=3 | 7584.2 | 7.91 | `shm-bgra` | - |
| `audio-mix` | `swangle`, c=2 | 7144.5 | 8.40 | `shm-bgra` | - |
| `audio-mix` | `angle`, c=2 | 4735.5 | 12.67 | `shm-bgra` | - |

- **Why `GlRenderer::None` matches Remotion v4:** Inspecting `remotion/packages/renderer/src/options/gl.tsx` showed Remotion v4 defaults to `null` (`GlRenderer::None`) rather than `angle-egl`, and `concurrency=2` outperformed both `1` and `3` on this 4-core CPU.
- **Impact of Chrome anti-throttling flags + `Target.activateTarget` + `--font-render-hinting=none`:**
  - Before adding `IntensiveWakeUpThrottling` disable + `Target.activateTarget`: `text-animated` ran at `6.29 fps` (`0.63×`), `captions` at `6.34 fps` (`0.72×`), `fonts-custom` `min_ssim = 0.99214`.
  - After adding them: `text-animated` jumped to **`11.27 fps` (`1.12×`)**, `captions` jumped to **`11.28 fps` (`1.28×`)**, and `fonts-custom` `min_ssim` reached **`1.00000`**.

## 4. Full 12-Fixture Tier 2 Benchmark (`--concurrency 2 --gl none`)

Measured via `/usr/bin/time -v target/release/rsmotion benchmark --concurrency 2 --gl none --out-dir /tmp/rsmotion-tier2`:

| Fixture | Frames | Remotion FPS | Tier 2 FPS | Tier 2 ms/Frame | Speedup | Capture | Encoder | Audio | Min SSIM |
|---|---:|---:|---:|---:|---:|---|---|---|---|
| `text-basic` | 60 | 10.22 | 9.39 | 106.46 | 0.92× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `text-animated` | 60 | 10.05 | 11.27 | 88.72 | 1.12× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `captions` | 60 | 8.84 | 11.28 | 88.69 | 1.28× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `image-slideshow` | 60 | 8.06 | 10.44 | 95.82 | 1.30× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `transforms-opacity` | 60 | 8.85 | 10.19 | 98.11 | 1.15× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `gradients-shadows` | 60 | 6.27 | 8.28 | 120.71 | 1.32× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `svg-shapes` | 60 | 9.71 | 11.42 | 87.56 | 1.18× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `chart-bars` | 60 | 9.86 | 12.61 | 79.31 | 1.28× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `video-overlay` | 60 | 5.71 | 5.17 | 193.34 | 0.91× | `shm-bgra` | `h264_vaapi` | AAC | 0.99982 |
| `audio-mix` | 60 | 12.30 | 9.62 | 103.92 | 0.78× | `shm-bgra` | `h264_vaapi` | AAC | 1.00000 |
| `fonts-custom` | 60 | 12.55 | 10.89 | 91.82 | 0.87× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| `unsupported-3d` | 60 | 8.46 | 6.61 | 151.19 | 0.78× | `shm-bgra` | `h264_vaapi` | - | 1.00000 |
| **Total / Avg** | **720** | **8.79** | **9.19** | **108.80** | **1.05×** | `shm-bgra` | `h264_vaapi` | **2/2** | **0.99982** |

- **Total wall time:** `78,338.9 ms` vs Remotion baseline `81,952.5 ms` (**1.046×** overall; up to **1.32×** on `gradients-shadows` and **1.30×** on `image-slideshow`).
- **Peak RSS (`rsmotion` process via `/usr/bin/time -v`):** `226,320 KB` (**221.0 MB**, 0 swaps).
- **Verification:** All 12 MP4s verified `1920,1080,60` via `ffprobe`; `video-overlay.mp4` and `audio-mix.mp4` verified `h264,video` + `aac,audio`; all 60 keyframes passed `SSIM >= 0.98` (global minimum `0.99982`); zero orphan Chrome processes left behind.

### Why Tier 2 clusters around ~1.05–1.3× (Key Empirical Finding)

Remotion `4.0.545` already uses `--remotion-shared-memory-capture` (`/dev/shm` BGRA zero-copy) and a Rust compositor in Phase 0. Therefore, replacing the Node CDP client with Rust CDP + VAAPI only removes Node IPC overhead; both tools remain bounded by Chrome Headless Shell's own frame rasterization (`~80–120 ms/frame` on 4 cores). Achieving **5–15×** speedups requires **Phase 3 (Tier 1: V8 + React Reconciler + Taffy + Skia GPU)** without Chrome in the loop.

## 5. `@rsmotion/renderer` Shim Smoke Test (`npm/renderer-shim/test/smoke.mjs`)

- `tsc` build: passed (`npm/renderer-shim/dist/` ignored via `.gitignore`).
- `node test/smoke.mjs` results:
  - `getCompositions`: `12` compositions returned
  - `selectComposition('text-basic')`: `1920x1080 @ 30fps, 60 frames`
  - `renderStill(frame: 30)` vs golden PNG: **`SSIM = 1.0`**
  - `renderMedia('text-basic')`: `60` `onProgress` callbacks (`final progress = 1`), `6044 ms`, `h264_vaapi` + `shm-bgra`, `ffprobe` verified `1920,1080,60`
  - `makeCancelSignal`: verified exported & callable

## 6. Quality Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: passed (`11 passed; 0 failed`)
