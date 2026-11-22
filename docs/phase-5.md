# Phase 5 Report: Analyzer, Automatic Tier 1/2/3 Dispatch & `rsmotion explain`

## 1. Summary

Phase 5 adds `crates/rsmotion-analyze`, `rsmotion explain`, `--explain`, the Tier 3 `@remotion/renderer` escape hatch (`--force-tier 3`), and automatic default tier dispatch (`Tier 1 -> Tier 2` with runtime fallback) across `rsmotion compositions`, `still`, `render`, `benchmark --tier auto`, and the `@rsmotion/renderer` npm shim.

Across all **12 fixtures** (`720` frames, `60` golden keyframes) via `rsmotion benchmark --tier auto --raster gpu`:
- **11/12 fixtures** (`text-basic`, `text-animated`, `captions`, `image-slideshow`, `transforms-opacity`, `gradients-shadows`, `svg-shapes`, `chart-bars`, `video-overlay`, `audio-mix`, `fonts-custom`) route automatically to **Tier 1 (`tier1-native`)** without spawning a browser.
- **`unsupported-3d`** automatically detects `backdrop-filter:div`, `canvas:canvas`, and `mix-blend-mode:div` on sampled frames and routes cleanly to **Tier 2 (`tier2-cdp`)** with **`minSsim = 1.00000`**.
- **All 12 fixtures pass SSIM verification (`allSsimPassed = true`, `globalMinSsim = 0.98455`)** at **`15.23 fps` overall (`1.73×` vs Remotion `4.0.545` baseline at `8.79 fps`)** and **`254.2 MB` Peak RSS** (`Swaps: 0`, zero leftover child processes).

## 2. Architecture

- **`crates/rsmotion-js/src/host.js` (Extended Unsupported Detector):**
  - Detects HTML CSS `backdrop-filter`, `mix-blend-mode`, `filter`, `clip-path`, `mask-image`, `background-clip: text`, `display: grid`/`inline-grid`, 3D CSS transforms (`perspective`/`rotateX`/`rotateY`/`rotate3d`/`translate3d`/`translateZ`/`scale3d`/`matrix3d`), SVG `<foreignObject>`, `<canvas>`, `<iframe>`, unresolvable remote `<video>`, external `<link rel="stylesheet">`, and user `<style>` tags (ignoring Remotion's internal `makeDefaultPreviewCSS`, `studioCssVariables`, and bundler UI `<style>` injections).
- **`crates/rsmotion-analyze` (`static_scan.rs`, `probe.rs`, `error.rs`):**
  - `scan_bundle_static`: inspects `serveUrl` (`http(s)` vs local dir), `index.html` stylesheets/scripts, and bundle-level hints (`WebGLRenderer`/`@remotion/three`, `lottie-web`/`@remotion/lottie`, `backdrop-filter`, `mix-blend-mode`, `.getContext("2d"/"webgl")`, `foreignObject`).
  - `analyze_composition_for_dispatch`: probes representative `[start, mid, end]` frames (or `[frame]` for `still`) inside embedded V8, produces `CompositionAnalysis` (`chosenTier`, `tierLabel`, `expectedFpsRange`, `sampledFrames`, `unsupportedFeatures`, `hasVideo`, `hasAudio`, `hasSvg`, `reasons`, `probeMs`), and retains `PreparedTier1Host` on its creating thread when Tier 1 is chosen so `rsmotion render`/`still` avoids booting a second V8 isolate.
  - `analyze_bundle` & `format_composition_explanation`: powers `rsmotion explain` and `--explain`.
- **`crates/rsmotion-cli` (`tier1.rs`, `tier3.rs`, `main.rs`):**
  - `try_analyze_and_render_still_tier1` and `try_analyze_and_render_media_tier1` keep `!Send` V8 isolates pinned on one blocking worker thread across analysis + Tier 1 rendering.
  - Added `abort_on_unsupported: true` (`LateUnsupportedFeatureError`) in auto mode so any unsupported feature appearing mid-composition aborts cleanly, deletes temporary video artifacts, and automatically re-renders in Tier 2 Chrome CDP.
  - Added `tier3.rs` calling stock `@remotion/renderer` via `node -e` for `--force-tier 3`.

## 3. Verification Results

### A. `rsmotion explain tests/fixtures/build`
- Total bundle scan + all 12 composition probes: `3,076.7 ms`.
- Static bundle hints: `backdrop-filter@bundle.js`, `mix-blend-mode@bundle.js`, `canvas-context@bundle.js`, `foreignObject@bundle.js`.
- Classifications:
  - 11 compositions → `tier 1 (tier1-native)`, `unsupportedFeatures: []` (`video-overlay` detects `native FFmpeg frame decoder enabled` + `native multi-track audio mixer enabled`; `svg-shapes` & `chart-bars` detect `native Skia SVG DOM rasterizer enabled`).
  - `unsupported-3d` → `tier 2 (tier2-cdp)`, `unsupportedFeatures: ["backdrop-filter:div", "canvas:canvas", "mix-blend-mode:div"]`.

### B. `render --explain`, `--force-tier 3`, and `@rsmotion/renderer` Shim
- `rsmotion render tests/fixtures/build text-basic /tmp/p5-auto-tb.mp4 --explain --overwrite`:
  - Probed in `285.7 ms`, routed to `tier 1`, rendered 60 frames in `2,231.1 ms` (**`26.89 fps`**), `h264_vaapi`.
- `rsmotion render tests/fixtures/build unsupported-3d /tmp/p5-auto-u3d.mp4 --explain --overwrite`:
  - Probed in `437.0 ms`, routed to `tier 2`, rendered 60 frames via Chrome CDP (`shm-bgra`, `h264_vaapi`) in `8,866.3 ms` (`6.77 fps`).
- `rsmotion still tests/fixtures/build unsupported-3d /tmp/p5-auto-u3d-30.png --frame 30 --explain --overwrite`:
  - Routed to `tier 2` (`probe=235.2 ms`, `wall_ms=684.9 ms`), golden `rsmotion diff` **`SSIM = 1.00000`** (`max_channel_diff = 0`).
- `rsmotion still tests/fixtures/build text-basic /tmp/p5-t3-still.png --frame 30 --force-tier 3 --overwrite --json`:
  - Executed stock `@remotion/renderer` (`tier: 3`, `wall_ms=680.8 ms`), golden `rsmotion diff` **`SSIM = 1.00000`**.
- `@rsmotion/renderer` shim (`node test/smoke.mjs` with zero flags changed):
  - `ok: true`, `compositionsCount: 12`, `stillSsim: 0.99374`, `progressCalls: 60`, `durationInMilliseconds: 2545`, automatically executed **`rsmotionReport.tier = 1`** at **`26.29 fps`**.

### C. Full 12-Fixture Automatic Benchmark (`--tier auto`, `720` Frames, `/tmp/rsmotion-bench-p5-auto`)
Measured via `/usr/bin/time -v ./target/release/rsmotion benchmark --tier auto --raster gpu --concurrency 2 --gl none --min-ssim 0.97 --out-dir /tmp/rsmotion-bench-p5-auto`:

| Fixture | Chosen Tier | Baseline FPS | Auto FPS | Speedup | Audio | Encoder | Min SSIM (5 kf) | Avg SSIM |
|---|---|---:|---:|---:|---|---|---:|---:|
| `text-basic` | 1 (`tier1-native`) | 10.22 | 14.51 | 1.42× | false | `h264_vaapi` | 0.99374 | 0.99529 |
| `text-animated` | 1 (`tier1-native`) | 10.05 | 20.02 | 1.99× | false | `h264_vaapi` | 0.99624 | 0.99706 |
| `captions` | 1 (`tier1-native`) | 8.84 | 13.16 | 1.49× | false | `h264_vaapi` | 0.98981 | 0.98996 |
| `image-slideshow` | 1 (`tier1-native`) | 8.06 | 18.61 | 2.31× | false | `h264_vaapi` | 0.99735 | 0.99833 |
| `transforms-opacity` | 1 (`tier1-native`) | 8.85 | 31.98 | 3.62× | false | `h264_vaapi` | 0.99983 | 0.99991 |
| `gradients-shadows` | 1 (`tier1-native`) | 6.27 | 21.76 | 3.47× | false | `h264_vaapi` | 0.98608 | 0.98801 |
| `svg-shapes` | 1 (`tier1-native`) | 9.71 | 23.96 | 2.47× | false | `h264_vaapi` | 0.99967 | 0.99967 |
| `chart-bars` | 1 (`tier1-native`) | 9.86 | 20.29 | 2.06× | false | `h264_vaapi` | 0.99897 | 0.99914 |
| `video-overlay` | 1 (`tier1-native`) | 5.71 | 16.54 | 2.90× | true | `h264_vaapi` | 0.99870 | 0.99885 |
| `audio-mix` | 1 (`tier1-native`) | 12.30 | 13.21 | 1.07× | true | `h264_vaapi` | 0.99824 | 0.99848 |
| `fonts-custom` | 1 (`tier1-native`) | 12.55 | 31.89 | 2.54× | false | `h264_vaapi` | 0.98455 | 0.98459 |
| `unsupported-3d` | 2 (`tier2-cdp`) | 8.46 | 4.92 | 0.58× | false | `h264_vaapi` | 1.00000 | 1.00000 |
| **Total / Avg** | **11 Tier 1 / 1 Tier 2** | **8.79** | **15.23** | **1.73×** | **2/2** | **`h264_vaapi`** | **0.98455** | **—** |

- **Peak RSS (`/usr/bin/time -v`):** `260,300 KB` (`254.2 MB`), `Swaps: 0`, total elapsed wall time `70.77 s` (includes analyzer probes, Tier 2 browser launch for `unsupported-3d`, `ffprobe` checks, 60 keyframe PNG writes, and 60 SSIM comparisons).

## 4. Quality & Hygiene Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: passed (`23 passed; 0 failed` across `rsmotion-analyze` (3), `rsmotion-core` (4), `rsmotion-encode` (4), `rsmotion-js` (2), `rsmotion-layout` (2), `rsmotion-media` (3), `rsmotion-raster` (4), `rsmotion-scene` (1))
- Process hygiene (`pgrep -af 'chrome-headless-shell|ffmpeg'`): 0 leftover child processes

## 5. Next Steps

Proceed to **Phase 6 (Optimization, API Parity & Hardening)**:
1. Memoize `FontCache::measure_text_box` and cache SVG XML/DOM (`chart-bars` & text layout speedup).
2. Add direct RGBA → NV12 conversion + VAAPI pipe optimization to remove FFmpeg's `format=nv12,hwupload` CPU bottleneck and unlock higher FPS.
3. Complete `@rsmotion/renderer` shim parity (`renderFrames`, `forceTier`, `raster`, `explain`, JPEG/WebP still tests), write `docs/compat.md`, update `docs/api-parity.md`, and complete `docs/phase-6.md` (`phase-6` tag + final walkthrough).
