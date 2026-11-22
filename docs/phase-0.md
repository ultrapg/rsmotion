# Phase 0 Report: Workspace, API Parity Map, Fixtures & Remotion Baseline

## 1. Summary

Phase 0 established the `rsmotion` local repository, mapped the `@remotion/renderer` v4.0.545 public API and CLI surface, built 12 self-contained 1080p30 Remotion fixtures with deterministic local media/fonts, captured 60 golden PNG keyframes (`0, 15, 30, 45, 59`), and measured Remotion's baseline render speed on the target host.

## 2. API Parity Coverage

Tracked in [api-parity.md](api-parity.md):
- **Public `@remotion/renderer` functions mapped:** 15 (`renderMedia`, `renderStill`, `renderFrames`, `selectComposition`, `getCompositions`, `openBrowser`, `ensureBrowser`, `stitchFramesToVideo`, `combineChunks`, `extractAudio`, `getVideoMetadata`, `getSilentParts`, `makeCancelSignal`, `validateOutputFilename`, `validateSelectedPixelFormatAndImageFormatCombination`).
- **`RenderInternals` symbols mapped:** 15 symbols used by `@remotion/cli`.
- **CLI flags mapped:** 26 Remotion flags plus 3 `rsmotion`-specific flags (`--explain`, `--force-tier`, `--raster`).

## 3. Fixtures Created (`tests/fixtures/`)

All 12 compositions are 1920×1080, 30 fps, 60 frames (2 seconds), bundled once into `tests/fixtures/build/`:

1. `text-basic`: Centered Flexbox text + `interpolate()` opacity
2. `text-animated`: Per-word `spring()` + `interpolate()` slide & scale
3. `captions`: Word-by-word active pill highlight subtitle bar
4. `image-slideshow`: Dual `<Sequence>` `<Img>` with `objectFit: 'cover'` cross-fade
5. `transforms-opacity`: Nested `translate`, `rotate`, `scale`, `skewX`, `overflow: hidden`
6. `gradients-shadows`: `radial-gradient`, `linear-gradient`, `boxShadow`, `textShadow`
7. `svg-shapes`: `<svg>` with `rect`, `ellipse`, `line`, `polygon`, `path`, `<linearGradient>`, `<clipPath>`, stroke dash animation
8. `chart-bars`: Animated SVG bar chart with gridlines, axes, `spring()` heights, and labels
9. `video-overlay`: `<OffthreadVideo>` with animated text overlay badge
10. `audio-mix`: `<Audio>` with per-frame `volume` callback + animated level bars
11. `fonts-custom`: `FontFace` loading `SpaceGrotesk-Regular.ttf` gated by `delayRender`/`continueRender`
12. `unsupported-3d`: Fallback trigger fixture (`<canvas>` 2D + `backdropFilter: blur(12px)` + `mixBlendMode: difference`)

## 4. Measured Remotion 4.0.545 Baseline

- **Host:** Linux x86_64, 4 CPU cores, 7.6 GiB RAM, Intel Iris 540
- **Node:** `v26.5.0`
- **Browser:** Remotion `chrome-headless-shell` (`158.0.8127.2`)
- **Concurrency:** `2`
- **One-time Webpack bundle time:** `32,021 ms`
- **Golden verification:** 12 MP4s (`1920x1080`, 60 frames each) and 60 PNG keyframes (`1920x1080`) verified via `ffprobe`.

| Fixture | Frames | Render (ms) | FPS | ms/Frame | Peak RSS (MB) | MP4 Size (KB) |
|---|---:|---:|---:|---:|---:|---:|
| text-basic | 60 | 5868.4 | 10.22 | 97.81 | 492.7 | 49.0 |
| text-animated | 60 | 5971.1 | 10.05 | 99.52 | 490.3 | 106.3 |
| captions | 60 | 6790.3 | 8.84 | 113.17 | 472.5 | 75.4 |
| image-slideshow | 60 | 7447.5 | 8.06 | 124.13 | 493.6 | 547.7 |
| transforms-opacity | 60 | 6783.2 | 8.85 | 113.05 | 482.1 | 129.7 |
| gradients-shadows | 60 | 9568.4 | 6.27 | 159.47 | 471.9 | 376.0 |
| svg-shapes | 60 | 6179.8 | 9.71 | 103.00 | 478.4 | 91.6 |
| chart-bars | 60 | 6088.2 | 9.86 | 101.47 | 491.5 | 47.1 |
| video-overlay | 60 | 10499.6 | 5.71 | 174.99 | 494.3 | 1119.8 |
| audio-mix | 60 | 4877.4 | 12.30 | 81.29 | 480.4 | 150.6 |
| fonts-custom | 60 | 4782.5 | 12.55 | 79.71 | 472.7 | 50.4 |
| unsupported-3d | 60 | 7096.1 | 8.46 | 118.27 | 479.6 | 124.5 |
| **Total / Avg** | **720** | **81952.5** | **8.79** | **113.82** | **494.3** | - |
