# Remotion 4.0.545 Baseline Benchmark

- **Date:** 2026-11-22T00:09:18.796Z
- **Node:** v26.5.0
- **Browser:** `tests/fixtures/node_modules/.remotion/chrome-headless-shell/linux64/chrome-headless-shell-linux64/chrome-headless-shell` (local-puppeteer-browser)
- **Concurrency:** 2
- **One-time Bundle Time:** 39159.2 ms (`tests/fixtures/build/`)
- **Keyframes (Golden PNGs):** 0, 15, 30, 45, 59

| Fixture | Frames | Render (ms) | FPS | ms/Frame | Peak RSS (MB) | MP4 Size (KB) |
|---|---:|---:|---:|---:|---:|---:|
| text-basic | 60 | 7140.8 | 8.40 | 119.01 | 494.8 | 49.0 |
| text-animated | 60 | 10293.8 | 5.83 | 171.56 | 493 | 106.3 |
| captions | 60 | 8102.2 | 7.41 | 135.04 | 472.5 | 75.4 |
| image-slideshow | 60 | 12781.8 | 4.69 | 213.03 | 491.3 | 547.7 |
| transforms-opacity | 60 | 8171.0 | 7.34 | 136.18 | 479.7 | 129.7 |
| gradients-shadows | 60 | 9444.4 | 6.35 | 157.41 | 472.4 | 376.0 |
| svg-shapes | 60 | 7667.9 | 7.82 | 127.80 | 478.7 | 91.6 |
| chart-bars | 60 | 10511.2 | 5.71 | 175.19 | 481.1 | 47.1 |
| video-overlay | 60 | 94427.3 | 0.64 | 1573.79 | 455.3 | 1097.7 |
| audio-mix | 60 | 40376.3 | 1.49 | 672.94 | 463.4 | 150.6 |
| fonts-custom | 60 | 8832.7 | 6.79 | 147.21 | 472.8 | 50.4 |
| css-tailwind-grid | 60 | 8782.3 | 6.83 | 146.37 | 477.5 | 107.0 |
| css-effects-3d | 60 | 16546.6 | 3.63 | 275.78 | 473.1 | 299.6 |
| unsupported-3d | 60 | 9998.2 | 6.00 | 166.64 | 480 | 124.5 |
| canvas-2d | 60 | 10197.6 | 5.88 | 169.96 | 480.6 | 304.3 |
| lottie-native | 60 | 6531.8 | 9.19 | 108.86 | 479.8 | 179.9 |
| audio-viz | 60 | 7577.4 | 7.92 | 126.29 | 491.6 | 213.5 |
| unsupported-webgl | 60 | 8109.3 | 7.40 | 135.16 | 480 | 241.5 |
| **Total / Avg** | **1080** | **285492.6** | **3.78** | **264.35** | **494.8** | - |
