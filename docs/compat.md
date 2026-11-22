# `rsmotion` Compatibility Matrix (Remotion `v4.0.545`)

Dieses Dokument beschreibt die gemessene und statisch geprüfte Kompatibilität von `rsmotion` gegen das lokale Remotion-Upstream-Repo (`4.0.545`), alle **18 Benchmark-Fixtures** sowie **6 Upstream-Templates**.

---

## 1. Erweiterte 18-Fixture-Suite (`rsmotion benchmark --tier auto --raster gpu`)

Alle 18 Kompositionen (`1920×1080`, 30 fps, je 60 Frames = **`1,080` Frames** gesamt) wurden über die automatische Tier-1/Tier-2-Erkennung gegen die 90 Golden-PNG-Keyframes (`0, 15, 30, 45, 59`) und die Remotion-`4.0.545`-Baseline (`285,492.6 ms` = `3.78 fps`) in Phase 9 verifiziert (`totalRenderMs = 31,822.5 ms`, Peak RSS `329.7 MiB`, `Swaps: 0`), plus direkter Rolldown-Auto-Bundle-Check in Phase 10 (`rsmotion explain /tmp/rs-bundle` → identisch **17× Tier 1 / 1× Tier 2**, `text-basic` direkt aus `src/index.ts` mit **`80.60 fps`** und **`0.99374` SSIM**):

| Fixture | Gewählter Tier | Remotion FPS | `rsmotion` Auto FPS | Speedup | Min SSIM | Avg SSIM | Audio (AAC) |
|---|---|---:|---:|---:|---:|---:|---|
| `text-basic` | Tier 1 (`tier1-native`) | 8.40 | 55.76 | 6.64× | 0.99374 | 0.99529 | - |
| `text-animated` | Tier 1 (`tier1-native`) | 5.83 | 58.96 | 10.11× | 0.99624 | 0.99706 | - |
| `captions` | Tier 1 (`tier1-native`) | 7.41 | 70.30 | 9.49× | 0.98981 | 0.98996 | - |
| `image-slideshow` | Tier 1 (`tier1-native`) | 4.69 | 41.99 | 8.94× | 0.99735 | 0.99833 | - |
| `transforms-opacity` | Tier 1 (`tier1-native`) | 7.34 | 62.52 | 8.51× | 0.99983 | 0.99991 | - |
| `gradients-shadows` | Tier 1 (`tier1-native`) | 6.35 | 45.75 | 7.20× | 0.98608 | 0.98801 | - |
| `svg-shapes` | Tier 1 (`tier1-native`) | 7.82 | 31.17 | 3.98× | 0.99967 | 0.99967 | - |
| `chart-bars` | Tier 1 (`tier1-native`) | 5.71 | 31.42 | 5.50× | 0.99897 | 0.99914 | - |
| `video-overlay` | Tier 1 (`tier1-native`) | 0.64 | 17.09 | **26.89×** | 0.99870 | 0.99884 | verifiziert |
| `audio-mix` | Tier 1 (`tier1-native`) | 1.49 | 24.23 | **16.31×** | 0.99824 | 0.99848 | verifiziert |
| `fonts-custom` | Tier 1 (`tier1-native`) | 6.79 | 63.12 | 9.29× | 0.98455 | 0.98459 | - |
| `css-tailwind-grid` | Tier 1 (`tier1-native`) | 6.83 | 32.59 | 4.77× | 0.99069 | 0.99081 | - |
| `css-effects-3d` | Tier 1 (`tier1-native`) | 3.63 | 33.49 | 9.24× | 0.99446 | 0.99479 | - |
| `unsupported-3d` | Tier 1 (`tier1-native`) | 6.00 | 54.66 | 9.11× | 0.99591 | 0.99594 | - |
| `canvas-2d` | Tier 1 (`tier1-native`) | 5.88 | 54.62 | 9.28× | 0.98499 | 0.98541 | - |
| `lottie-native` | Tier 1 (`tier1-native`) | 9.19 | **81.34** | 8.85× | 0.99923 | 0.99925 | - |
| `audio-viz` | Tier 1 (`tier1-native`) | 7.92 | 42.58 | 5.38× | 0.99940 | 0.99946 | verifiziert |
| `unsupported-webgl` | Tier 2 (`tier2-cdp`) | 7.40 | 9.34 | 1.26× | 1.00000 | 1.00000 | - |
| **Gesamt** | **17× Tier 1 / 1× Tier 2** | **3.78** | **33.94** | **8.97×** | **0.98455** | — | **3/3** |

---

## 2. Live-getestete Upstream-Templates (`remotion/packages/template-*`)

Alle **6 / 6** getesteten Remotion-`4.0.545`-Template-Kompositionen routen nach Phase 8–9 nativ auf **Tier 1** und übertreffen `SSIM >= 0.96` gegen Tier 3 (`@remotion/renderer --force-tier 3`):

| Template (`Komposition`) | Spec | Gewählter Tier | T1 Still (ms) | T3 Still (ms) | Still SSIM vs Tier 3 | Short MP4 (15f) FPS | `ffprobe` / `--offline` |
|---|---|---|---:|---:|---:|---:|---|
| `template-blank` (`MyComp`) | `1280x720@30` (60f) | Tier 1 | 1,547.9 | 1,828.8 | **1.0000** | 8.06 | `1280,720,15` |
| `template-helloworld` (`HelloWorld`) | `1920x1080@30` (150f) | Tier 1 | 297.6 | 1,520.8 | **0.9614** | 20.47 | `1920,1080,15` |
| `template-still` (`PreviewCard`) | `1200x627@1` (1f) | Tier 1 | 231.1 | 1,107.8 | **0.9931** | — | `1200,627` |
| `template-overlay` (`Overlay`) | `1920x1080@30` (75f) | Tier 1 | 265.5 | 996.1 | **0.9980** | 18.31 | `1920,1080,15`; `--offline` |
| `template-audiogram` (`Audiogram`) | `1080x1080@30` (814f) | Tier 1 | 198.0 | 1,618.0 | **0.9913** | 29.85 | `1080,1080,15`; `--offline` |
| `template-music-visualization` (`Visualizer`) | `1080x1080@30` (7165f) | Tier 1 | 577.7 | 3,438.5 | **0.9733** | 6.31 | `1080,1080,15` |

---

## 3. Statisch geprüfte weitere Upstream-Templates (`remotion/packages/`)

| Package | Relevante Dependencies / APIs | Erwartetes Tier-Routing | Begründung |
|---|---|---|---|
| `template-tiktok` | `<OffthreadVideo>`, `@remotion/media-utils`, `@remotion/layout-utils`, `@remotion/google-fonts` | **Tier 1** *(oder Tier 2 bei `fillTextBox`/Sonder-CSS)* | `<OffthreadVideo>`, Google Fonts (WOFF2 + `ureq`-Cache) und Basistext laufen nativ in Tier 1. |
| `template-next-app-tailwind` | `tailwindcss` v4, `@remotion/tailwind-v4` | **Tier 1** (`rsmotion-bundler` + `rsmotion-css`) | `rsmotion bundle --enable-tailwind` bzw. automatische Tailwind-Erkennung kompiliert Utility-Klassen für `rsmotion-css`; Next.js-App-Routen werden ggf. via Chrome/Tier 2 gehostet. |
| `template-code-hike` | `@code-hike/lighter`, `codehike`, `twoslash-cdn` | **Tier 1 / Tier 2** | Reine Token-Spans laufen in Tier 1; etwaige nicht-unterstützte CSS-Features fallen automatisch auf Tier 2 zurück. |
| `template-skia` | `@remotion/skia` (`<SkiaCanvas>`), `@shopify/react-native-skia` (CanvasKit WASM) | **Tier 2** | `@remotion/skia` nutzt `<canvas>` mit WebGL/WASM-Backend -> `canvas-webgl` routet nach Tier 2. |
| `template-three` | `@remotion/three` (`<ThreeCanvas>`), `three`, `@react-three/fiber` | **Tier 2** | `static_scan` / V8 `canvas-webgl` erkennt WebGLRenderer und routet sauber nach Tier 2 Chrome CDP. |
| `packages/example` | Gemischte Showcase-Kompositionen (`@remotion/three`, `@remotion/lottie`, CSS Grid, `<Audio>`, `<Canvas>`) | **Pro Komposition Tier 1 oder Tier 2** | Reine Flex/Grid/CSS-Effekte/Canvas-2D/Lottie/Audio-Comps nutzen Tier 1; WebGL/3D-Canvas-Comps routen gezielt auf Tier 2. |

---

## 4. Bundler- & Renderer-Fähigkeiten vs. Tier-2-Fallback

### Reines Rust Bundling (`rsmotion-bundler` mit `rolldown 1.3.2`)
- **Zero-Config-Entrypoints:** Bundelt `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.mts`, `.cts` und Projektordner direkt (`rsmotion bundle` sowie transparentes Auto-Bundling in `render`, `still`, `frames`, `compositions`, `explain`) in **`329.7 ms`** kalt (`97.1×` schneller als Phase-0-Webpack) und **`1.88 ms`** aus dem SHA-256-Cache (`~/.cache/rsmotion/bundles/<hash>`).
- **CSS & Assets:** Injiziert lokale CSS-Dateien (inkl. relativer `@import`s & CSS-Modules), optionales `@tailwindcss/cli` mit Cache (`--enable-tailwind`), emittiert statische Asset-Imports nach `./assets/` und spiegelt/symlinked `public/`.

### Nativ in Tier 1 (`V8` + `rsmotion-css` + `Taffy 0.15.1` + `Skia EGL GPU` + `rsmotion-media` + `FFmpeg`)
- **React & Remotion Core:** React 19, `useCurrentFrame`, `useVideoConfig`, `interpolate`, `spring`, `<Sequence>`, `<AbsoluteFill>`, `<Img>`, `staticFile`, `delayRender`/`continueRender`, `calculateMetadata`, deterministische Timers/`requestAnimationFrame`.
- **CSS-Engine & Layout (`rsmotion-css` + `rsmotion-layout`):** Selektor-Matching (Klassen, IDs, Attribute, Pseudo-Klassen inkl. `:nth-child`, Kombinatoren), `@layer`-Kaskade, Custom Properties (`--var()`), `calc()`, `oklch()` / `color-mix()`, `display: flex|inline-flex|grid|inline-grid|block|none` (`grid-template-columns`/`rows`/`auto-flow`, `repeat()`), `position`, `aspect-ratio` (inkl. ersetzter Elemente), `box-sizing`, `z-index`, `overflow`.
- **Visuelles Styling & Effekte (`rsmotion-raster` GPU/CPU):** Farbflächen & Gradients, `border*`, `border-radius`, `box-shadow`, `text-shadow`, `opacity`, 2D/3D-`transform`s inkl. `perspective`/`rotateX/Y/3d`, `filter` (`blur`, `brightness`, `contrast`, `drop-shadow`, ...), `backdrop-filter`, `mix-blend-mode`, `clip-path`, `background-clip: text`, `object-fit`/`object-position`.
- **Typografie & Webfonts (`rsmotion-media` + `rsmotion-css`):** System- & Custom-Fonts inkl. pure-Rust `wuff` WOFF/WOFF2-Dekodierung, Google-Fonts-CSS + Caching (`~/.cache/rsmotion/assets/`, respektiert `--offline`), Variable Fonts (`wght`), `@font-face`-Deduplizierung, `white-space: pre|nowrap`.
- **Vektorgrafik, Canvas 2D & Lottie:** Inline-`<svg>` inkl. Verläufen/`<clipPath>`/`<text>`, `<canvas>` 2D (`CanvasRenderingContext2D`, `Path2D`, Gradients, `measureText`), und `@remotion/lottie` (`skia_safe::skottie`).
- **Video & Audio (`rsmotion-media` + `rsmotion-encode`):** `<OffthreadVideo>`/`<Video>`, mehrspuriges `<Audio>` (`aac`/`mp3`/`opus`/`pcm-16`), `symphonia` + FFmpeg Audio-Decoding für `@remotion/media-utils` (`getAudioData`, `useWindowedAudioData`, `visualizeAudio`), `getSilentParts` (`silencedetect`), `extractAudio`, ProRes-Profile, BT.709/BT.2020-ncl Color-Space-Tags, Video-/Audio-Bitrates und GIF-Loops.

### Automatische Tier-2-Fallback-Trigger (`host.js` + `rsmotion-analyze`)
- Elemente & APIs: `<canvas getContext("webgl"|"webgl2"|"bitmaprenderer")>`, `<iframe>`, SVG `<foreignObject>`, Remote-`http(s)`-`<video>` oder Remote-`serveUrl`.
- Stylesheets & Sonderfälle: Nicht auflösbare Remote-`<link rel="stylesheet">` im `--offline`-Modus oder Laufzeit-Escape-Hatch (`LateUnsupportedFeatureError`) -> nahtloser Fallback auf Tier 2 Chrome CDP (`--force-tier 3` bleibt weiterhin für Stock-`@remotion/renderer` verfügbar).
