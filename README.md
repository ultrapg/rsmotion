# rsmotion

> **Ultra-Fast, Chrome-Free Native Rust Renderer & Rolldown Bundler for Remotion**  
> High-performance video (`render`), still (`still`), frame-sequence (`frames`), and bundle (`bundle`) CLI for Remotion `v4.0.545` projects with **zero Chromium overhead on Tier 1**, pure-Rust Rolldown bundling (`97.1x` faster than Webpack), embedded V8, `taffy` Flexbox/Grid, headless Skia EGL GPU rasterization, native Skottie Lottie, HTML5 `<canvas>` 2D, and automatic Chrome CDP fallback for WebGL.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux%20x86__64-orange.svg)](#building-from-source)
[![Engine](https://img.shields.io/badge/Engine-Rust%20%2B%20V8%20%2B%20Skia%20GPU-green.svg)](#architecture-overview)
[![Release](https://img.shields.io/badge/Release-v0.1.0-purple.svg)](https://github.com/ultrapg/rsmotion/releases/tag/v0.1.0)

---

## What's New in v0.1.0

- **End-to-End Chrome-Free Tier 1 Rendering Engine (`crates/rsmotion-js`, `rsmotion-css`, `rsmotion-layout`, `rsmotion-raster`, `rsmotion-media`)**:
  - Evaluates React 19 + Remotion compositions directly inside an in-process V8 isolate (`deno_core 0.413.0`) with deterministic timer/`requestAnimationFrame` scheduling, `delayRender`/`continueRender`, synthetic DOM/CSSOM/Canvas 2D/Web Audio shims, and compact `SceneTree` snapshot serialization (`crates/rsmotion-scene`).
  - Solves pure-Rust CSS cascade (`cssparser 0.38.0` with `@layer`, specificity, `:nth-child`/attribute selectors, `--var()`, `calc()`, `oklch()`, `color-mix()`), Flexbox & CSS Grid layout (`taffy 0.15.1` including replaced-element `aspect-ratio`), and hardware-accelerated headless EGL Skia rasterization (`skia-safe 0.157.0`) for gradients, shadows, 2D/3D transforms, `filter`/`backdrop-filter`, `mix-blend-mode`, `clip-path`, `background-clip: text`, inline `<svg>`, HTML5 `<canvas>` 2D (`CanvasRenderingContext2D`, `Path2D`), and native Lottie (`skia_safe::skottie`).
  - Decodes fonts (`wuff 0.2.10` WOFF/WOFF2 + cached Google Fonts CSS with `--offline` support), video frames (`<OffthreadVideo>`/`<Video>`), and `@remotion/media-utils` audio (`symphonia 0.5.5` + FFmpeg) natively in-process.
- **Pure-Rust Rolldown Bundler & Zero-Config Auto-Bundling (`crates/rsmotion-bundler`)**:
  - Replaces Node/Webpack (`32,021 ms` cold) with `rolldown 1.3.2` (`329.7 ms` cold — **`97.1x` speedup**) and SHA-256 incremental caching (`1.88 ms` warm hits under `~/.cache/rsmotion/bundles`), with automatic `.ts`/`.tsx`/`.js`/`.jsx`/directory bundling in `render`, `still`, `frames`, `compositions`, and `explain`, local CSS `@import` inlining, CSS Modules, optional `--enable-tailwind`, static asset emission, and `public/` mirroring.
- **Automatic 3-Tier Dispatch & Hardware Video Encoding (`crates/rsmotion-analyze`, `rsmotion-cdp`, `rsmotion-encode`)**:
  - Statically scans bundles and probes frames in V8 to run 17/18 benchmark fixtures natively on Tier 1 GPU, automatically routing browser-only features (`webgl`/`webgl2`, `<iframe>`, SVG `<foreignObject>`, remote HTTP video) to direct Chrome CDP + shared-memory capture (`Tier 2`), with optional `--force-tier 3` stock `@remotion/renderer`.
  - Pipes raw frames straight into `ffmpeg` (`h264_vaapi`, `qsv`, `libx264`, `h265`, `vp8`, `vp9`, `prores_ks`, `gif`, `mp3`, `aac`, `wav`) with BT.709/BT.2020-ncl color-space metadata, ProRes profiles, multi-track audio mixing, `silent-parts`, and `extract-audio`.
- **Drop-In `@rsmotion/renderer` Shim (`npm/renderer-shim`)**:
  - Exports TypeScript-typed `bundle()`, `getCompositions()`, `selectComposition()`, `renderMedia()`, `renderStill()`, `renderFrames()`, `getVideoMetadata()`, `getSilentParts()`, `extractAudio()`, and `makeCancelSignal()`.

### Measured Speedup & Visual Parity (Remotion `4.0.545` vs. `rsmotion` `v0.1.0`)

Measured against Remotion `v4.0.545` (`chrome-headless-shell`, `--concurrency 2`) across the **18-fixture suite** (`1920x1080` @ 30 fps, 60 frames each = **`1,080` frames total**) and **6 upstream Remotion templates**:

| Workload | Remotion `4.0.545` | `rsmotion` (`v0.1.0`) | Speedup | Visual Parity (`Min SSIM`) |
| :--- | :---: | :---: | :---: | :---: |
| Cold bundle (`tests/fixtures/src/index.ts`) | `32,021 ms` (Webpack) | **`329.7 ms`** (`1.88 ms` cached) | **`97.1x`** | Identical `17 T1 / 1 T2` scan |
| **Full 18-fixture suite (`1,080` frames)** | `285.49 s` (`3.78 fps`, `494.8 MB` RSS) | **`31.82 s`** (**`33.94 fps`**, **`329.7 MiB`** RSS) | **`8.97x`** | **`0.98455`** (across 90 keyframes) |
| `video-overlay` (Tier 1 GPU + video/audio) | `0.64 fps` | **`17.09 fps`** | **`26.89x`** | **`0.99870`** (+ AAC verified) |
| `audio-mix` (Tier 1 GPU + multi-track mix) | `1.49 fps` | **`24.23 fps`** | **`16.31x`** | **`0.99824`** (+ AAC verified) |
| `text-animated` (Tier 1 GPU) | `5.83 fps` | **`58.96 fps`** | **`10.11x`** | **`0.99624`** |
| `canvas-2d` (Tier 1 Canvas 2D on GPU) | `5.88 fps` | **`54.62 fps`** | **`9.28x`** | **`0.98499`** |
| `lottie-native` (Tier 1 Skottie GPU) | `9.19 fps` | **`81.34 fps`** | **`8.85x`** | **`0.99923`** |
| `unsupported-webgl` (Auto Tier 2 CDP) | `7.40 fps` | **`9.34 fps`** | **`1.26x`** | **`1.00000`** |
| **6/6 Upstream Templates** (`template-blank`..`music-visualization`) | Chrome Tier 3 | **All 6 on Native Tier 1** | **Up to `8.1x` still** | **`>= 0.9614`** vs Tier 3 |

---

## Table of Contents

- [Overview](#overview)
- [The Re-Engineering: Headless Chrome + Webpack vs. Native Rust](#the-re-engineering-headless-chrome--webpack-vs-native-rust)
- [Architecture Overview](#architecture-overview)
- [Quick Start: Prebuilt Linux Binary (v0.1.0)](#quick-start-prebuilt-linux-binary-v010)
- [CLI Usage & Command Reference](#cli-usage--command-reference)
  - [1. Render Video or Frame Sequence (`rsmotion render`)](#1-render-video-or-frame-sequence-rsmotion-render)
  - [2. Render Single Frame (`rsmotion still`)](#2-render-single-frame-rsmotion-still)
  - [3. Render Image Sequences (`rsmotion frames`)](#3-render-image-sequences-rsmotion-frames)
  - [4. Pure-Rust Rolldown Bundling (`rsmotion bundle`)](#4-pure-rust-rolldown-bundling-rsmotion-bundle)
  - [5. Composition Discovery & Tier Routing (`rsmotion compositions` / `explain`)](#5-composition-discovery--tier-routing-rsmotion-compositions--explain)
  - [6. Audio Utilities (`rsmotion silent-parts` / `extract-audio`)](#6-audio-utilities-rsmotion-silent-parts--extract-audio)
  - [7. Golden SSIM Benchmarking & Diffing (`rsmotion benchmark` / `diff`)](#7-golden-ssim-benchmarking--diffing-rsmotion-benchmark--diff)
- [Drop-In `@rsmotion/renderer` TypeScript API](#drop-in-rsmotionrenderer-typescript-api)
- [Building from Source](#building-from-source)
- [Verification & Known Platform Limitations](#verification--known-platform-limitations)
- [License & Acknowledgments](#license--acknowledgments)

---

## Overview

`rsmotion` is a standalone Rust rendering engine and CLI for **Remotion (`v4.0.545`)** compositions.

Instead of spinning up Chromium processes, serializing compositor screenshots over IPC, and bundling every source edit through Webpack in Node.js, `rsmotion` compiles `.ts`/`.tsx` projects in Rust with `rolldown`, evaluates frames inside an embedded V8 isolate (`deno_core`), solves CSS cascade and Flexbox/Grid layout natively (`rsmotion-css` + `taffy`), rasterizes scenes directly on a headless Skia EGL GPU surface (`skia-safe` + `skottie`), and streams raw RGBA frames straight into `ffmpeg` (VAAPI / QSV / `libx264`). When a composition requires APIs outside Tier 1 (such as WebGL2 or `<iframe>`), `rsmotion-analyze` automatically dispatches only that composition to Chrome CDP (Tier 2).

---

## The Re-Engineering: Headless Chrome + Webpack vs. Native Rust

| Metric / Stage | Stock Remotion (`v4.0.545` + Chromium + Webpack) | `rsmotion` Native Architecture (`v0.1.0`) |
| :--- | :--- | :--- |
| **Bundler** | Node.js + Webpack + Babel/SWC | **Pure-Rust `rolldown 1.3.2` (`oxc` transform & resolver) + `@tailwindcss/cli`** |
| **Cold / Warm Bundle Time** | `32,021 ms` cold | **`329.7 ms` cold (`97.1x`) / `1.88 ms` SHA-256 incremental cache** |
| **JavaScript Execution** | Full multi-process `chrome-headless-shell` tabs | **Embedded V8 (`deno_core 0.413.0`) + synthetic DOM/CSSOM/Canvas 2D/Web Audio host** |
| **CSS & Layout Solver** | Blink Style/Layout engine inside headless tabs | **`rsmotion-css` (`cssparser 0.38.0` cascade/`@layer`/`--var`/`calc`/`oklch`) + `taffy 0.15.1`** |
| **Frame Rasterization** | Chromium compositor screenshot roundtrips | **In-process headless Skia EGL OpenGL ES GPU (`skia-safe 0.157.0`) + CPU fallback** |
| **Canvas 2D & Lottie** | Browser `<canvas>` / `lottie-web` SVG DOM | **Direct GPU Skia `Canvas2D` op buffer + native `skia_safe::skottie`** |
| **Media & Web Fonts** | Chromium media pipeline + headless audio extract | **`symphonia 0.5.5` + `wuff 0.2.10` WOFF/WOFF2 + offline asset cache + FFmpeg decode** |
| **Video Encoding** | Frame PNG/JPEG extraction -> FFmpeg | **Zero-copy RGBA pipe -> `h264_vaapi` / `qsv` / `libx264` / `prores_ks` / `gif` + BT.709/BT.2020** |
| **18-Fixture Suite (1,080f)** | `285.49 s` (`3.78 fps`) | **`31.82 s` (`33.94 fps` overall, up to `81.34 fps` / `26.89x` per fixture)** |
| **Peak Memory (`1,080f`)** | `494.8 MB` RSS (per browser stack) | **`329.7 MiB` Peak RSS (`Swaps: 0`)** |
| **WebGL / `<iframe>` Fallback**| Same heavyweight browser path for everything | **Automatic per-composition Tier 2 Chrome CDP (`rsmotion-cdp`) or Tier 3 (`--force-tier 3`)** |

---

## Architecture Overview

```text
+---------------------------------------------------------------------------------+
|                              rsmotion Single Binary                             |
|                                                                                 |
|  +---------------------------------------------------------------------------+  |
|  |         CLI & Pure-Rust Bundler (rsmotion-cli + rsmotion-bundler)         |  |
|  |   - clap subcommands (`render`, `still`, `frames`, `bundle`, `explain`)   |  |
|  |   - rolldown 1.3.2 auto-bundling (.ts/.tsx/dir) + ~/.cache/rsmotion cache |  |
|  |   - CSS @import inliner, CSS Modules, optional Tailwind CLI, static files |  |
|  +------------------------------------+--------------------------------------+  |
|                                       |                                         |
|                                       v                                         |
|  +---------------------------------------------------------------------------+  |
|  |              Capability Analyzer & 3-Tier Router (rsmotion-analyze)       |  |
|  |   - Static bundle keyword scan + multi-frame V8 dry-run probe             |  |
|  |   - Selects Tier 1 Native vs Tier 2 Chrome CDP vs Tier 3 (@remotion)      |  |
|  +---------------------+-----------------------------------+-----------------+  |
|                        |                                   |                    |
|            Tier 1 (Native, 17/18 fixtures)       Tier 2 (Browser Fallback)      |
|                        |                                   |                    |
|                        v                                   v                    |
|  +---------------------------------------+  +--------------------------------+  |
|  | rsmotion-js (V8 deno_core)            |  | rsmotion-cdp                   |  |
|  |   -> rsmotion-scene (SceneTree IR)    |  |   - tiny_http bundle server    |  |
|  |   -> rsmotion-css (cascade/oklch)     |  |   - WebSocket CDP client       |  |
|  |   -> rsmotion-layout (taffy 0.15.1)   |  |   - SHM / BGRA frame capture   |  |
|  |   -> rsmotion-raster (Skia EGL/Skottie|  +---------------+----------------+  |
|  |   -> rsmotion-media (symphonia/wuff)  |                  |                    |
|  +---------------------+-----------------+                  |                    |
|                        |                                   |                    |
|                        +-----------------+-----------------+                    |
|                                          |                                      |
|                                          v                                      |
|  +---------------------------------------------------------------------------+  |
|  |            Zero-Copy Encode & Audio Pipeline (rsmotion-encode)            |  |
|  |   - Raw RGBA/YUV pipe -> ffmpeg (h264_vaapi / qsv / libx264 / prores/gif) |  |
|  |   - Multi-track audio mix (aac/mp3/opus/pcm-16), BT.709 VUI, PNG/JPEG/WebP|  |
|  +---------------------------------------------------------------------------+  |
+---------------------------------------------------------------------------------+
```

### Workspace & Tech Stack

| Layer | Crate / Path | Key Libraries & Tools |
| :--- | :--- | :--- |
| **CLI & Orchestration** | [`crates/rsmotion-cli`](./crates/rsmotion-cli) | `clap 4.6`, parallel Tier 1 chunk worker, auto-bundling |
| **Bundler & HTML** | [`crates/rsmotion-bundler`](./crates/rsmotion-bundler) | `rolldown 1.3.2`, `sha2 0.10.9`, `@tailwindcss/cli` |
| **Static + V8 Analyzer** | [`crates/rsmotion-analyze`](./crates/rsmotion-analyze) | Static JS scan + V8 dry-run feature detection |
| **JS Isolate & DOM Shim** | [`crates/rsmotion-js`](./crates/rsmotion-js) | `deno_core 0.413.0` (V8), `host.js` synthetic DOM/Canvas/Audio |
| **Scene Graph IR** | [`crates/rsmotion-scene`](./crates/rsmotion-scene) | `SceneTree`, `SceneNode`, `Canvas2DOp`, `LottieSource` |
| **CSS Cascade & Calc** | [`crates/rsmotion-css`](./crates/rsmotion-css) | `cssparser 0.38.0`, `@layer`, `--var()`, `calc()`, `oklch()` |
| **Flex & Grid Layout** | [`crates/rsmotion-layout`](./crates/rsmotion-layout) | `taffy 0.15.1`, replaced-element `aspect-ratio` |
| **Skia GPU & SSIM Diff** | [`crates/rsmotion-raster`](./crates/rsmotion-raster) | `skia-safe 0.157.0` (`gl`/`egl`/`svg`/`skottie`), `khronos-egl 6.0` |
| **Media & Web Fonts** | [`crates/rsmotion-media`](./crates/rsmotion-media) | `symphonia 0.5.5`, `wuff 0.2.10`, `ureq 3.5.2`, `ffmpeg`/`ffprobe` |
| **Encode & Audio Mux** | [`crates/rsmotion-encode`](./crates/rsmotion-encode) | `ffmpeg` pipe (`vaapi`/`qsv`/`libx264`/`prores_ks`), BT.709/BT.2020 |
| **Browser CDP Fallback** | [`crates/rsmotion-cdp`](./crates/rsmotion-cdp) | `tokio-tungstenite 0.30`, `tiny_http 0.12`, shared-memory grab |
| **Node / TS Shim** | [`npm/renderer-shim`](./npm/renderer-shim) | `@rsmotion/renderer` TypeScript bindings delegating to `rsmotion` |
| **Core Types** | [`crates/rsmotion-core`](./crates/rsmotion-core) | Codecs, pixel formats, `FrameRange`, shared config |

---

## Quick Start: Prebuilt Linux Binary (v0.1.0)

```bash
curl -LO https://github.com/ultrapg/rsmotion/releases/download/v0.1.0/rsmotion-linux-x86_64.tar.gz
curl -LO https://github.com/ultrapg/rsmotion/releases/download/v0.1.0/SHA256SUMS.txt
sha256sum -c SHA256SUMS.txt
tar -xzf rsmotion-linux-x86_64.tar.gz
cd rsmotion
./rsmotion --version
```

- **Release:** [`v0.1.0`](https://github.com/ultrapg/rsmotion/releases/tag/v0.1.0)
- **Archive:** `rsmotion-linux-x86_64.tar.gz` (`37,788,262` bytes / `36.04 MiB`)
- **Stripped Binary:** `rsmotion/rsmotion` (`97,646,520` bytes / `93.12 MiB`, `strip --strip-all`)
- **Archive SHA-256:** `010310cab91bfcbe07d743c9610a1ca6e7cb5b8e9daa6dcf4857e976f62967fc`
- **Binary SHA-256:** `fed40f527b027a5af6b8436e1dc40740417976ce8fea230b76088eecea629b6c`

### 1. Render Directly from a TypeScript Entrypoint
```bash
./rsmotion render src/index.ts MyComposition out/video.mp4 \
  --video-bitrate 4M \
  --audio-codec aac \
  --color-space bt709 \
  --overwrite
```

### 2. Render Stills, Sequences, GIF & ProRes
```bash
# Single frame (PNG / JPEG / WebP)
./rsmotion still src/index.ts MyComposition out/frame-30.png --frame 30 --overwrite

# Frame sequence via --sequence alias
./rsmotion render src/index.ts MyComposition out/frames --sequence --frames 0-29 --overwrite

# Looping GIF or ProRes 422 HQ master
./rsmotion render src/index.ts MyComposition out/preview.gif --codec gif --number-of-gif-loops 0 --overwrite
./rsmotion render src/index.ts MyComposition out/master.mov --codec prores --prores-profile hq --overwrite
```

### 3. Inspect Tier Routing & Audio
```bash
./rsmotion explain src/index.ts
./rsmotion compositions src/index.ts --json
./rsmotion silent-parts public/voice.wav --noise-threshold -20 --min-duration 0.5 --json
./rsmotion extract-audio public/input.mp4 out/audio.m4a --overwrite
```

---

## CLI Usage & Command Reference

```text
Usage: rsmotion [OPTIONS] <COMMAND>

Commands:
  spike-encode   Phase 1 spike: benchmark raw RGBA pipe encoding into FFmpeg
  spike-raster   Phase 1 spike: benchmark Skia CPU vs headless EGL GPU rasterization (+ optional encode)
  diff           Compare a PNG or directory of PNGs against golden frames using windowed RGB SSIM
  compositions   List compositions in a Remotion bundle
  still          Render a single frame to PNG, JPEG, or WebP
  render         Render a Remotion composition to video (or frame sequence with `--sequence`)
  frames         Render a frame sequence of images (`element-0000.png`, ...)
  benchmark      Run Tier 1, Tier 2, or automatic dispatch benchmarks against baseline.json and verify golden PNG SSIM
  explain        Explain which rendering tier would be chosen for each composition
  bundle         Bundle a Remotion entrypoint (TypeScript/JS/CSS) into index.html + bundle.js using Rolldown
  silent-parts   Detect silent and audible intervals in an audio or video file via FFmpeg silencedetect
  extract-audio  Extract the audio track from a video file without re-encoding video frames
  help           Print this message or the help of the given subcommand(s)
```

### Options (`rsmotion render`)

| Flag | Default | Description |
| :--- | :---: | :--- |
| `--codec <CODEC>` | `h264` | Video/audio codec: `h264`, `h265`, `vp8`, `vp9`, `prores`, `gif`, `mp3`, `aac`, `wav`. |
| `--crf <CRF>` | *Codec default* | Constant Rate Factor passed to the encoder. |
| `--video-bitrate <RATE>` | *None* | Target video bitrate (e.g. `4M`). Auto-probes VAAPI bitrate support. |
| `--audio-bitrate <RATE>` | *None* | Target audio bitrate (e.g. `192k`). |
| `--audio-codec <CODEC>` | *Codec default* | Audio track encoder: `aac`, `mp3`, `opus`, `pcm-16`. |
| `--color-space <SPACE>` | `default` | Output VUI color space metadata: `default`, `bt709`, `bt2020-ncl`. |
| `--prores-profile <PROF>` | `standard` | ProRes profile: `proxy`, `light`, `standard`, `hq`, `4444`, `4444-xq`. |
| `--number-of-gif-loops <N>` | *None* | Loop count for GIF output (`0` = infinite). |
| `--sequence` | `false` | Emit an image sequence (`renderFrames` mode) instead of a video container. |
| `--image-format <FMT>` | `png` | Sequence image format when `--sequence` is set: `png`, `jpeg`, `webp`. |
| `--jpeg-quality <Q>` | `80` | JPEG compression quality (`0`–`100`). |
| `--concurrency <N>` | `2` | Worker thread/process concurrency. |
| `--frames <RANGE>` | *Full duration* | Single frame (`30`) or inclusive range (`0-59`). |
| `--every-nth-frame <N>` | `1` | Render every N-th frame. |
| `--scale <FACTOR>` | `1` | Resolution scale multiplier. |
| `--muted` | `false` | Omit all audio tracks from output. |
| `--pixel-format <FMT>` | `yuv420p` | Pixel format: `yuv420p`, `yuv444p`, `yuva420p`, `rgb24`, `rgba`. |
| `--raster <BACKEND>` | `auto` | Tier 1 Skia rasterizer backend: `auto`, `gpu`, `cpu`. |
| `--encoder <ENC>` | `auto` | Hardware/software encoder selection: `auto`, `vaapi`, `qsv`, `software`. |
| `--x264-preset <PRESET>` | *None* | Software `libx264` speed/quality preset (e.g. `medium`, `fast`). |
| `--gl <RENDERER>` | `none` | Browser OpenGL mode for Tier 2/3: `angle-egl`, `swangle`, `angle`, `none`. |
| `--props <JSON\|PATH>` | *None* | Input props JSON object or path to JSON file. |
| `--timeout <MS>` | `30000` | Render and `delayRender()` timeout in milliseconds. |
| `--force-tier <1\|2\|3>` | *Auto* | Force Tier 1 (Native), Tier 2 (Chrome CDP), or Tier 3 (Stock `@remotion/renderer`). |
| `--explain` | `false` | Print per-composition tier routing reasoning before rendering. |
| `--json-progress` | `false` | Emit machine-readable JSON progress lines on stdout. |
| `--enable-tailwind` | `false` | Run `@tailwindcss/cli` during automatic source bundling. |
| `--public-dir <DIR>` | `<root>/public` | Override `public/` directory when auto-bundling a source entrypoint. |
| `--no-cache` | `false` | Bypass SHA-256 incremental bundle cache (`~/.cache/rsmotion/bundles`). |
| `--offline` | `false` | Disallow remote network downloads (use cached `~/.cache/rsmotion/assets` only). |
| `--overwrite` | `false` | Overwrite existing output file or directory. |
| `--browser-executable <P>` | *Auto-detect* | Explicit Chromium/Chrome path for Tier 2 or Tier 3 fallback. |

### 1. Render Video or Frame Sequence (`rsmotion render`)
```bash
rsmotion render <SERVE_URL> <COMPOSITION_ID> <OUTPUT> [OPTIONS]
```
`<SERVE_URL>` accepts a TypeScript/JavaScript entrypoint (`.ts`, `.tsx`, `.js`, `.jsx`), a project directory, an already built bundle directory containing `index.html`, or an `http(s)://` URL (automatically routed to Tier 2).

### 2. Render Single Frame (`rsmotion still`)
```bash
rsmotion still tests/fixtures/src/index.ts text-basic out/thumb.png --frame 30 --scale 1 --overwrite
rsmotion still build/ PreviewCard out/card.webp --image-format webp --offline --overwrite
```

### 3. Render Image Sequences (`rsmotion frames`)
```bash
rsmotion frames src/index.ts MyComposition out/seq --frames 0-59 --image-format png --overwrite
```

### 4. Pure-Rust Rolldown Bundling (`rsmotion bundle`)
```bash
rsmotion bundle src/index.ts build --enable-tailwind --json
```

### 5. Composition Discovery & Tier Routing (`rsmotion compositions` / `explain`)
```bash
rsmotion compositions src/index.ts --props '{"title":"Demo"}' --json
rsmotion explain build --composition unsupported-webgl --json
```

### 6. Audio Utilities (`rsmotion silent-parts` / `extract-audio`)
```bash
rsmotion silent-parts sample.wav --noise-threshold -20 --min-duration 0.5 --json
rsmotion extract-audio input.mp4 output.m4a --overwrite --json
```

### 7. Golden SSIM Benchmarking & Diffing (`rsmotion benchmark` / `diff`)
```bash
rsmotion benchmark --tier auto --raster gpu --concurrency 2 --min-ssim 0.98
rsmotion diff tests/golden/text-basic/frame_0030.png out/frame-30.png --min-ssim 0.98
```

---

## Drop-In `@rsmotion/renderer` TypeScript API

[`npm/renderer-shim`](./npm/renderer-shim) provides API parity with `@remotion/renderer` and `bundle()` while delegating bundling, rendering, probing, and audio extraction to the `rsmotion` binary:

```ts
import {
  bundle,
  extractAudio,
  getSilentParts,
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

const silence = await getSilentParts({ src: './public/voice.wav' });
await extractAudio({ src: './out/video.mp4', output: './out/audio.m4a', overwrite: true });
```

---

## Building from Source

### Prerequisites
- **Rust** toolchain (`rustc`, `cargo`, 2021 edition)
- **`ffmpeg`** and **`ffprobe`** available on `PATH`
- **Linux system libraries:** `libstdc++`, `fontconfig`, `freetype`, `libEGL`, `libGL` / X11/XCB (CPU raster + software `libx264` automatically activate when GPU EGL/VAAPI is unavailable)
- **Node.js** (optional: only required for user project `node_modules`, Tailwind CSS CLI, fixture benchmarks, or building [`npm/renderer-shim`](./npm/renderer-shim))

### Build & Test
```bash
git clone https://github.com/ultrapg/rsmotion.git
cd rsmotion

cargo test --workspace
cargo build --release
```

The release binary is output to `./target/release/rsmotion`.

To run the `@rsmotion/renderer` TypeScript shim smoke suite or full 18-fixture benchmarks locally:
```bash
npm --prefix tests/fixtures ci
cd npm/renderer-shim && ../../tests/fixtures/node_modules/.bin/tsc && node test/smoke.mjs
```

---

## Verification & Known Platform Limitations

### Verified on Build Host (2026-11-22)
- `cargo fmt --check` and `cargo clippy --workspace -j2 -- -D warnings`: **0 warnings / passed**.
- `cargo test --workspace -j2`: **43 passed, 0 failed** (1 upstream `deno_core` doc-test ignored) covering all 12 crates (`rsmotion-analyze`, `rsmotion-bundler`, `rsmotion-cli`, `rsmotion-core`, `rsmotion-css`, `rsmotion-encode`, `rsmotion-js`, `rsmotion-layout`, `rsmotion-media`, `rsmotion-raster`, `rsmotion-scene`).
- **E2E Clean-Room Bundler + Dual-Tier Check (`target/release/rsmotion`):**
  - `rsmotion bundle tests/fixtures/src/index.ts ... --no-cache --json`: **`248.9 ms`** (`1,835,271` bytes).
  - `rsmotion explain`: **17 Tier 1 + 1 Tier 2 (`unsupported-webgl`)**.
  - `rsmotion still` (`text-basic`, frame 30) + `diff`: **`SSIM = 0.993743`** (`>= 0.98`).
  - `rsmotion render` (`unsupported-webgl`, Tier 2 Chrome CDP, frames `0-2`): **`1,255.6 ms`**, `ffprobe` verified `1920,1080,3`.
- **Stripped Release Artifact (`rsmotion-linux-x86_64.tar.gz`):** `strip --strip-all` (`93.12 MiB` binary, `36.04 MiB` `.tar.gz`), verified `sha256sum -c SHA256SUMS.txt` (`OK`), `./rsmotion --version`, and Tier 1 golden `SSIM = 0.993743`.
- **`@rsmotion/renderer` JS Shim (`npm/renderer-shim`):** `tsc && node test/smoke.mjs` passed (`ok: true`, `18` Webpack & `18` Rolldown compositions, `text-basic` 60f at **`111.42 fps`**, `progressCalls: 60`, `stillSsim: 0.993743`).

### Platform & Runtime Requirements
- **Linux `x86_64` Only (Tested):** The prebuilt `v0.1.0` binary links dynamically against standard desktop Linux system libraries (`libstdc++.so.6`, `libfontconfig.so.1`, `libfreetype.so.6`, `libEGL.so.1`, `libGL.so.1`) and **`GLIBC_2.43`**. On distributions with older glibc versions (such as Ubuntu 22.04/24.04 or Debian 12), compile locally with `cargo build --release`.
- **External `ffmpeg` / `ffprobe`:** Video/audio decoding fallback, audio mixing, `silent-parts`, and all video encoding require `ffmpeg` and `ffprobe` on `PATH`.
- **macOS & Windows:** Currently untested and not provided as prebuilt binaries.
- **Compatibility Matrix & API Parity:** See [`docs/compat.md`](./docs/compat.md) and [`docs/api-parity.md`](./docs/api-parity.md).

---

## License & Acknowledgments

- **License:** Distributed under the **Apache License, Version 2.0 (`Apache-2.0`)**. See [`LICENSE`](./LICENSE) for full terms.
- **Independent Project:** `rsmotion` is an independent open-source project and is **not** affiliated with, endorsed by, or sponsored by Remotion.
- **Remotion License Notice:** Projects bundled or rendered with `rsmotion` that import `remotion` or `@remotion/*` npm packages remain subject to the official Remotion License for those packages.
- **Third-Party Libraries & Test Assets:** Uses `rolldown`, `deno_core` (V8), `cssparser`, `taffy`, `skia-safe` (Skia/Skottie), `symphonia`, `wuff`, and external `ffmpeg`/`ffprobe` CLI binaries; fixture font `tests/fixtures/public/fonts/SpaceGrotesk-Regular.ttf` is licensed under SIL OFL 1.1.
