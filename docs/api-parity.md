# `rsmotion` API Parity Map (Target: Remotion v4.0.545)

Derived from `remotion` v4.0.545 (`packages/renderer/src/index.ts`, `packages/renderer/src/options/options-map.ts`, `packages/bundler/src/bundle.ts`, `packages/cli/src/parsed-cli.ts`), and `rsmotion` Phase 1–10 implementation (`crates/rsmotion-bundler`, `crates/rsmotion-cli`, `crates/rsmotion-core`, `crates/rsmotion-encode`, `crates/rsmotion-media`, `npm/renderer-shim`).

Status legend: `todo` | `partial` | `done` | `fallback` (delegated to Tier 2 Chrome CDP or Remotion Tier 3)

## 1. Public Functions (`@remotion/renderer` & `@remotion/bundler` compatibility)

| Symbol / Flag | Implemented Phase | Status | Notes |
|---|---|---|---|
| `bundle` | Phase 10 | done | Exported from `@rsmotion/renderer` & `rsmotion bundle`; pure-Rust `rolldown 1.3.2` bundler with SHA-256 incremental cache, singleton React/Remotion aliasing, CSS/Tailwind, and `public/` mirroring |
| `renderMedia` | Phase 2–10 | done | Auto Tier 1 -> Tier 2 CDP -> `--force-tier 3`, auto-bundles `.ts`/`.tsx`/dirs, supports `videoBitrate`, `audioBitrate`, `audioCodec`, `colorSpace`, `proResProfile`, `numberOfGifLoops`, `offline`, `onProgress`, `cancelSignal`, `frameRange`, `everyNthFrame`, `scale`, `muted`, `pixelFormat`, `x264Preset`, `logLevel`, `forceTier`, `raster`, `explain` |
| `renderStill` | Phase 2–10 | done | PNG / JPEG / WebP stills with auto-bundling, `--offline`, `--scale`, `--jpeg-quality`, `--props`, auto Tier 1 -> Tier 2 dispatch, and `--force-tier 3` |
| `renderFrames` | Phase 6 / 10 | done | Implemented via `rsmotion frames` (with auto-bundling & `--offline`) returning `{ assets, assetsInfo, durationInMilliseconds, rsmotionReport }` with `onProgress` and `cancelSignal` |
| `selectComposition` | Phase 2–10 | done | Resolves composition metadata + `calculateMetadata()` from bundles or source entrypoints |
| `getCompositions` | Phase 2–10 | done | Calls `getStaticCompositions()` (`rsmotion compositions --json`) supporting `(serveUrl, opts)` & `({ serveUrl, ... })`, auto-bundling, and `--offline` |
| `openBrowser` | Phase 2 | partial | Internal `BrowserProcess::launch` managed transparently by `rsmotion-cdp` for Tier 2 |
| `ensureBrowser` | Phase 2 / 6 | done | Resolves `.remotion/chrome-headless-shell` / system Chrome inside `rsmotion-cdp` and exports `ensureBrowser()` in `@rsmotion/renderer` |
| `stitchFramesToVideo` | Phase 1 / 2 | partial | FFmpeg frame/concat muxing inside `rsmotion-encode` (`concat.rs` / `pipe.rs`) |
| `combineChunks` | Phase 1 / 2 | partial | `concat_chunks` implemented in `rsmotion-encode` |
| `extractAudio` | Phase 2 / 10 | done | Exported in `@rsmotion/renderer` and exposed via `rsmotion extract-audio <src> <out>` backed by `rsmotion_encode::extract_audio_track` |
| `getVideoMetadata` | Phase 4 / 6 | done | Exported in `@rsmotion/renderer` (`ffprobe` -> `{ width, height, fps, durationInSeconds, codec, hasAudio }`) and backed natively by `rsmotion-media::probe_media` |
| `getSilentParts` | Phase 10 | done | Exported in `@rsmotion/renderer` and exposed via `rsmotion silent-parts <src>` backed by `rsmotion_media::get_silent_parts` (`silencedetect`) |
| `makeCancelSignal` | Phase 2 / 6 | done | Implemented in `@rsmotion/renderer` shim; cleanly terminates spawned `rsmotion` subprocesses (`SIGTERM` -> `SIGKILL`) |
| `validateOutputFilename` | Phase 1 / 6 | done | Enforced in `rsmotion-cli` (`ensure_writable`) and exported from `@rsmotion/renderer` |
| `validateSelectedPixelFormatAndImageFormatCombination` | Phase 1 / 6 | done | Validated in `@rsmotion/renderer` before spawning `renderMedia()` |

## 2. Key `RenderInternals` Symbols (used by `@remotion/cli`)

| Symbol / Flag | Implemented Phase | Status | Notes |
|---|---|---|---|
| `RenderInternals.internalRenderMedia` | Phase 2 / 6 | done | Exported by `@rsmotion/renderer` (`renderMedia`) |
| `RenderInternals.internalRenderStill` | Phase 2 / 6 | done | Exported by `@rsmotion/renderer` (`renderStill`) |
| `RenderInternals.internalRenderFrames` | Phase 6 | done | Exported by `@rsmotion/renderer` (`renderFrames`) |
| `RenderInternals.internalGetCompositions` | Phase 2 / 6 | done | Exported by `@rsmotion/renderer` (`getCompositions`) |
| `RenderInternals.internalSelectComposition` | Phase 2 / 6 | done | Exported by `@rsmotion/renderer` (`selectComposition`) |
| `RenderInternals.internalEnsureBrowser` | Phase 2 / 6 | done | Exported by `@rsmotion/renderer` (`ensureBrowser`) |
| `RenderInternals.internalCombineChunks` | Phase 1 / 2 | partial | Rust chunk concatenation in `rsmotion-encode/src/concat.rs` |
| `RenderInternals.resolveConcurrency` | Phase 2 / 7 | done | Default `2` (clamped safely for 7.6 GiB / 4-core host across Tier 1 & Tier 2 workers) |
| `RenderInternals.getRealFrameRange` / `getRealFrameRanges` | Phase 1 | done | `FrameRange` parsing & stride resolution in `rsmotion-core` |
| `RenderInternals.validateEvenDimensionsWithCodec` | Phase 1 | done | `validate_even_dimensions` in `rsmotion-core` |
| `RenderInternals.probeSharedMemoryCapture` | Phase 2 | done | Auto-probed via `Page.remotionCreateFramePool` (`shm-bgra` with `png-fallback`) in Tier 2 |
| `RenderInternals.serveStatic` / `prepareServer` | Phase 2 | done | Embedded `tiny_http` static + HTTP `Range` + `/proxy` server in `rsmotion-cdp` |
| `RenderInternals.callFf` | Phase 1 / 2 / 10 | done | Direct FFmpeg stdin pipe (`yuv.rs` + `pipe.rs`), `mix_and_mux_audio_with_options`, `extract_audio_track`, and `get_silent_parts` |
| `RenderInternals.canUseParallelEncoding` | Phase 1 / 7 | done | Chunked parallel V8 + GPU workers in Tier 1 (`--concurrency 2`) and multi-tab Chrome CDP in Tier 2 |

## 3. CLI Subcommands & Flags (`rsmotion` & `@remotion/cli` parity)

| Subcommand / Flag | Implemented Phase | Status | Notes |
|---|---|---|---|
| `rsmotion bundle <entry> [out_dir]` | Phase 10 | done | Native Rust Rolldown bundler (`--enable-tailwind`, `--public-dir`, `--no-cache`, `--json`) |
| `rsmotion render <serveUrl\|entry> <compId> <out>` | Phase 2–10 | done | Auto-bundles source entrypoints, auto-dispatches Tier 1 -> Tier 2 CDP (or `--force-tier 1\|2\|3`), supports `--sequence` |
| `rsmotion still <serveUrl\|entry> <compId> <out>` | Phase 2–10 | done | Auto-bundles source entrypoints, PNG / JPEG / WebP single-frame render |
| `rsmotion frames <serveUrl\|entry> <compId> <dir>` | Phase 6 / 10 | done | Auto-bundles source entrypoints, emits numbered image sequence (`element-0000.png`...) |
| `rsmotion compositions <serveUrl\|entry>` | Phase 2–10 | done | Auto-bundles source entrypoints, evaluates compositions via Tier 1, Tier 2 fallback, or `--force-tier 1\|2\|3` |
| `rsmotion explain <serveUrl\|entry>` | Phase 5 / 10 | done | Auto-bundles source entrypoints + static scan & 3-frame V8 probe (`--json` supported) |
| `rsmotion silent-parts <src>` | Phase 10 | done | FFmpeg `silencedetect` (`--noise-threshold`, `--min-duration`, `--json`) |
| `rsmotion extract-audio <src> <out>` | Phase 10 | done | Audio stream extraction (`--overwrite`, `--json`) |
| `rsmotion benchmark` | Phase 2–9 | done | `--tier auto\|1\|2`, `--raster auto\|gpu\|cpu`, verifies `ffprobe` + windowed RGB SSIM against `tests/golden/` and `bench/baseline.json` |
| `rsmotion diff <golden> <actual>` | Phase 1 | done | Windowed RGB SSIM + PSNR + max channel diff + optional heatmap PNG |
| `--codec` | Phase 1 / 2 | done | `h264`, `h265`, `vp8`, `vp9`, `prores`, `gif`, `mp3`, `aac`, `wav` |
| `--crf` | Phase 1 / 2 | done | Validated per codec (`crf_range`) and mapped to `-crf` / VAAPI `-qp` (omitted when `--video-bitrate` is specified without `--crf`) |
| `--video-bitrate` / `--audio-bitrate` | Phase 10 | done | Passed to FFmpeg (`-b:v` / `-b:a`) with automatic `libx264` fallback on CQP-only VAAPI drivers under `--encoder auto` |
| `--audio-codec` | Phase 10 | done | `aac`, `mp3`, `opus`, `pcm-16` |
| `--color-space` | Phase 10 | done | `default`, `bt709`, `bt2020-ncl` (via `setparams` before `hwupload` on VAAPI or FFmpeg output flags) |
| `--prores-profile` | Phase 10 | done | `proxy`, `light`, `standard`, `hq` (`yuv422p10le`) and `4444`, `4444-xq` (`yuva444p10le`) |
| `--number-of-gif-loops` | Phase 10 | done | `-loop <n>` on FFmpeg `gif` muxer (supports negative values like `-1`) |
| `--sequence` | Phase 10 | done | Alias on `rsmotion render` delegating to image sequence output (`--image-format`, `--jpeg-quality`) |
| `--enable-tailwind` / `--public-dir` / `--no-cache` | Phase 10 | done | Supported on `bundle` and auto-bundling in `render`, `still`, `frames`, `compositions`, `explain` |
| `--offline` | Phase 9 / 10 | done | Global flag disallowing remote HTTP asset/stylesheet/font downloads across CLI and JS shim |
| `--concurrency` / `--frames` / `--every-nth-frame` | Phase 1–7 | done | Worker count, frame range (`start-end` or single frame), and stride |
| `--props` / `--image-format` / `--jpeg-quality` / `--scale` / `--muted` / `--pixel-format` | Phase 1–6 | done | Full parity across Tier 1/2/3 (`yuva420p` valid with PNG sequences) |
| `--gl` / `--timeout` / `--log` / `--overwrite` / `--x264-preset` / `--encoder` | Phase 1–6 | done | Full parity (`RUST_LOG` overrides `--log`) |
| `--explain` / `--force-tier` / `--raster` | Phase 1–6 | done | `rsmotion` tier introspection and explicit dispatch controls |
