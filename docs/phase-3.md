# Phase 3 Report: Tier 1 Native Browserless Renderer (V8 + Taffy + GPU Skia)

## 1. Summary

Phase 3 built the complete Chrome-free Tier 1 rendering pipeline across `rsmotion-scene`, `rsmotion-js`, `rsmotion-layout`, `rsmotion-raster`, and `rsmotion-cli`. Instead of launching Headless Chrome, `rsmotion` now executes unmodified Remotion `4.0.545` Webpack bundles directly inside embedded V8 (`deno_core`), runs CSS cascade and Flexbox/Box layout via `taffy 0.15.1`, and rasterizes DOM, typography, gradients, shadows, decoded images, and SVG trees natively on the Intel Iris 540 GPU via headless EGL/GLES3 Skia (`--raster gpu`, with `--raster cpu` fallback).

Across all **9 non-video / non-3D fixtures** (`540` total frames, `45` golden keyframes), Tier 1 achieves **`20.17 fps` (`2.22×` faster than Remotion `4.0.545` baseline at `9.08 fps`; up to `3.65×` / `30.03 fps`)** during full keyframe-verified benchmark runs — and up to **`30.56 fps`** in pure video rendering — with **`globalMinSsim = 0.98455`** and **`254.7 MB` Peak RSS** (zero Chrome processes).

## 2. Tier 1 Architecture

- **`crates/rsmotion-scene` (`SceneTree`, `SceneNode`, `NodeKind`, `Tier1Composition`):**
  Flat arena-indexed DOM/style/SVG/font/audio/unsupported-feature snapshot schema decoupling JS execution from layout and rasterization.
- **`crates/rsmotion-js` (`V8BundleHost`, `host.js`, `ops.rs`):**
  Embeds V8 `150.4.0` via `deno_core 0.413.0` (prebuilt `librusty_v8`, zero C++ compilation) and executes `bundle_dir/index.html` inline scripts + `bundle.js` against a Virtual DOM host:
  - Full Remotion bootstrap globals (`setPropsAndEnv`), `URL`/`URLSearchParams`, `AbortController`, `BroadcastChannel`, `localStorage`/`sessionStorage`, `history`, `document.currentScript`, `getComputedStyle`.
  - Deterministic timer/rAF/MessageChannel scheduler (`setTimeout <= 50ms` & `rAF` drained per tick; `delayRender`'s 28s safety timers never fired prematurely).
  - Synchronous `op_probe_image` backing `HTMLImageElement.src` / `decode()` and `op_register_font` backing `FontFace.load()` / `document.fonts`.
  - Case-preserving `<svg>` XML serializer inheriting HTML `font-family` and `color` onto the `<svg>` root.
  - Runtime unsupported-feature collector flagging `canvas`, `video`, `iframe`, `foreignObject`, `backdrop-filter`, `mix-blend-mode`, and non-`none` CSS `filter`.
- **`crates/rsmotion-layout` (`style.rs`, `engine.rs`):**
  - CSS cascade + inheritance + UA defaults + Remotion `default-css.ts` (`box-sizing: border-box`, `.__remotion_objectfitcontain`).
  - Full parsing for colors (hex/rgb/rgba/hsl/hsla/named/`transparent`/`currentColor`), lengths (`px`/`%`/`em`/`rem`/`vw`/`vh`), `linear-gradient`, `radial-gradient`, borders, rounded radii, `box-shadow`, `text-shadow`, 2D `transform` + `transform-origin`, typography, and `object-fit` / `object-position`.
  - Merges adjacent React JSX `Text` nodes (`Frame {frame}`), resolves Flexbox/Box geometry via `taffy::TaffyTree::compute_layout_with_measure` with Skia measurement callbacks, and stably sorts children by `z-index`.
- **`crates/rsmotion-raster` (`font_cache.rs`, `image_cache.rs`, `scene_render.rs`):**
  - `FontCache`: registers dynamic `FontFace` TTFs via `TypefaceFontProvider`, configures `FontHinting::None` + subpixel positioning + CSS letter-spacing spec, builds & measures Skia `Paragraph`s.
  - `ImageCache`: decodes referenced raster images once per render and supports `object-fit: fill/contain/cover/none/scale-down`.
  - `SceneRenderer`: renders onto `HeadlessGpuSurface` (`Mesa Intel(R) Iris(R) Graphics 540 (SKL GT3)`) by default with CPU Skia fallback, hashes computed `LayoutTree` frames to skip redundant draw + GPU readback, paints `box-shadow` with `ClipOp::Difference` and `sigma = 0.5 * blur_radius`, paints CSS linear/radial gradients using Skia `gradient_shaders`, and renders `<svg>` natively on GPU Skia via `skia_safe::svg::Dom`.
- **`crates/rsmotion-cli` (`src/tier1.rs`, `src/main.rs`):**
  Added `--force-tier 1` on `compositions`, `still`, and `render`, `--raster <auto|gpu|cpu>` on `still`, `render`, and `benchmark`, and `--tier <1|2> --fixtures <list>` on `benchmark`.

## 3. Benchmark & Visual Fidelity Results (9 Tier-1 Fixtures, 540 Frames)

Measured via:
- GPU: `/usr/bin/time -v ./target/release/rsmotion benchmark --tier 1 --raster gpu --fixtures text-basic,text-animated,captions,image-slideshow,transforms-opacity,gradients-shadows,svg-shapes,chart-bars,fonts-custom --out-dir /tmp/rsmotion-tier1-gpu --min-ssim 0.95`
- CPU: `/usr/bin/time -v ./target/release/rsmotion benchmark --tier 1 --raster cpu --fixtures ... --out-dir /tmp/rsmotion-tier1-cpu --min-ssim 0.95`

| Fixture | Baseline FPS | Tier 2 FPS | Tier 1 GPU FPS | GPU Speedup vs Base | Tier 1 CPU FPS | CPU Speedup | Dedup | GPU Draw (60f) | CPU Draw (60f) | Min SSIM (5 kf) | Avg SSIM |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `text-basic` | 10.22 | 9.39 | 13.56 | 1.33× | 16.49 | 1.61× | 0 | 167.9 ms | 954.1 ms | 0.99374 | 0.99529 |
| `text-animated` | 10.05 | 11.27 | 20.71 | 2.06× | 18.03 | 1.79× | 0 | 232.5 ms | 1,624.4 ms | 0.99624 | 0.99706 |
| `captions` | 8.84 | 11.28 | 16.90 | 1.91× | 20.40 | 2.31× | 0 | 106.9 ms | 543.9 ms | 0.98981 | 0.98996 |
| `image-slideshow` | 8.06 | 10.44 | 29.39 | 3.65× | 20.68 | 2.57× | 45/60 | 133.0 ms | 1,438.6 ms | 0.99735 | 0.99833 |
| `transforms-opacity` | 8.85 | 10.19 | 30.03 | 3.40× | 23.60 | 2.67× | 0 | 110.9 ms | 684.7 ms | 0.99983 | 0.99991 |
| `gradients-shadows` | 6.27 | 8.28 | 20.47 | 3.27× | 11.29 | 1.80× | 0 | 261.2 ms | 3,895.2 ms | 0.98608 | 0.98801 |
| `svg-shapes` | 9.71 | 11.42 | 26.51 | 2.73× | 32.88 | 3.39× | 0 | 194.6 ms | 427.5 ms | 0.99967 | 0.99967 |
| `chart-bars` | 9.86 | 12.61 | 13.93 | 1.41× | 13.93 | 1.41× | 0 | 2,940.4 ms | 3,268.0 ms | 0.99897 | 0.99914 |
| `fonts-custom` | 12.55 | 10.89 | 25.42 | 2.03× | 24.05 | 1.92× | 0 | 62.9 ms | 369.7 ms | 0.98455 | 0.98459 |
| **Total / Avg** | **9.08** | **10.50** | **20.17** | **2.22×** | **18.43** | **2.03×** | **45** | **4,210.4 ms** | **13,206.1 ms** | **0.98455** | **0.99466** |

- **GPU vs CPU Rasterization:** Across all 540 frames, total draw time dropped from **13,206.1 ms (CPU)** to **4,210.3 ms (GPU)** (**3.14×** faster draw overall; **14.9×** faster on `gradients-shadows` (`261.2 ms` vs `3,895.2 ms`), **10.8×** on `image-slideshow`, **7.0×** on `text-animated`).
- **Peak RSS & System Health:**
  - Tier 1 GPU: **`260,848 KB` (`254.7 MB`)**, 0 swaps, wall elapsed `37.28 s` (includes `ffprobe` checks & 45 SSIM diffs).
  - Tier 1 CPU: **`252,652 KB` (`246.7 MB`)**, 0 swaps, wall elapsed `39.22 s`, zero `chrome-headless-shell` processes spawned.
  - All 9/9 output MP4s verified `1920,1080,60` via `ffprobe`; encoder: `h264_vaapi`.

### Pure `render --force-tier 1` Spot Check (No Mid-Loop PNG Writes)

Because `benchmark` synchronously encodes 5 1080p keyframe PNGs per fixture inside the frame loop, standalone `rsmotion render --force-tier 1` is noticeably faster on lightweight scenes:
- `text-basic`: `2,842.6 ms` -> **`21.11 fps`** (`js 381.8 ms`, `layout 799.8 ms`, `draw 93.8 ms`, `readback 398.7 ms`, `encode 2,174.6 ms`)
- `transforms-opacity`: `1,963.2 ms` -> **`30.56 fps`** (`js 318.8 ms`, `layout 12.2 ms`, `draw 61.2 ms`, `readback 409.6 ms`, `encode 1,490.6 ms`)
- `gradients-shadows`: `2,572.1 ms` -> **`23.33 fps`** (`draw 160.4 ms`)
- `svg-shapes`: `2,529.3 ms` -> **`23.72 fps`** (`draw 152.1 ms`)
- `chart-bars`: `4,729.8 ms` -> **`12.69 fps`** (`draw 3,419.8 ms`)

### Identified Optimization Headroom for Phase 6
1. **Layout `measure_text` cache:** `text-basic` (`800–1,065 ms` for 60f) and `captions` (`964 ms`) currently re-shape static/repeated text via `ParagraphBuilder` multiple times per frame during Taffy's passes; memoizing `(text, style_hash, max_width)` should cut text `layout_ms` by >5×.
2. **SVG / Static DOM caching & `chart-bars` draw:** Re-parsing `<svg>` XML per frame + Skia SVG text re-layout took `~49–57 ms/frame` on `chart-bars`. Caching `FontMgr`/glyphs or unchanged subtrees and switching RGBA pipes to direct NV12 for VAAPI (`Phase 6`) will push Tier 1 well past the current ~`21–31 fps` encode/layout ceiling.

## 4. Fallback Feature Detection Verification (`unsupported-3d`)

Running `rsmotion still tests/fixtures/build unsupported-3d /tmp/t1-still-check/actual/unsupported-3d_30.png --frame 30 --force-tier 1 --overwrite` succeeded cleanly without crashing in V8 and accurately reported:
- `unsupported_features: ["backdrop-filter:div", "canvas:canvas", "mix-blend-mode:div"]`

Remaining gaps deliberately scoped to later phases:
- **Phase 4:** Native Tier 1 `<OffthreadVideo>` / `<Video>` frame decoding (`video-overlay`) and `<Audio>` muxing (`audio-mix`).
- **Phase 5:** Static + dynamic analyzer and automatic Tier 1 -> Tier 2 -> Tier 3 dispatch (`rsmotion explain`).

## 5. Quality Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: passed (`17 passed; 0 failed` across `rsmotion-core` (4), `rsmotion-encode` (4), `rsmotion-js` (2), `rsmotion-layout` (2), `rsmotion-raster` (4), `rsmotion-scene` (1))
