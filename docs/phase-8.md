# Phase 8 Report: `rsmotion-css` Engine, Color 4, CSS Grid & Native Skia GPU Effects

## 1. Summary

Phase 8 eliminates Tier 2 Chrome fallbacks for modern stylesheets, Tailwind v4 utilities, CSS Grid layouts, Color 4 spaces, and GPU visual effects/transforms across the native Rust pipeline, raising Tier 1 coverage on the expanded 14-fixture suite to **13 / 14 fixtures** (**`840` frames**, with only `<canvas>` in `unsupported-3d` remaining on Tier 2) and delivering **49.61 fps overall** (**`5.96×`** vs Remotion `100,839.0 ms`, up to **`12.98×`** on `css-effects-3d`) at **298.0 MB Peak RSS** and **`Swaps: 0`**:

1. **Pure-Rust CSS Engine (`crates/rsmotion-css` using `cssparser 0.38.0`):**
   - Full stylesheet parser & at-rule engine supporting `@layer` ordering, `@media` queries, `@supports`, `@font-face` extraction, and `@property` typed custom property initial values and inheritance.
   - CSS selector matcher (`src/selector.rs`) covering tags, `#id`, `.class` including Tailwind escaped selectors (`.bg-\[\#0f172a\]`, `.w-1\/2`, `.hover\:scale-105`), universal `*`, attributes (`[attr]`, `[attr="v"]`), combinators (` `, `>`, `+`, `~`), pseudo-classes (`:first-child`, `:last-child`, `:nth-child(An+B)`, `:not(...)`, `:is(...)`, `:where(...)`, `:root`), O(N) `NodeDomMeta`, Cascade Level 5 priority + bucketed selector index, and `!important`.
   - Cycle-safe custom property inheritance (`--*` & `var(--name, fallback)`) and full math evaluator (`calc()`, `min()`, `max()`, `clamp()`, `abs()`) with an 8-entry thread-local compiled stylesheet cache.
2. **DOM Stylesheet Bridge & Analyzer Dispatch (`rsmotion-scene`, `rsmotion-js`, `rsmotion-analyze`):**
   - Extended `SceneTree` with `StyleSheetSource { source, css }` and added `classes`/`attrs` onto `NodeKind::Svg`.
   - Added canonicalized, `bundle_dir`-confined `op_read_text_asset` (rejecting remote URLs), HTML `<style>` + `<link rel="stylesheet">` injection via `parse_html_styles`, DOM-order stylesheet serialization in `host.js`, and `unresolved_external_stylesheets` gating in `rsmotion-analyze/src/probe.rs`.
   - Removed Tier 2 fallback triggers in `host.js` for `css-grid`, `filter`, `backdrop-filter`, `mix-blend-mode`, `clip-path`, `mask-image`, `background-clip: text`, and 3D `transform` (leaving `unsupported-3d` routed to Tier 2 solely on `canvas:canvas`).
3. **Color 4 & Full CSS Grid Layout (`crates/rsmotion-layout`):**
   - Added Color 4 parsing (`src/color4.rs`) for `oklch(...)`, `oklab(...)`, and `color-mix(in <space>, ...)`.
   - Wired full Taffy `0.15.1` CSS Grid (`src/grid_parse.rs`): `grid-template-columns/rows` (`repeat(...)`, `fr`, `minmax()`, `auto`), `grid-auto-flow`, `grid-auto-columns/rows`, `grid-column/row` (`span N`, `a / b`), `place-items`, `place-content`, `justify-items`, `align-items`, `justify-self`, `align-self`.
   - Integrated CSS cascade + `var()` substitution into `engine.rs` and parsed filters, blend modes, clip paths, masks, `background-clip: text`, standalone `translate`/`rotate`/`scale`, and 3D `TransformOp` + `perspective`/`perspective-origin`.
4. **Native Skia GPU Effects & 3D Transforms (`crates/rsmotion-raster`):**
   - Synced `@font-face` rules from stylesheets into `FontCache`, implemented 3D `M44` matrix transforms + perspective (`effects.rs`), `clip-path` (`inset`/`circle`/`ellipse`/`polygon`/`path()`), `backdrop-filter` with `TileMode::Clamp` inside `outer_rrect` prior to element compositing `SaveLayerRec`, chained `filter` DAG (`sigma = r`), all 16 CSS `mix-blend-mode`s, `mask-image` (`BlendMode::DstIn`), `background-clip: text` (`BlendMode::SrcIn` / `SrcATop`), and extended `hash_node` frame deduplication.
5. **New Phase 8 Fixtures & Remotion Baselines (`tests/fixtures`, `tests/golden`, `bench/baseline.json`):**
   - Added `CssTailwindGrid.tsx` (`css-tailwind-grid`: `--spacing`, `@property --card-radius`, `@layer`, `oklch`/`color-mix`, `display: grid` with `repeat(3, 1fr)`, `calc()`, `span 2`) and `CssEffects3d.tsx` (`css-effects-3d`: `filter`, `backdrop-filter: blur(...)`, `mix-blend-mode`, `clip-path`, `mask-image`, `background-clip: text`, 3D `rotateY`/`perspective`).
   - Updated Chrome golden PNGs and `bench/baseline.json` to 14 fixtures / 840 frames (Remotion `@remotion/renderer` baseline: **`100,839.0 ms`**, **`8.33 fps`** avg, max RSS **`494.3 MB`**).

---

## 2. Final 14-Fixture Automatic Benchmark (`--tier auto --raster gpu`)

Measured with `/usr/bin/time -v ./target/release/rsmotion benchmark --tier auto --raster gpu --concurrency 2 --gl none --min-ssim 0.97 --out-dir /tmp/rsmotion-bench-p8-auto`:

| Fixture | Chosen Tier | Remotion FPS | Auto FPS | ms/Frame | Speedup | Audio | Min SSIM (5 kf) | Avg SSIM |
|---|---|---:|---:|---:|---:|---|---:|---:|
| `text-basic` | 1 (`tier1-native`) | 10.22 | 68.27 | 14.65 | 6.68× | false | 0.99374 | 0.99529 |
| `text-animated` | 1 (`tier1-native`) | 10.05 | 81.84 | 12.22 | 8.14× | false | 0.99624 | 0.99706 |
| `captions` | 1 (`tier1-native`) | 8.84 | 86.38 | 11.58 | 9.78× | false | 0.98981 | 0.98996 |
| `image-slideshow` | 1 (`tier1-native`) | 8.06 | 95.75 | 10.44 | 11.89× | false | 0.99735 | 0.99833 |
| `transforms-opacity` | 1 (`tier1-native`) | 8.85 | 95.88 | 10.43 | 10.84× | false | 0.99983 | 0.99991 |
| `gradients-shadows` | 1 (`tier1-native`) | 6.27 | 77.19 | 12.95 | 12.31× | false | 0.98608 | 0.98801 |
| `svg-shapes` | 1 (`tier1-native`) | 9.71 | **101.30** | 9.87 | 10.43× | false | 0.99967 | 0.99967 |
| `chart-bars` | 1 (`tier1-native`) | 9.86 | 96.14 | 10.40 | 9.76× | false | 0.99897 | 0.99914 |
| `video-overlay` | 1 (`tier1-native`) | 5.71 | 36.32 | 27.53 | 6.36× | true | 0.99870 | 0.99885 |
| `audio-mix` | 1 (`tier1-native`) | 12.30 | 49.11 | 20.36 | 3.99× | true | 0.99824 | 0.99848 |
| `fonts-custom` | 1 (`tier1-native`) | 12.55 | **111.34** | 8.98 | 8.87× | false | 0.98455 | 0.98459 |
| `css-tailwind-grid` | 1 (`tier1-native`) | 10.38 | 84.29 | 11.86 | **8.12×** | false | **0.98736** | 0.98748 |
| `css-effects-3d` | 1 (`tier1-native`) | 4.58 | 59.42 | 16.83 | **12.98×** | false | **0.99446** | 0.99479 |
| `unsupported-3d` | 2 (`tier2-cdp`) | 8.46 | 9.61 | 104.09 | 1.14× | false | 1.00000 | 1.00000 |
| **Total / Avg** | **13 Tier 1 / 1 Tier 2** | **8.33** | **49.61** | **—** | **5.96×** | **2/2** | **0.98455** | **—** |

- **Total Render Time (`840` frames):** **`16,932.1 ms`** (`16.93 s`) vs Remotion **`100,839.0 ms`** (**`5.96×`** overall).
- **Resource Usage (`/usr/bin/time -v`):** Peak RSS `305,160 KB` (**`298.0 MB`**), `911,567` minor page faults, **`Swaps: 0`**, elapsed wall clock `32.52 s`.
- **`allSsimPassed = true`**, `globalMinSsim = 0.98455`. `unsupported-3d` routed to Tier 2 solely on `canvas:canvas`.

---

## 3. Quality Gates & Hygiene

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: **32 passed, 0 failed** (1 upstream `deno_core` doc-test ignored):
  - `rsmotion-analyze`: 3/3 (`static_scan_finds_bundle_hints_without_external_css`, `remote_url_routes_to_tier2_and_video_overlay_reuses_host`, `analyzes_all_14_fixtures_accurately`)
  - `rsmotion-cli`: 2/2 (`resolve_worker_count_gates_short_clips_and_caps_workers`, `multi_worker_chunking_renders_and_concats_chunks`)
  - `rsmotion-core`: 4/4
  - `rsmotion-css`: 3/3 (`layer_order_specificity_and_important_follow_cascade_level_5`, `font_face_and_var_cycles_do_not_panic`, `selectors_combinators_nth_child_escaped_tailwind_and_vars_calc`)
  - `rsmotion-encode`: 5/5
  - `rsmotion-js`: 2/2 (`lists_all_12_compositions_in_v8` verifying 14 compositions, `text_basic_and_fonts_custom_and_images_and_svgs_snapshot`)
  - `rsmotion-layout`: 3/3 (including `parses_oklch_color_mix_effects_3d_and_computes_css_grid`)
  - `rsmotion-media`: 3/3
  - `rsmotion-raster`: 6/6 (including `renders_gpu_css_effects_3d_and_deduplicates_cleanly`)
  - `rsmotion-scene`: 1/1
- `tsc && node npm/renderer-shim/test/smoke.mjs`: passed (`ok: true`, `compositionsCount: 14`, `stillSsim: 0.9937430637577008`, `framesSeqCount: 5`, `progressCalls: 60`, `text-basic` 60f at **`37.06 fps`** / `1618.89 ms`).
- `./target/release/rsmotion explain tests/fixtures/build`: **13 Tier 1 (`tier1-native`)**, **1 Tier 2 (`tier2-cdp`: `unsupported-3d` on `canvas:canvas`)**.
- Process hygiene (`pgrep -ax 'chrome-headless-shell|ffmpeg' || echo CLEAN`): **`CLEAN`**.
