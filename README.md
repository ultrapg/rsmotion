# rsmotion

> **High-performance pure-Rust video renderer and Rolldown bundler for Remotion compositions — Chrome-free by default, GPU-accelerated, with automatic browser fallback.**

![Rust](https://img.shields.io/badge/Language-Rust-orange?logo=rust&logoColor=white)
![Platform](https://img.shields.io/badge/Platform-Linux%20x86__64-2496ED?logo=linux&logoColor=white)
![Remotion Compat](https://img.shields.io/badge/Target-Remotion%20v4.0.545-0B84F3)
![License](https://img.shields.io/badge/License-Apache%202.0-blue)

**rsmotion** is a standalone, 12-crate Rust engine and CLI compatible with Remotion (`v4.0.545`) projects and `@remotion/renderer` workflows. Instead of launching headless Chromium and screenshots for every frame, **rsmotion** compiles bundles in Rust with `rolldown`, evaluates React/Remotion frames inside an embedded V8 runtime (`deno_core`), computes layout with `taffy`, rasterizes directly on a headless Skia EGL GPU surface, and pipes raw RGBA frames straight into `ffmpeg` — while seamlessly falling back to Chrome CDP (Tier 2) for unsupported browser APIs such as WebGL or `<iframe>`.

- **Architecture:** 12-crate Rust workspace (`Rolldown` + `V8` + `rsmotion-css` + `Taffy` + `Skia EGL` + `FFmpeg`)
- **Platform:** Tested on **Linux `x86_64`** (with optional VAAPI / EGL GPU hardware acceleration; software CPU fallbacks included)
- **License:** Apache License 2.0 ([`LICENSE`](./LICENSE))

---

## Highlights

- **Pure-Rust Rolldown Bundler (`97.1×` faster than Webpack):**
  - Directly bundles `.ts`, `.tsx`, `.js`, `.jsx`, or project directories via `rolldown 1.3.2` — eliminating the slow Node/Webpack step (`329.7 ms` cold vs `32,021 ms` Webpack, **`1.88 ms`** on warm SHA-256 incremental cache hits under `~/.cache/rsmotion/bundles`).
  - Handles React 19 automatic JSX transforms, singleton module pinning, local CSS `@import` inlining, CSS Modules, optional `@tailwindcss/cli` compilation (`--enable-tailwind`), static asset emission, and `public/` mirroring/symlinking.
- **Chrome-Free Native Tier 1 Rendering Engine:**
  - **Embedded V8 (`deno_core`):** Evaluates React 19 & Remotion hooks (`useCurrentFrame`, `useVideoConfig`, `interpolate`, `spring`, `<Sequence>`, `<AbsoluteFill>`, `calculateMetadata`, `delayRender`/`continueRender`, deterministic timers) with synthetic DOM, CSSOM, Canvas 2D, and Web Audio host bindings.
  - **Custom CSS & Taffy Layout (`rsmotion-css` + `rsmotion-layout`):** Supports selector matching (`:nth-child`, attribute selectors, combinators), `@layer` cascade, CSS custom properties (`--var()`), `calc()`, `oklch()` / `color-mix()`, Flexbox (`flex` / `inline-flex`), CSS Grid (`grid-template-columns`/`rows`, `repeat()`, `auto-flow`), `aspect-ratio`, and replaced-element sizing via `taffy 0.15.1`.
  - **Hardware Skia GPU Rasterization (`rsmotion-raster`):** Renders directly on headless EGL OpenGL ES surfaces (with CPU raster fallback) for gradients, shadows, 2D/3D transforms, CSS `filter` / `backdrop-filter`, `mix-blend-mode`, `clip-path`, `background-clip: text`, inline `<svg>`, HTML5 `<canvas>` 2D (`CanvasRenderingContext2D`, `Path2D`), and native Lottie (`skia_safe::skottie`).
  - **Native Typography & Audio (`rsmotion-media`):** Pure-Rust `wuff` WOFF/WOFF2 font decoding, cached Google Fonts CSS (`~/.cache/rsmotion/assets/`, offline-safe via `--offline`), `symphonia` + FFmpeg audio decoding for `@remotion/media-utils` (`getAudioData`, `useWindowedAudioData`, `visualizeAudio`), and direct `<OffthreadVideo>` / `<Video>` frame decoding.
- **Automatic 3-Tier Dispatch (`rsmotion-analyze`):**
  - Statically scans bundles and runs a dry-run V8 probe (`rsmotion explain`) to route each composition cleanly: **Tier 1** (pure Rust + Skia), **Tier 2** (direct Chrome CDP + shared-memory capture for WebGL, `<iframe>`, SVG `<foreignObject>`, remote `http(s)` video/serveUrl), or optional **Tier 3** (`--force-tier 3` delegating to stock `@remotion/renderer`).
- **Hardware Video Encoding & Audio Tools (`rsmotion-encode`):**
  - Zero-copy RGBA pipe into `ffmpeg` supporting `h264`, `h265`, `vp8`, `vp9`, `prores`, `gif`, `mp3`, `aac`, and `wav`, automatic VAAPI/QSV hardware probing with software (`libx264`) fallback, BT.709 / BT.2020-ncl color-space metadata, ProRes profiles, multi-track audio mixing (`aac`/`mp3`/`opus`/`pcm-16`), `silent-parts` (`silencedetect`), and stream-copy `extract-audio`.
- **Drop-In `@rsmotion/renderer` TypeScript Shim:**
  - Provides `@remotion/renderer` + `bundle()` API parity (`bundle`, `getCompositions`, `selectComposition`, `renderMedia`, `renderStill`, `renderFrames`, `getVideoMetadata`, `getSilentParts`, `extractAudio`, `makeCancelSignal`) backed by the native `rsmotion` binary ([`npm/renderer-shim`](./npm/renderer-shim/README.md)).

---

## Architecture

```text
  Entrypoint (.ts / .tsx / dir) or Bundle URL
                        |
                        v
        +-------------------------------+
        |  rsmotion-bundler (Rolldown)  |  --> index.html + bundle.js + assets/
        +---------------+---------------+      (SHA-256 cache under ~/.cache/rsmotion)
                        |
                        v
        +-------------------------------+
        |  rsmotion-analyze (Probe)     |  --> Static AST/bundle scan + V8 dry-run
        +-------+---------------+-------+
                |               |
      Tier 1 (Native)       Tier 2 (Browser Fallback: WebGL, <iframe>, <foreignObject>)
                |               |
                v               v
   +-------------------------+  +-------------------------+
   | V8 (deno_core) DOM Shim |  | Direct Chrome CDP       |
   | rsmotion-css + Taffy    |  | HTTP Server + SHM Grab  |
   | Skia EGL GPU + Skottie  |  +------------+------------+
   | symphonia / FFmpeg      |               |
   +------------+------------+               |
                |                            |
                +-------------+--------------+
                              |
                              v
        +---------------------------------------+
        |     rsmotion-encode (FFmpeg Pipe)     |  --> VAAPI / QSV / libx264 / ProRes / GIF
        |   + Multi-Track Audio Mix / Mux       |      + PNG / JPEG / WebP stills & sequences
        +---------------------------------------+
```

### Workspace Crates

| Crate | Responsibility |
| :--- | :--- |
| [`rsmotion-core`](./crates/rsmotion-core) | Shared configuration structs, codecs, frame ranges, pixel formats, error types |
| [`rsmotion-bundler`](./crates/rsmotion-bundler) | Pure-Rust `rolldown 1.3.2` bundler, incremental cache, CSS/Tailwind CLI, HTML generator |
| [`rsmotion-analyze`](./crates/rsmotion-analyze) | Static + V8 runtime capability analyzer driving automatic Tier 1 / Tier 2 routing |
| [`rsmotion-js`](./crates/rsmotion-js) | Embedded V8 (`deno_core`) isolate, synthetic DOM/Canvas 2D/Web Audio shims |
| [`rsmotion-scene`](./crates/rsmotion-scene) | Serializable `SceneTree` / `SceneNode` / `Canvas2DOp` / `LottieSource` IR |
| [`rsmotion-css`](./crates/rsmotion-css) | Pure-Rust CSS parser (`cssparser`), cascade, custom properties, `calc()`, `oklch()` |
| [`rsmotion-layout`](./crates/rsmotion-layout) | Flexbox & CSS Grid layout solver backed by `taffy 0.15.1` |
| [`rsmotion-raster`](./crates/rsmotion-raster) | Skia EGL GPU/CPU rasterizer, Canvas 2D, Skottie Lottie, SVG, SSIM diffing |
| [`rsmotion-media`](./crates/rsmotion-media) | `symphonia` + FFmpeg decoding, `wuff` WOFF/WOFF2 fonts, `ureq` asset cache, `silencedetect` |
| [`rsmotion-encode`](./crates/rsmotion-encode) | Zero-copy FFmpeg encoder pipe (VAAPI/QSV/CPU), color tags, audio muxing & extraction |
| [`rsmotion-cdp`](./crates/rsmotion-cdp) | Direct Chrome DevTools Protocol client and static server for Tier 2 compositions |
| [`rsmotion-cli`](./crates/rsmotion-cli) | `rsmotion` command-line binary orchestrating all subcommands and tiers |

---

## Benchmarks

Measured against Remotion `v4.0.545` (`chrome-headless-shell`, `--concurrency 2`) across the **18-fixture suite** (`1920×1080` @ 30 fps, 60 frames each = **`1,080` frames total**) and **6 upstream Remotion templates**:

| Workload | Remotion `4.0.545` | `rsmotion` | Speedup / Parity |
| :--- | :--- | :--- | :--- |
| **Cold Bundle (`tests/fixtures/src/index.ts`)** | `32,021 ms` (Webpack) | **`329.7 ms`** (Rolldown, `1.88 ms` cached) | **`97.1×` faster** |
| **Full 18-Fixture Suite (1,080 frames)** | `285.49 s` (**`3.78 fps`**) | **`31.82 s`** (**`33.94 fps`**, Peak RSS **`329.7 MiB`**) | **`8.97×` overall**, Min SSIM **`0.98455`** |
| `video-overlay` (Tier 1) | `0.64 fps` | **`17.09 fps`** | **`26.89×`**, Min SSIM `0.99870`, AAC verified |
| `audio-mix` (Tier 1) | `1.49 fps` | **`24.23 fps`** | **`16.31×`**, Min SSIM `0.99824`, AAC verified |
| `text-animated` (Tier 1) | `5.83 fps` | **`58.96 fps`** | **`10.11×`**, Min SSIM `0.99624` |
| `lottie-native` (Tier 1 Skottie GPU) | `9.19 fps` | **`81.34 fps`** | **`8.85×`**, Min SSIM `0.99923` |
| `unsupported-webgl` (Auto Tier 2 CDP) | `7.40 fps` | **`9.34 fps`** | **`1.26×`**, Min SSIM **`1.00000`** |
| **6/6 Upstream Templates** (`template-blank`, `helloworld`, `still`, `overlay`, `audiogram`, `music-visualization`) | Chrome Tier 3 | **All 6 on Native Tier 1** | **`SSIM >= 0.9614`** vs Tier 3 |

See [`docs/compat.md`](./docs/compat.md) for the complete fixture, template, and CSS/API compatibility matrix.

---

## Quick Start

### Prerequisites

- **Rust** toolchain (`cargo`, `rustc`) — repository currently embeds V8 (`deno_core`) + Skia (`skia-safe`) + `rolldown`
- **`ffmpeg`** and **`ffprobe`** installed on `PATH`
- **Linux `x86_64`** (optional EGL / VAAPI drivers for GPU rasterization & hardware encoding; CPU/software fallbacks are automatic)
- **Node.js** (only needed to resolve `node_modules` dependencies of Remotion projects, Tailwind CLI, or the `@rsmotion/renderer` JS shim)

### Build from Source

```bash
git clone https://github.com/ultrapg/rsmotion.git
cd rsmotion
cargo build --release
```

The release binary is placed at `./target/release/rsmotion`.

---

## Usage & CLI Examples

### Render Directly from a TypeScript Entrypoint or Project Directory

```bash
# Auto-bundle with Rolldown, auto-select Tier 1 GPU or Tier 2 CDP, and render H.264 MP4
./target/release/rsmotion render src/index.ts MyComposition out/video.mp4 --overwrite

# Specify bitrate, AAC audio, and BT.709 color-space tags
./target/release/rsmotion render src/index.ts MyComposition out/video.mp4 \
  --video-bitrate 4M --audio-codec aac --color-space bt709 --overwrite

# Render an image sequence, GIF, or ProRes 422 HQ
./target/release/rsmotion render src/index.ts MyComposition out/frames --sequence --frames 0-29 --overwrite
./target/release/rsmotion render src/index.ts MyComposition out/preview.gif --codec gif --number-of-gif-loops 0 --overwrite
./target/release/rsmotion render src/index.ts MyComposition out/master.mov --codec prores --prores-profile hq --overwrite
```

### Render Stills, Frame Sequences & Pre-Bundled Outputs

```bash
# Render frame 30 as PNG, JPEG, or WebP
./target/release/rsmotion still src/index.ts MyComposition out/thumb.png --frame 30 --overwrite

# Explicit Rolldown bundle + composition inspection + tier explanation
./target/release/rsmotion bundle src/index.ts build --enable-tailwind
./target/release/rsmotion compositions build --json
./target/release/rsmotion explain build
```

### Audio Inspection & Extraction

```bash
# Detect silent and audible intervals via FFmpeg silencedetect
./target/release/rsmotion silent-parts public/audio.wav --noise-threshold -20 --min-duration 0.5 --json

# Stream-copy / extract audio track without re-encoding video
./target/release/rsmotion extract-audio public/input.mp4 out/audio.m4a --overwrite
```

### Running Workspace Tests & Benchmarks

```bash
cargo test --workspace
./target/release/rsmotion benchmark --tier auto --raster gpu --min-ssim 0.97
```

*(Running `tests/fixtures` benchmarks requires installing fixture npm dependencies first: `npm --prefix tests/fixtures ci`.)*

---

## Drop-In `@rsmotion/renderer` API

Build the TypeScript shim in [`npm/renderer-shim`](./npm/renderer-shim):

```ts
import {
  bundle,
  renderMedia,
  selectComposition,
} from '@rsmotion/renderer';

const serveUrl = await bundle({
  entryPoint: './src/index.ts',
  enableTailwind: true,
});

const composition = await selectComposition({
  serveUrl,
  id: 'MyComposition',
});

await renderMedia({
  composition,
  serveUrl,
  codec: 'h264',
  videoBitrate: '4M',
  audioCodec: 'aac',
  colorSpace: 'bt709',
  outputLocation: './out/video.mp4',
  overwrite: true,
  onProgress: ({ progress }) => console.log(`${Math.round(progress * 100)}%`),
});
```

---

## Platform Support & Documentation

- **Tested Platform:** Linux `x86_64` (tested with Mesa/Intel VAAPI + EGL GPU and software fallbacks). **macOS and Windows builds are currently untested** and may require adjustments to headless EGL GPU context creation or platform binaries.
- **Offline Operation:** Pass `--offline` to disallow remote HTTP downloads and use strictly cached assets (`~/.cache/rsmotion/assets`) and local files.
- **Further Reading:**
  - [`docs/compat.md`](./docs/compat.md) — Benchmark tables, template parity, and Tier 1 vs. Tier 2 feature matrix
  - [`docs/api-parity.md`](./docs/api-parity.md) — `@remotion/renderer` & CLI parity tracking against Remotion `v4.0.545`
  - [`npm/renderer-shim/README.md`](./npm/renderer-shim/README.md) — Node/TypeScript shim reference

---

## License & Disclaimer

- **`rsmotion` Source Code:** Distributed under the **Apache License, Version 2.0 (`Apache-2.0`)**. See [`LICENSE`](./LICENSE) for full terms.
- **No Affiliation:** `rsmotion` is an independent open-source project and is **not** affiliated with, endorsed by, or sponsored by Remotion.
- **Remotion License Notice:** Projects bundled or rendered with `rsmotion` that import `remotion` or `@remotion/*` npm packages remain subject to the official **Remotion License** for those packages.
- **Third-Party Tools & Test Assets:** `ffmpeg` and `ffprobe` are invoked externally as CLI subprocesses. Test fixture font `tests/fixtures/public/fonts/SpaceGrotesk-Regular.ttf` is licensed under the SIL Open Font License 1.1.

---
Developed by [@ultrapg](https://github.com/ultrapg)
