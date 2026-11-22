# Phase 9 Report: Native `<canvas>` 2D, Skottie Lottie, WOFF2/Google Fonts & `@remotion/media-utils` Audio

## 1. Summary

Phase 9 eliminates Chrome/Tier 2 fallbacks across 2D Canvas drawing, Lottie vector animations, web fonts (WOFF/WOFF2 + Google Fonts CSS), and browser Web Audio / MediaBunny decoding workflows, raising Tier 1 native coverage on the expanded 18-fixture suite to **17 / 18 fixtures** (**`1,080` frames**, with only WebGL2 in `unsupported-webgl` routed to Tier 2 Chrome CDP) and delivering **33.94 fps overall** (**`8.97×`** vs Remotion `285,492.6 ms`, up to **`26.89×`** on `video-overlay` and **`81.34 fps`** on `lottie-native`) at **329.7 MiB Peak RSS** (`337,660 KB`) and **`Swaps: 0`**, while passing all **6 / 6 upstream Remotion templates** (`template-blank`, `template-helloworld`, `template-still`, `template-overlay`, `template-audiogram`, `template-music-visualization`) natively in Tier 1 with **`SSIM >= 0.9614`**.

1. **HTML5 `<canvas>` 2D → GPU Skia (`rsmotion-scene`, `rsmotion-js`, `crates/rsmotion-raster/src/canvas2d.rs`):**
   - Added serializable `Canvas2DOp` command buffers (`NodeKind::Canvas2d`) and V8 `CanvasRenderingContext2D` / `Path2D` / `CanvasGradient` state-stack shims plus synchronous `op_canvas_measure_text` via Skia `FontCache::measure_canvas_text`, keeping `<canvas getContext("webgl"|"webgl2"|"bitmaprenderer")>` routed cleanly to Tier 2 (`canvas-webgl`).
   - Executed Canvas 2D vector, text (`textAlign`, `textBaseline`), image, gradient, dash, blend, shadow, clip, and CSS-filter operations directly on the hardware EGL Skia GPU canvas.
2. **Native Lottie Rendering via `skia_safe::skottie` (`crates/rsmotion-raster/src/lottie_cache.rs`, `rsmotion-js`, `rsmotion-scene`):**
   - Registered `@remotion/lottie` `loadAnimation` / `goToAndStop` payloads as `LottieSource` + `NodeKind::Lottie`, cached `skottie::Animation` instances with project font & asset managers, and rendered `seek_frame` + `preserveAspectRatio` fit directly on Skia GPU (`81.34 fps`, `0.99923` SSIM on `lottie-native`).
3. **WOFF/WOFF2 & Google Fonts + `--offline` Policy (`crates/rsmotion-media/src/asset_cache.rs`, `font_cache.rs`, `rsmotion-css`):**
   - Added pure-Rust `wuff` WOFF/WOFF2 → SFNT decoding, `ureq` remote asset caching under `~/.cache/rsmotion/assets/`, `--offline` opt-out plumbing across CLI/analyzer/runtime, per-weight/subset `@font-face` deduplication in `rsmotion-css`, variable-font `wght` variation registration, CSS `half_leading(true)`, and `white-space: pre`/`nowrap` layout width fixes (`template-still` at **`0.9931` SSIM**).
4. **Replaced-Element Aspect Ratio Constraints (`crates/rsmotion-layout/src/engine.rs`):**
   - Transferred intrinsic `aspect_ratio` and mutual `max_size`/`min_size` constraints onto auto-sized replaced elements inside flex containers without breaking explicit `width` + `height` boxes (`template-audiogram` improved from `0.9048` to **`0.9913` SSIM**).
5. **`@remotion/media-utils` & `mediabunny` In-Process Audio Decoding (`crates/rsmotion-media/src/audio_data.rs`, `ops.rs`, `host.js`):**
   - Added `symphonia 0.5.5` `StreamingAudioDecoder` (`mp3`, `aac`, `flac`, `pcm`, `vorbis`, `alac`) plus FFmpeg PCM decoding, HTTP `Range` `fetch`, `HTMLScriptElement` dynamic chunk loading, `RSMAudioContext`, `RSMAudioData` (`s16`/`s16-planar`/`f32`), and `RSMAudioDecoder` shims so `getAudioData()`, `useAudioData()`, `useWindowedAudioData()`, and `visualizeAudio()` run chrome-free in Tier 1.
6. **4 New Fixtures & Baselines (`tests/fixtures`, `tests/golden`, `bench/baseline.json`):**
   - Added `Canvas2dDemo.tsx` (`canvas-2d`), `LottieNative.tsx` (`lottie-native`), `AudioViz.tsx` (`audio-viz`), and `UnsupportedWebgl.tsx` (`unsupported-webgl`), rebuilt golden PNGs across all 18 fixtures, and updated `bench/baseline.json` (**`285,492.6 ms`** / **`3.78 fps`** Chrome baseline over `1,080` frames).

---

## 2. Final 18-Fixture Automatic Benchmark (`--tier auto --raster gpu`)

Measured with `/usr/bin/time -v ./target/release/rsmotion benchmark --tier auto --raster gpu --concurrency 2 --gl none --min-ssim 0.97 --out-dir /tmp/rsmotion-bench-p9-auto`:

| Fixture | Chosen Tier | Remotion FPS | Auto FPS | ms/Frame | Speedup | Audio | Min SSIM (5 kf) | Avg SSIM |
|---|---|---:|---:|---:|---:|---|---:|---:|
| `text-basic` | 1 (`tier1-native`) | 8.40 | 55.76 | 17.93 | 6.64× | false | 0.99374 | 0.99529 |
| `text-animated` | 1 (`tier1-native`) | 5.83 | 58.96 | 16.96 | 10.11× | false | 0.99624 | 0.99706 |
| `captions` | 1 (`tier1-native`) | 7.41 | 70.30 | 14.22 | 9.49× | false | 0.98981 | 0.98996 |
| `image-slideshow` | 1 (`tier1-native`) | 4.69 | 41.99 | 23.82 | 8.94× | false | 0.99735 | 0.99833 |
| `transforms-opacity` | 1 (`tier1-native`) | 7.34 | 62.52 | 16.00 | 8.51× | false | 0.99983 | 0.99991 |
| `gradients-shadows` | 1 (`tier1-native`) | 6.35 | 45.75 | 21.86 | 7.20× | false | 0.98608 | 0.98801 |
| `svg-shapes` | 1 (`tier1-native`) | 7.82 | 31.17 | 32.08 | 3.98× | false | 0.99967 | 0.99967 |
| `chart-bars` | 1 (`tier1-native`) | 5.71 | 31.42 | 31.83 | 5.50× | false | 0.99897 | 0.99914 |
| `video-overlay` | 1 (`tier1-native`) | 0.64 | 17.09 | 58.52 | **26.89×** | true | 0.99870 | 0.99884 |
| `audio-mix` | 1 (`tier1-native`) | 1.49 | 24.23 | 41.27 | **16.31×** | true | 0.99824 | 0.99848 |
| `fonts-custom` | 1 (`tier1-native`) | 6.79 | 63.12 | 15.84 | 9.29× | false | 0.98455 | 0.98459 |
| `css-tailwind-grid` | 1 (`tier1-native`) | 6.83 | 32.59 | 30.69 | 4.77× | false | 0.99069 | 0.99081 |
| `css-effects-3d` | 1 (`tier1-native`) | 3.63 | 33.49 | 29.86 | 9.24× | false | 0.99446 | 0.99479 |
| `unsupported-3d` | 1 (`tier1-native`) | 6.00 | 54.66 | 18.29 | 9.11× | false | 0.99591 | 0.99594 |
| `canvas-2d` | 1 (`tier1-native`) | 5.88 | 54.62 | 18.31 | 9.28× | false | **0.98499** | 0.98541 |
| `lottie-native` | 1 (`tier1-native`) | 9.19 | **81.34** | 12.29 | 8.85× | false | **0.99923** | 0.99925 |
| `audio-viz` | 1 (`tier1-native`) | 7.92 | 42.58 | 23.48 | 5.38× | true | **0.99940** | 0.99946 |
| `unsupported-webgl` | 2 (`tier2-cdp`) | 7.40 | 9.34 | 107.12 | 1.26× | false | 1.00000 | 1.00000 |
| **Total / Avg** | **17 Tier 1 / 1 Tier 2** | **3.78** | **33.94** | **—** | **8.97×** | **3/3** | **0.98455** | **—** |

- **Total Render Time (`1,080` frames):** **`31,822.5 ms`** (`31.82 s`) vs Remotion **`285,492.6 ms`** (**`8.97×`** overall speedup).
- **Resource Usage (`/usr/bin/time -v`):** Peak RSS `337,660 KB` (**`329.7 MiB`**, `< 4 GiB`), `1,534,485` minor page faults, `564` major page faults, **`Swaps: 0`**, elapsed wall clock `73.22 s`.
- **`allSsimPassed = true`**, `globalMinSsim = 0.98455`. `unsupported-webgl` routed to Tier 2 solely on `canvas-webgl:canvas`.

---

## 3. Upstream Remotion `4.0.545` Template Parity

All 6 bundled upstream templates (`template-blank`, `template-helloworld`, `template-still`, `template-overlay`, `template-audiogram`, `template-music-visualization`) route natively to **Tier 1** and exceed **`SSIM >= 0.96`** against `--force-tier 3 --gl none`, with valid `ffprobe` streams and `--offline` still rendering verified on network-font templates:

| Template (`Composition`) | Spec | Tier | T1 Still (ms) | T3 Still (ms) | SSIM vs T3 | PSNR | Short MP4 (15f) | `ffprobe` / `--offline` |
|---|---|---|---|---|---|---:|---|---|
| `template-blank` (`MyComp`) | `1280x720@30` (60f) | Tier 1 | 1547.9 | 1828.8 | **1.0000** | — | 8.06 fps (1862.1 ms) | `1280,720,15` |
| `template-helloworld` (`HelloWorld`) | `1920x1080@30` (150f) | Tier 1 | 297.6 | 1520.8 | **0.9614** | 17.01 | 20.47 fps (732.9 ms) | `1920,1080,15` |
| `template-still` (`PreviewCard`) | `1200x627@1` (1f) | Tier 1 | 231.1 | 1107.8 | **0.9931** | 29.86 | — | `1200,627` |
| `template-overlay` (`Overlay`) | `1920x1080@30` (75f) | Tier 1 | 265.5 | 996.1 | **0.9980** | 34.41 | 18.31 fps (819.0 ms) | `1920,1080,15`; `--offline` 290.7 ms |
| `template-audiogram` (`Audiogram`) | `1080x1080@30` (814f) | Tier 1 | 198.0 | 1618.0 | **0.9913** | 31.00 | 29.85 fps (502.5 ms) | `1080,1080,15`; `--offline` 697.2 ms |
| `template-music-visualization` (`Visualizer`) | `1080x1080@30` (7165f) | Tier 1 | 577.7 | 3438.5 | **0.9733** | 24.87 | 6.31 fps (2375.7 ms) | `1080,1080,15` |

---

## 4. Quality Gates & Hygiene

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: **38 passed, 0 failed** (1 upstream `deno_core` doc-test ignored):
  - `rsmotion-analyze`: 3/3 (`analyzes_all_18_fixtures_accurately`)
  - `rsmotion-cli`: 2/2
  - `rsmotion-core`: 4/4
  - `rsmotion-css`: 4/4 (`preserves_distinct_font_weights_sharing_variable_font_url`)
  - `rsmotion-encode`: 5/5
  - `rsmotion-js`: 2/2 (`lists_all_18_compositions_in_v8`)
  - `rsmotion-layout`: 5/5 (`replaced_image_transfers_max_height_across_aspect_ratio_in_flex_row`, `replaced_image_with_explicit_width_and_height_ignores_intrinsic_aspect_ratio`)
  - `rsmotion-media`: 4/4 (`decodes_woff_passthrough_offline_guard_and_audio_wav`)
  - `rsmotion-raster`: 8/8 (`renders_canvas2d_and_lottie_skottie_and_measures_canvas_text`, `renders_right_aligned_white_space_pre_text_within_bounds`)
  - `rsmotion-scene`: 1/1
- `tsc && node npm/renderer-shim/test/smoke.mjs`: passed (`ok: true`, `compositionsCount: 18`, `stillSsim: 0.9937430637577008`, `framesSeqCount: 5`, `progressCalls: 60`, `text-basic` 60f at **`40.07 fps`** / `1497.22 ms`).
- `./target/release/rsmotion explain tests/fixtures/build`: **17 Tier 1 (`tier1-native`)**, **1 Tier 2 (`tier2-cdp`: `unsupported-webgl` on `canvas-webgl:canvas`)**.
- `remotion` upstream checkout: untouched (`git status --short` empty).
- Process hygiene (`pgrep -ax 'chrome-headless-shell|ffmpeg' || echo CLEAN`): **`CLEAN`**.
- `git remote -v`: empty (strictly local repository).
