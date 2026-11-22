# Phase 7 Report: Max Backend Speed (`FontCache`/`ParagraphCache`, Packed GPU NV12 Readback & Multi-Worker Chunking)

## 1. Summary

Phase 7 removes the three remaining CPU/GPU bottlenecks in the native Rust pipeline (`rsmotion-raster` and `rsmotion-cli`), nearly doubling average throughput from **19.45 fps** (`2.21×` in Phase 6) to **38.39 fps** (**`4.37×` overall**, up to **`99.09 fps`** / **`11.06×`** on individual Tier 1 fixtures) while keeping Peak RSS at **315.0 MB**, **`Swaps: 0`**, and `globalMinSsim = 0.98455`:

1. **In-Memory `FontCache` & `ParagraphCache` (`crates/rsmotion-raster/src/font_cache.rs`, `scene_render.rs`):**
   - Pre-seeded an in-memory `TypefaceFontProvider` (`fast_provider`) once from `system_font_mgr` using exact host `fontconfig` family alias mappings (e.g. `sans-serif`, `system-ui`, `Roboto`, `Arial`, `Helvetica`, `DejaVu Sans`, plus dynamically registered `@font-face` / `loadFont()` families via `register_font_bytes`).
   - Wired `set_dynamic_font_manager` onto Skia `FontCollection` with negative family caching (`known_families`) so `layout_and_shape` never hits `libfontconfig` per frame or per node.
   - Added a bounded Skia `Paragraph` layout cache (`para_cache`, `MAX_PARA_CACHE_ENTRIES = 512`) and `FontCache::paint_paragraph`, eliminating repeated text shaping across identical/reused text nodes.
2. **GPU-Side Packed `RGBA8888` BT.601 NV12 Readback (`crates/rsmotion-raster/src/gpu_egl.rs`, `scene_render.rs`, `lib.rs`):**
   - Replaced the intermediate `Alpha8 + R8G8UNorm` two-surface NV12 experiment (which suffered `582–1,004 ms` Mesa GLES3 readback due to dual FBOs and 1/2-channel driver conversion) and an unpremul offscreen surface attempt (rejected by Skia Ganesh) with a **single packed `RGBA8888` `AlphaType::Premul` offscreen SkSL target** (`PACKED_NV12_SKSL`, dimensions `(width / 4) × (height + height / 2)`, `BlendMode::Src`).
   - One GPU shader pass + one `glReadPixels(GL_RGBA)` transfers `width * height * 3 / 2` bytes (`3.11 MB` at 1080p vs `8.29 MB` RGBA) directly into FFmpeg VAAPI NV12 layout — verified bit-accurate within `max_diff <= 2` against CPU BT.601 (`gpu_sksl_nv12_matches_cpu_bt601`), even on 1px alternating black/white stripes.
   - Added `SceneRenderer::supports_gpu_nv12()` and `render_scene_nv12(&mut self, scene, scale, need_rgba)` with independent `last_rgba_hash` and `last_nv12_hash` tracking so keyframe stills only pay RGBA readback when requested.
3. **Multi-Worker Chunk Rendering (`crates/rsmotion-cli/src/tier1.rs`, `main.rs`):**
   - Added `pub concurrency: usize` to `Tier1RenderOptions` (wired in `main.rs`), extracted `resolve_worker_count` and `render_parallel_chunks` (chunk 0 runs on the calling thread so the `!Send` V8 isolate `PreparedTier1Host` stays pinned while chunks `1..` boot isolates concurrently inside `std::thread::scope`, joined with `rsmotion_encode::concat_chunks`, `AtomicBool` abort propagation, and merged/sorted audio tracks).
   - Empirical `c=1` vs `c=2` benchmarking on 60f fixtures showed `c=1` (`27.15 fps` / `24.31 s`) outperformed `c=2` (`19.92 fps` / `33.14 s`) on 60f clips on this 2-core Intel Skylake-U Iris 540 GPU due to second V8/EGL/VAAPI context startup and mux contention, so `resolve_worker_count` enables multi-worker chunking automatically at `total_frames >= 120` (4s+ at 30fps, capped at 3 workers, single-worker for GIF/audio-only).

---

## 2. Final 12-Fixture Automatic Benchmark (`--tier auto --raster gpu`)

Measured after final edits with `/usr/bin/time -v ./target/release/rsmotion benchmark --tier auto --raster gpu --concurrency 2 --gl none --min-ssim 0.97 --out-dir /tmp/rsmotion-bench-p7-auto`:

| Fixture | Chosen Tier | Remotion FPS | Auto FPS | ms/Frame | Speedup | Audio | Min SSIM (5 kf) | Avg SSIM |
|---|---|---:|---:|---:|---:|---|---:|---:|
| `text-basic` | 1 (`tier1-native`) | 10.22 | 52.26 | 19.13 | 5.11× | false | 0.99374 | 0.99529 |
| `text-animated` | 1 (`tier1-native`) | 10.05 | 35.05 | 28.53 | 3.49× | false | 0.99624 | 0.99706 |
| `captions` | 1 (`tier1-native`) | 8.84 | **80.49** | 12.42 | **9.11×** | false | 0.98981 | 0.98996 |
| `image-slideshow` | 1 (`tier1-native`) | 8.06 | **89.14** | 11.22 | **11.06×** | false | 0.99735 | 0.99833 |
| `transforms-opacity` | 1 (`tier1-native`) | 8.85 | 64.51 | 15.50 | 7.29× | false | 0.99983 | 0.99991 |
| `gradients-shadows` | 1 (`tier1-native`) | 6.27 | 41.24 | 24.25 | 6.58× | false | 0.98608 | 0.98801 |
| `svg-shapes` | 1 (`tier1-native`) | 9.71 | **74.75** | 13.38 | **7.70×** | false | 0.99967 | 0.99967 |
| `chart-bars` | 1 (`tier1-native`) | 9.86 | **89.76** | 11.14 | **9.11×** | false | 0.99897 | 0.99914 |
| `video-overlay` | 1 (`tier1-native`) | 5.71 | 34.33 | 29.13 | 6.01× | true | 0.99870 | 0.99885 |
| `audio-mix` | 1 (`tier1-native`) | 12.30 | 50.08 | 19.97 | 4.07× | true | 0.99824 | 0.99848 |
| `fonts-custom` | 1 (`tier1-native`) | 12.55 | **99.09** | 10.09 | **7.90×** | false | 0.98455 | 0.98459 |
| `unsupported-3d` | 2 (`tier2-cdp`) | 8.46 | 8.49 | 117.78 | 1.00× | false | 1.00000 | 1.00000 |
| **Total / Avg** | **11 Tier 1 / 1 Tier 2** | **8.79** | **38.39** | **—** | **4.37×** | **2/2** | **0.98455** | **—** |

- **Total Render Time (`720` frames):** **`18,753.0 ms`** (`18.75 s`) vs Remotion **`81,952.5 ms`** (**`4.37×`**, compared to `37,012.1 ms` / `19.45 fps` / `2.21×` in Phase 6).
- **Resource Usage (`/usr/bin/time -v`):** Peak RSS `322,564 KB` (**`315.0 MB`**, including Chrome Headless Shell for `unsupported-3d`), `809,249` minor page faults, **`Swaps: 0`**, elapsed wall clock `34.53 s`.
- **`allSsimPassed = true`**, `globalMinSsim = 0.98455`, Encoder `h264_vaapi`. `unsupported-3d` routed to Tier 2 (`backdrop-filter:div, canvas:canvas, mix-blend-mode:div`).

---

## 3. 11-Fixture Tier-1-GPU Comparison (`--concurrency 1` vs `--concurrency 2`)

Both runs used packed `RGBA8888` NV12 readback prior to setting the `>= 120` frame worker gate (showing directly why 60-frame clips stay single-worker):

### 3.1 Single-Worker (`--concurrency 1`, 660 frames: `24,307.6 ms`, `27.15 fps`, `3.08×`)

| Fixture | Base FPS | Tier1 FPS | Speedup | Min SSIM | `js` (ms) | `lay` (ms) | `vid` (ms) | `draw` (ms) | `rb` (ms) | `aud` (ms) | Audio |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `text-basic` | 10.22 | 28.61 | 2.80× | 0.99374 | 295.3 | 64.7 | 0.0 | 129.4 | 404.4 | 0.6 | false |
| `text-animated` | 10.05 | 39.80 | 3.96× | 0.99624 | 305.2 | 30.4 | 0.0 | 111.9 | 251.8 | 0.0 | false |
| `captions` | 8.84 | 62.75 | 7.10× | 0.98981 | 213.5 | 29.5 | 0.0 | 44.7 | 166.6 | 0.0 | false |
| `image-slideshow` | 8.06 | 39.71 | 4.93× | 0.99735 | 385.5 | 10.6 | 0.0 | 118.5 | 110.5 | 0.0 | false |
| `transforms-opacity` | 8.85 | 42.25 | 4.78× | 0.99983 | 215.6 | 9.4 | 3.1 | 71.4 | 243.4 | 0.0 | false |
| `gradients-shadows` | 6.27 | 25.78 | 4.11× | 0.98608 | 600.4 | 34.3 | 0.1 | 211.5 | 534.8 | 0.0 | false |
| `svg-shapes` | 9.71 | 25.80 | 2.66× | 0.99967 | 500.6 | 19.0 | 0.0 | 226.8 | 400.6 | 0.0 | false |
| `chart-bars` | 9.86 | 20.76 | 2.11× | 0.99897 | 840.6 | 23.2 | 0.0 | 287.3 | 514.4 | 0.0 | false |
| `video-overlay` | 5.71 | 10.44 | 1.83× | 0.99870 | 895.6 | 28.4 | 536.5 | 305.5 | 313.0 | 1240.0 | true |
| `audio-mix` | 12.30 | 30.80 | 2.50× | 0.99824 | 310.3 | 27.3 | 0.0 | 50.2 | 212.1 | 767.6 | true |
| `fonts-custom` | 12.55 | 38.09 | 3.04× | 0.98455 | 355.6 | 39.0 | 0.0 | 80.8 | 386.8 | 0.0 | false |

- `/usr/bin/time -v`: Peak RSS **`236,692 KB`** (**`231.1 MB`**), `519,345` minor page faults, **`Swaps: 0`**, elapsed wall time `39.46 s`.

### 3.2 Two-Worker 60f (`--concurrency 2` pre-threshold, 660 frames: `33,135.2 ms`, `19.92 fps`, `2.26×`)

| Fixture | Base FPS | Tier1 FPS | Speedup | Min SSIM | `js` (ms) | `lay` (ms) | `vid` (ms) | `draw` (ms) | `rb` (ms) | `aud` (ms) | Audio |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `text-basic` | 10.22 | 13.97 | 1.37× | 0.99374 | 798.5 | 72.0 | 0.0 | 624.9 | 1605.0 | 0.4 | false |
| `text-animated` | 10.05 | 18.55 | 1.85× | 0.99624 | 767.4 | 77.6 | 0.0 | 814.2 | 1516.0 | 0.0 | false |
| `captions` | 8.84 | 16.29 | 1.84× | 0.98981 | 1132.2 | 214.5 | 0.0 | 338.6 | 729.4 | 0.0 | false |
| `image-slideshow` | 8.06 | 22.96 | 2.85× | 0.99735 | 1017.8 | 22.8 | 0.0 | 324.7 | 480.3 | 0.0 | false |
| `transforms-opacity` | 8.85 | 18.20 | 2.06× | 0.99983 | 887.9 | 27.7 | 0.0 | 165.8 | 646.9 | 0.0 | false |
| `gradients-shadows` | 6.27 | 18.47 | 2.95× | 0.98608 | 926.7 | 53.4 | 0.0 | 624.4 | 743.5 | 0.0 | false |
| `svg-shapes` | 9.71 | 28.64 | 2.95× | 0.99967 | 372.6 | 12.3 | 0.0 | 192.9 | 280.0 | 0.0 | false |
| `chart-bars` | 9.86 | 28.81 | 2.92× | 0.99897 | 566.9 | 8.7 | 0.0 | 177.9 | 363.6 | 0.0 | false |
| `video-overlay` | 5.71 | 15.19 | 2.66× | 0.99870 | 1043.7 | 60.4 | 748.0 | 313.0 | 357.0 | 847.1 | true |
| `audio-mix` | 12.30 | 19.10 | 1.55× | 0.99824 | 868.5 | 60.5 | 0.0 | 124.7 | 388.8 | 605.9 | true |
| `fonts-custom` | 12.55 | 40.11 | 3.20× | 0.98455 | 294.2 | 37.2 | 0.0 | 82.2 | 253.4 | 0.0 | false |

- `/usr/bin/time -v`: Peak RSS **`278,480 KB`** (**`272.0 MB`**), `880,444` minor page faults, **`Swaps: 0`**, elapsed wall time `47.09 s`.
- While JS/decode-bound clips (`svg-shapes`, `chart-bars`, `video-overlay`, `fonts-custom`) gained slightly with 2 workers, GPU/readback-bound 60f clips (`text-basic`, `captions`, `text-animated`) suffered 2–3× slower readback (`729–1,605 ms`) from concurrent EGL/VAAPI contention. Setting the threshold to `total_frames >= 120` preserved single-worker speed on 60f clips while keeping multi-worker chunking tested and ready for 4s+ renders.

---

## 4. Standalone Spot Checks (`rsmotion render`) & Hygiene

Measured after final build on `Mesa Intel(R) Iris(R) Graphics 540 (SKL GT3)` + `h264_vaapi`:

| Fixture | Wall (ms) | FPS | `js` (ms) | `lay` (ms) | `vid` (ms) | `draw` (ms) | `rb` (ms) | `enc` (ms) | `aud` (ms) | Audio |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `text-basic` | 781.2 | 76.80 | 162.4 | 39.2 | 0.0 | 45.6 | 187.4 | 498.9 | 0.0 | false |
| `captions` | 756.5 | 79.31 | 166.6 | 29.2 | 0.0 | 49.8 | 172.0 | 488.1 | 0.0 | false |
| `image-slideshow` | 740.0 | 81.08 | 235.5 | 8.4 | 0.0 | 66.0 | 68.0 | 465.4 | 0.0 | false |
| `svg-shapes` | 739.1 | 81.18 | 171.4 | 7.6 | 0.0 | 87.0 | 155.4 | 497.0 | 0.0 | false |
| `chart-bars` | 792.7 | 75.69 | 194.0 | 5.1 | 0.0 | 87.6 | 148.2 | 512.5 | 0.0 | false |
| `video-overlay` | 1941.1 | 30.91 | 353.3 | 20.2 | 230.4 | 177.0 | 217.8 | 1173.8 | 411.6 | true |
| `audio-mix` | 1365.6 | 43.94 | 209.4 | 27.3 | 0.0 | 46.7 | 152.8 | 507.3 | 581.3 | true |

- Process hygiene (`pgrep -ax 'chrome-headless-shell|ffmpeg' || echo CLEAN`): **`CLEAN`**.

---

## 5. Quality Gates

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: **27 passed, 0 failed** (1 upstream `deno_core` doc-test ignored):
  - `rsmotion-analyze`: 3/3 (`static_scan_finds_bundle_hints_without_external_css`, `analyzes_all_12_fixtures_accurately`, `remote_url_routes_to_tier2_and_video_overlay_reuses_host`)
  - `rsmotion-cli`: 2/2 (`resolve_worker_count_gates_short_clips_and_caps_workers`, `multi_worker_chunking_renders_and_concats_chunks`)
  - `rsmotion-core`: 4/4
  - `rsmotion-encode`: 5/5
  - `rsmotion-js`: 2/2
  - `rsmotion-layout`: 2/2
  - `rsmotion-media`: 3/3
  - `rsmotion-raster`: 5/5 (including `gpu_sksl_nv12_matches_cpu_bt601`, `max_diff <= 2`, and `renders_scene_and_deduplicates_identical_frames`)
  - `rsmotion-scene`: 1/1
- `node npm/renderer-shim/test/smoke.mjs`: passed (`ok: true`, `compositionsCount: 12`, `stillSsim: 0.99374`, `framesSeqCount: 5`, `progressCalls: 60`, `text-basic` 60f at **68.50 fps** / `875.9 ms`).
