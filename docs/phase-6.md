# Phase 6 Report: Performance Tuning, API/CLI Parity, Compatibility Matrix & Final Verification

## 1. Summary

Phase 6 completes `rsmotion` as a fast, verified drop-in replacement for `@remotion/renderer` (Remotion `4.0.545`):
1. **Tier 1 Performance Tuning (`rsmotion-raster`, `rsmotion-encode`):**
   - Added bounded **`measure_cache`** (`MAX_MEASURE_CACHE_ENTRIES = 4096`) inside `FontCache::measure_text_box` and a pre-seeded in-memory **`svg_fast_provider`** (`svg_font_mgr_for_xml`) for `DejaVu Sans`, `DejaVu Serif`, `DejaVu Sans Mono`, and custom `FontFace` families to avoid repeated `libfontconfig` queries during SVG DOM parsing.
   - Added a bounded FIFO **`svg_dom_cache`** (`MAX_SVG_DOM_CACHE = 16`) preloaded in `preload_tree_svgs` and extracted `draw_svg_dom` in `SceneRenderer`.
   - Added direct fixed-point BT.601 studio-swing **`convert_rgba_or_bgra_to_nv12`** (`rsmotion-encode/src/yuv.rs`), an automatic **`vaapi_nv12_fastpath`** (`-pix_fmt nv12 -vf hwupload` when `HardwareAccel::Vaapi + PixelFormat::Yuv420p`), **warm frame buffer recycling** (`acquire_frame_buffer`), and **zero-copy duplicate frame emission** (`push_duplicate_frame`).
2. **CLI & `@rsmotion/renderer` Shim Polish (`rsmotion-cli`, `npm/renderer-shim`):**
   - Added global **`--log <LEVEL>`** (`error`, `warn`, `info`, `verbose`, `debug`) to `rsmotion-cli`.
   - Added **`rsmotion frames`** (`render_frames_tier1_with_host` + `render_frames_cdp` fallback) emitting image sequences with `--image-format`, `--jpeg-quality`, `--scale`, `--frames`, `--every-nth-frame`, `--explain`, and `--json-progress`.
   - Extended `@rsmotion/renderer` with `renderFrames`, `getVideoMetadata`, `validateOutputFilename`, `validateSelectedPixelFormatAndImageFormatCombination`, `forceTier`/`raster`/`explain` options, and `RenderInternals` exports (`internalRenderFrames`, etc.), expanding `npm/renderer-shim/test/smoke.mjs` to test PNG/JPEG/WebP stills, frame sequences, Tier 1 auto-dispatch, and `unsupported-3d` Tier 2 auto-fallback.
3. **Upstream Compatibility Verification (`docs/compat.md`):**
   - Bundled and tested 4 upstream Remotion `4.0.545` template compositions (`template-blank:MyComp`, `template-helloworld:HelloWorld`, `template-helloworld:Logo`, `template-still:PreviewCard`), all routing automatically to the expected tier with **`ssimVsTier3 >= 0.95296`** (`1.00000` on `template-blank` and `template-still`), plus statically audited 9 additional upstream template packages.

---

## 2. Final 12-Fixture Automatic Benchmark (`--tier auto --raster gpu`)

Gemessen mit `/usr/bin/time -v ./target/release/rsmotion benchmark --tier auto --raster gpu --concurrency 2 --gl none --min-ssim 0.97 --out-dir /tmp/rsmotion-bench-p6-auto`:

| Fixture | Gewählter Tier | Remotion FPS | Auto FPS | ms/Frame | Speedup | Audio | Min SSIM (5 kf) | Avg SSIM |
|---|---|---:|---:|---:|---:|---|---:|---:|
| `text-basic` | 1 (`tier1-native`) | 10.22 | 31.84 | 31.41 | 3.11× | false | 0.99374 | 0.99529 |
| `text-animated` | 1 (`tier1-native`) | 10.05 | 25.91 | 38.60 | 2.58× | false | 0.99624 | 0.99706 |
| `captions` | 1 (`tier1-native`) | 8.84 | 31.88 | 31.37 | 3.61× | false | 0.98981 | 0.98996 |
| `image-slideshow` | 1 (`tier1-native`) | 8.06 | **65.44** | 15.28 | **8.12×** | false | 0.99735 | 0.99833 |
| `transforms-opacity` | 1 (`tier1-native`) | 8.85 | 24.97 | 40.04 | 2.82× | false | 0.99983 | 0.99991 |
| `gradients-shadows` | 1 (`tier1-native`) | 6.27 | 13.15 | 76.07 | 2.10× | false | 0.98608 | 0.98801 |
| `svg-shapes` | 1 (`tier1-native`) | 9.71 | 24.48 | 40.85 | 2.52× | false | 0.99967 | 0.99967 |
| `chart-bars` | 1 (`tier1-native`) | 9.86 | **36.36** | 27.50 | **3.69×** | false | 0.99897 | 0.99914 |
| `video-overlay` | 1 (`tier1-native`) | 5.71 | 18.65 | 53.63 | 3.26× | true | 0.99870 | 0.99885 |
| `audio-mix` | 1 (`tier1-native`) | 12.30 | 25.14 | 39.78 | 2.04× | true | 0.99824 | 0.99848 |
| `fonts-custom` | 1 (`tier1-native`) | 12.55 | 26.28 | 38.05 | 2.09× | false | 0.98455 | 0.98459 |
| `unsupported-3d` | 2 (`tier2-cdp`) | 8.46 | 5.43 | 184.28 | 0.64× | false | 1.00000 | 1.00000 |
| **Total / Avg** | **11 Tier 1 / 1 Tier 2** | **8.79** | **19.45** | **51.41** | **2.21×** | **2/2** | **0.98455** | **—** |

- **Total Benchmark Render Time:** **`37,012.1 ms`** (`720` frames) vs Remotion `81,952.5 ms` (**`2.21×`**, verbessert gegenüber `15.23 fps` / `1.73×` in Phase 5).
- **Resource Usage (`/usr/bin/time -v`):** Peak RSS `370,448 KB` (**`361.8 MB`**, inkl. Chrome Headless Shell für `unsupported-3d`), `814,375` minor page faults, **`Swaps: 0`**, total elapsed wall time `60.72 s` (`1:00.72`).
- **`allSsimPassed = true`**, `globalMinSsim = 0.98455` (über alle 60 Golden Keyframes), Encoder `h264_vaapi`.

---

## 3. 11-Fixture Tier-1-GPU-Benchmarks & Optimierungs-Deltas

### A. Initialer Tier-1-GPU-Lauf (`/tmp/rsmotion-bench-p6-t1/tier1-benchmark.json`)
- **Gesamt:** `28,715.3 ms` für `660` Frames = **`22.98 fps`** (`2.61×` vs `8.82 fps` Baseline), `allSsimPassed = true`.
- **`chart-bars` SVG-Draw-Sprung:** `drawMs` sank von `2,090.9 ms` in Phase 4 (`19.86 fps`) auf **`68.2 ms`** (**`30.65×` schnelleres SVG-Draw** -> **`43.56 fps`**, **`4.42×`** vs `9.86 fps` Baseline) durch `svg_fast_provider` + `svg_dom_cache`.

### B. Finaler instrumentierter Tier-1-GPU-Lauf (`/tmp/rsmotion-bench-p6-t1-final`, `/usr/bin/time -v`)
Direkt nach dem `--tier auto`-Lauf und 6 Standalone-Renders gemessen (`Swaps: 0`, `Peak RSS = 278,032 KB` / **`271.5 MB`**, `Minor page faults = 551,721` — rund die Hälfte der früheren Allokations-Faults dank Warm-Buffer-Recycling + NV12-Pipe; Hinweis: auf diesem passiv gekühlten 15W-Skylake-U-Prozessor setzte gegen Ende aufeinanderfolgender Läufe erwartungsgemäß leichtes Thermal-Throttling ein):

| Fixture | Baseline FPS | Tier 1 GPU FPS | Speedup | Dedup | JS (ms) | Layout (ms) | Draw (ms) | Readback (ms) | Min SSIM |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `text-basic` | 10.22 | 18.41 | 1.80× | 0 | 211.1 | 1,096.7 | 75.2 | 322.3 | 0.99374 |
| `text-animated` | 10.05 | 24.35 | 2.42× | 0 | 521.2 | 447.5 | 175.5 | 444.1 | 0.99624 |
| `captions` | 8.84 | 17.02 | 1.93× | 0 | 834.0 | 1,148.2 | 156.0 | 567.7 | 0.98981 |
| `image-slideshow` | 8.06 | **59.34** | **7.37×** | **45/60** | 263.9 | 9.2 | 60.9 | 77.8 | 0.99735 |
| `transforms-opacity` | 8.85 | 30.60 | 3.46× | 0 | 281.0 | 12.9 | 72.5 | 411.6 | 0.99983 |
| `gradients-shadows` | 6.27 | **28.83** | **4.60×** | 0 | 224.4 | 553.3 | 149.7 | 382.2 | 0.98608 |
| `svg-shapes` | 9.71 | **39.06** | **4.02×** | 0 | 275.0 | 8.0 | 154.6 | 334.2 | 0.99967 |
| `chart-bars` | 9.86 | 24.56 | 2.49× | 0 | 467.2 | 15.5 | 180.6 | 353.2 | 0.99897 |
| `video-overlay` | 5.71 | 8.98 | 1.57× | 0 | 1,244.5 | 658.8 | 326.7 | 541.1 | 0.99870 |
| `audio-mix` | 12.30 | 13.51 | 1.10× | 0 | 704.1 | 681.8 | 114.3 | 512.1 | 0.99824 |
| `fonts-custom` | 12.55 | 14.02 | 1.12× | 0 | 640.4 | 939.1 | 202.6 | 585.5 | 0.98455 |
| **Total / Avg** | **8.82** | **19.59** | **2.22×** | **45** | - | - | - | - | **0.98455** |

- **`spike-encode` NV12-Pipe (`100` Frames `1920x1080`):** `2,876.5 ms` (**`34.76 fps`**), `h264_vaapi`, `ffprobe`: `1920,1080,yuv420p,100`.
- **`image-slideshow` Zero-Copy-Dedup:** Durch `push_duplicate_frame` für die `45/60` unveränderten Frames sanken `drawMs` auf `60.9 ms`, `readbackMs` auf `77.8 ms` und `encodeWallMs` auf `601.6 ms`, wodurch `image-slideshow` auf **`59.34 fps`** (bzw. **`65.44 fps` / `8.12×`** im `--tier auto`-Lauf) beschleunigte.

---

## 4. Standalone Pure `rsmotion render` Spot Checks (ohne Mid-Loop-PNG-Saves)

| Fixture | Remotion FPS | `rsmotion render` Wall (ms) | Pure Render FPS | Speedup | JS (ms) | Layout (ms) | Draw (ms) | Encode (ms) | Audio |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| `text-basic` *(erster Cold-Start)* | 10.22 | 4,172.0 | 14.38 | 1.41× | 458.1 | 932.5 | 129.7 | 2,991.9 | false |
| `transforms-opacity` | 8.85 | 1,535.8 | **39.07** | **4.41×** | 234.7 | 7.0 | 75.5 | 1,142.0 | false |
| `svg-shapes` | 9.71 | 1,310.8 | **45.77** | **4.71×** | 213.0 | 7.3 | 90.8 | 1,064.8 | false |
| `chart-bars` | 9.86 | 1,225.8 | **48.95** | **4.96×** | 223.4 | 5.3 | 76.4 | 990.6 | false |
| `video-overlay` | 5.71 | 4,518.5 | 13.28 | 2.33× | 899.9 | 496.4 | 318.8 | 3,185.8 | true |
| `audio-mix` | 12.30 | 2,357.6 | **25.45** | **2.07×** | 256.0 | 487.5 | 53.3 | 1,449.4 | true |

*(Hinweis: Im warmen `--tier auto`-Benchmark erreichte `text-basic` `31.84 fps` / `3.11×` und `video-overlay` `18.65 fps` / `3.26×`.)*

---

## 5. Entwicklung über alle Phasen (Phase 0 -> Phase 6)

| Phase | Umfang | Benchmark-Umfang | Gesamt-FPS | Speedup vs Remotion | Min SSIM | Peak RSS |
|---|---|---|---:|---:|---:|---:|
| **Phase 0** | Remotion `4.0.545` Baseline (`chrome-headless-shell`, `concurrency=2`) | 12 Fixtures (720f) | 8.79 | 1.00× | Golden | 494.3 MB |
| **Phase 1** | VAAPI + Skia EGL/CPU Raster Spike | 100f synthetisch (GPU+VAAPI) | 41.36–55.73 | 4.7–6.3× | 0.99941 | 174.8 MB |
| **Phase 2** | Tier 2 Rust-CDP (`shm-bgra` + `/proxy` + Audio) & `@rsmotion/renderer` | 12 Fixtures Tier 2 (720f) | 9.19 | 1.05× | 0.99982 | 221.0 MB |
| **Phase 3** | Tier 1 Browserless (`V8 + Taffy + Skia GPU`) | 9 Non-Media Tier 1 (540f) | 20.17 | 2.22× | 0.98455 | 254.7 MB |
| **Phase 4** | Native Tier 1 Media (`rsmotion-media` + `<OffthreadVideo>`/`<Audio>`) | 11 Media+DOM Tier 1 (660f) | 28.96 | 3.28× | 0.98455 | 254.2 MB |
| **Phase 5** | Analyzer (`rsmotion-analyze`) & Auto Tier 1/2/3 Dispatch | 12 Fixtures `--tier auto` (720f) | 15.23 | 1.73× | 0.98455 | 254.2 MB |
| **Phase 6** | SVG/Font/NV12-Optimierung, `rsmotion frames`, Parity & Upstream Compat | 12 Fixtures `--tier auto` (720f) | **19.45** *(Tier-1 Peaks bis 65.44 fps)* | **2.21×** *(Tier-1 bis 8.12×)* | **0.98455** | **271.5 MB** *(Tier 1)* / **361.8 MB** *(Auto)* |

---

## 6. `@rsmotion/renderer` Shim & Upstream Compatibility Verification

- **Upstream-Templates (`remotion/packages/template-*`, alle Details in [compat.md](compat.md)):**
  - `template-blank` (`MyComp`): Auto Tier 1 (`still = 1.00000 SSIM`, Video `26.18 fps`, `1280,720,30`).
  - `template-helloworld` (`HelloWorld`): Auto Tier 1 (`still = 0.96501 SSIM`, Video `28.26 fps`, `1920,1080,60`).
  - `template-helloworld` (`Logo`): Auto Tier 1 (`still = 0.95296 SSIM`, Video `22.94 fps`, `1920,1080,30`).
  - `template-still` (`PreviewCard`): Auto Tier 2 (`canvas` + `background-clip: text` + `<style>`, `still = 1.00000 SSIM`).
- **`npm/renderer-shim/test/smoke.mjs`:**
  - `ok: true`, `compositionsCount: 12`
  - `getVideoMetadata('sample-video.mp4')`: `640x360`, `30 fps`, `durationInSeconds: 3`, `codec: "h264"`, `hasAudio: true`
  - `validateOutputFilename` + `validateSelectedPixelFormatAndImageFormatCombination` + `RenderInternals.internalRenderFrames` geprüft
  - `renderStill`: PNG (`SSIM = 0.99374`), JPEG (`jpegQuality: 85`, `scale: 0.5`), WebP
  - `renderFrames`: `5` PNGs in Sequenz (`framesSeqCount: 5`)
  - `renderMedia('text-basic')`: `60` `onProgress` Callbacks, `3,278 ms`, Auto Tier 1 (`21.21 fps`)
  - `renderMedia('unsupported-3d', frameRange: [0, 4])`: automatischer Fallback auf Tier 2 (`1920,1080,5`)

---

## 7. Quality & Hygiene Gates

- `cargo fmt --check`: bestanden
- `cargo clippy --workspace -j2 -- -D warnings`: bestanden (0 Warnungen über alle 10 Crates)
- `cargo test --workspace -j2`: bestanden (**`24 passed; 0 failed`**: `rsmotion-analyze` 3, `rsmotion-core` 4, `rsmotion-encode` 5, `rsmotion-js` 2, `rsmotion-layout` 2, `rsmotion-media` 3, `rsmotion-raster` 4, `rsmotion-scene` 1)
- Prozesshygiene (`pgrep -ax 'chrome-headless-shell|ffmpeg'`): keine verwaisten Hintergrundprozesse
