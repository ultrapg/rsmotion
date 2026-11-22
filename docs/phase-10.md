# Phase 10 Report: Pure-Rust Rolldown Bundler, Zero-Config CLI Auto-Bundling & Full API/CLI Parity

## 1. Summary

Phase 10 completes the pure-Rust end-to-end Remotion replacement pipeline by removing Webpack from local bundling and closing remaining `@remotion/renderer` and CLI parity gaps:

1. **Native Rust Bundler (`crates/rsmotion-bundler/`, `rolldown = "1.3.2"`):**
   - Replaces Webpack (`32,021 ms` cold baseline from Phase 0) with `rolldown 1.3.2` (`oxc` parser/transformer + Rust module resolver + IIFE browser bundle writer), cutting cold bundle time on `tests/fixtures/src/index.ts` to **`329.7 ms`** (**`97.1×`** faster than Webpack, generating a `1,837,801`-byte `bundle.js` + `index.html` + `6` static files) and **`1.88 ms`** on warm incremental cache hits under `~/.cache/rsmotion/bundles/<hash>`.
   - Synthesizes a headless Remotion bootstrap entry (`\0rsmotion-setup-env.mjs` -> React 19 `jsx`/`jsxs` runtime -> user entry -> `GetVideoComposition` / `getStaticCompositions` / `remotion_calculateComposition` / `remotion_setBundleMode`), pins singleton aliases (`react`, `react/jsx-runtime`, `react/jsx-dev-runtime`, `react-dom`, `react-dom/client`, `remotion`, `remotion/no-react`, `remotion/version`), emits static media imports to `./assets/`, mirrors/symlinks `public/`, inlines local CSS `@import`s, supports optional `tailwindcss` CLI compilation + caching, and emits a Remotion `v4.0.545` compatible `index.html`.
2. **CLI Auto-Bundling & New Subcommands (`crates/rsmotion-cli/src/main.rs`):**
   - Added `rsmotion bundle <entry> [out_dir]` (`--enable-tailwind`, `--public-dir`, `--no-cache`, `--json`), `rsmotion silent-parts <src>` (`--noise-threshold`, `--min-duration`, `--json`), and `rsmotion extract-audio <src> <output>` (`--overwrite`, `--json`).
   - Added `resolve_serve_url_or_bundle()` across `compositions`, `still`, `render`, `frames`, and `explain`, so passing `.ts`/`.tsx`/`.js`/`.jsx` or a project directory automatically bundles and executes without a separate Webpack step.
   - Added `--video-bitrate`, `--audio-bitrate`, `--audio-codec`, `--color-space`, `--prores-profile`, `--number-of-gif-loops`, `--sequence`, `--image-format`, and `--jpeg-quality` to `rsmotion render` (routing `--sequence` through `run_frames_command`).
3. **Encoder, Audio Muxing & Silence Detection (`rsmotion-core`, `rsmotion-encode`, `rsmotion-media`):**
   - Added `AudioCodec` (`aac`, `mp3`, `opus`, `pcm-16`), `ColorSpace` (`default`, `bt709`, `bt2020-ncl`), and `ProResProfile` (`proxy`, `light`, `standard`, `hq`, `4444`, `4444-xq`).
   - Extended `EncodeOptions`, `AudioMuxOptions`, `mix_and_mux_audio_with_options`, and `extract_audio_track`; added hardware-safe VAAPI color metadata injection via `setparams=...` before `hwupload`, automatic VAAPI bitrate probing with transparent `libx264` fallback on CQP-only VAAPI drivers when `--video-bitrate` is specified with `--encoder auto`, and codec-matched intermediate container extensions with GIF audio-mux skipping.
   - Implemented `crates/rsmotion-media/src/silence.rs` (`get_silent_parts`) using `probe_media` + FFmpeg `silencedetect`.
4. **Full `@remotion/renderer` JS Shim Parity (`npm/renderer-shim/`):**
   - Exported `bundle()`, `getSilentParts()`, and `extractAudio()` alongside TypeScript types (`AudioCodec`, `ColorSpace`, `ProResProfile`, `BundleOptions`, `GetSilentPartsOptions`, `ExtractAudioOptions`).
   - Forwarded `videoBitrate`, `audioBitrate`, `audioCodec`, `colorSpace`, `proResProfile`, `numberOfGifLoops`, and `offline`, and removed `'videoBitrate'`, `'audioBitrate'`, and `'audioCodec'` from `warnUnsupported`.
5. **Workspace `arbitrary_precision` Compatibility Fix (`rsmotion-js`, `rsmotion-scene`):**
   - `rolldown_common` enabled Cargo's `serde_json/arbitrary_precision` feature workspace-wide; updated `op_register_lottie` (`crates/rsmotion-js/src/ops.rs`) to return a typed `LottieMeta` struct and implemented manual `serde::Deserialize` on `CanvasPaint`, `Canvas2DOp`, `NodeKind`, and `SceneNode` (`crates/rsmotion-scene/src/lib.rs`) without `serde::__private::de::Content` buffering.

---

## 2. Measured Performance & Parity Results

All release measurements used `./target/release/rsmotion` on `tests/fixtures/src/index.ts` (18 compositions):

| Check | Command / Target | Measured Result | Target / Baseline |
|---|---|---|---|
| **Cold Rust bundle (`--no-cache`)** | `rsmotion bundle tests/fixtures/src/index.ts /tmp/rs-bundle --no-cache --json` | **`329.74 ms`** (`1,837,801` B, `6` static files, `cache_hit: false`) | Phase 0 Webpack `32,021 ms` (**`97.1×` speedup**, `< 2,000 ms`) |
| **Warm incremental cache hit** | `rsmotion bundle tests/fixtures/src/index.ts /tmp/rs-bundle --json` | **`1.88 ms`** (`cache_hit: true`) | `< 50 ms` |
| **Analyzer parity (`Rolldown` bundle)** | `rsmotion explain /tmp/rs-bundle` | **17 Tier 1 + 1 Tier 2** (`unsupported-webgl`: `canvas-webgl:canvas`) | Matches Webpack bundle (`17` Tier 1 + `1` Tier 2) |
| **Direct `.ts` render (`text-basic`)** | `rsmotion render tests/fixtures/src/index.ts text-basic ... --overwrite` | **`744.4 ms`** (`80.60 fps`, `h264_vaapi`), `ffprobe`: `1920,1080,60` | Direct TS -> MP4 `< 2,000 ms` |
| **Direct `.ts` still + golden SSIM** | `rsmotion still tests/fixtures/src/index.ts text-basic ... --frame 30` | **`160.3 ms`**, **`SSIM = 0.99374`**, `PSNR = 30.45 dB` | `SSIM >= 0.98` |
| **Bitrate + AAC + BT.709 (`video-overlay`)** | `rsmotion render tests/fixtures/src/index.ts video-overlay ... --video-bitrate 4M --audio-codec aac --color-space bt709` | **`4,553.6 ms`** (`13.18 fps`, `libx264` auto-fallback, `has_audio: true`), `ffprobe`: `1920,1080,bt709,60` + `aac` | `1920,1080,60` + `aac` |
| **VAAPI + BT.709 color tags (`text-basic`)** | `rsmotion render tests/fixtures/src/index.ts text-basic ... --frames 0-4 --color-space bt709` | **`408.9 ms`** (`h264_vaapi`), `ffprobe`: `bt709,bt709,bt709` | Valid BT.709 VUI metadata |
| **Sequence alias (`render --sequence`)** | `rsmotion render tests/fixtures/src/index.ts text-basic ... --sequence --frames 0-4` | **`317.2 ms`**, emitted `element-0000.png`..`element-0004.png` (5 files) | Sequence PNG output |
| **GIF & ProRes smoke** | `--codec gif --number-of-gif-loops 1 --frames 0-4` / `--codec prores --prores-profile standard --frames 0-2` | GIF **`615.3 ms`** (`gif`); ProRes **`623.5 ms`** (`prores_ks`, `ffprobe`: `prores`) | Clean encode |
| **`silent-parts` CLI** | `rsmotion silent-parts tests/fixtures/public/sample-audio.wav --json` | `silentParts: [{0.0, 3.0}]`, `audibleParts: []`, `durationInSeconds: 3.0` | Valid JSON schema |
| **`extract-audio` CLI** | `rsmotion extract-audio tests/fixtures/public/sample-video.mp4 ... --json` | **`73.4 ms`**, `27,708` bytes, `ffprobe`: `aac,audio` | Stream-copied / extracted audio |
| **`@rsmotion/renderer` JS shim** | `tsc && node test/smoke.mjs` | `ok: true`, `18` Webpack & `18` Rolldown comps, `stillSsim: 0.993743`, `framesSeqCount: 5`, `silenceDuration: 3`, `progressCalls: 60` | Full JS + TS parity |

---

## 3. Quality Gates & Hygiene

- `cargo fmt --check`: passed
- `cargo clippy --workspace -j2 -- -D warnings`: passed (0 warnings)
- `cargo test --workspace -j2`: **43 passed, 0 failed** (1 upstream `deno_core` doc-test ignored):
  - `rsmotion-analyze`: 3/3 (`analyzes_all_18_fixtures_accurately` across all 18 fixtures)
  - `rsmotion-bundler`: 3/3 (`detects_source_entries_vs_built_bundles`, `renders_compatible_index_html`, `bundles_fixtures_and_hits_incremental_cache`)
  - `rsmotion-cdp`: 0/0
  - `rsmotion-cli`: 2/2
  - `rsmotion-core`: 4/4
  - `rsmotion-css`: 4/4
  - `rsmotion-encode`: 6/6 (`encode_with_bitrate_colorspace_and_extract_audio_track`)
  - `rsmotion-js`: 2/2
  - `rsmotion-layout`: 5/5
  - `rsmotion-media`: 5/5 (`detects_silent_and_audible_parts_in_wav`)
  - `rsmotion-raster`: 8/8
  - `rsmotion-scene`: 1/1 (`scene_tree_roundtrip_serde`)
- `npm/renderer-shim`: `../../tests/fixtures/node_modules/.bin/tsc && node test/smoke.mjs` passed (`ok: true`)
- Process hygiene (`pgrep -ax 'chrome-headless-shell|ffmpeg'`): `CLEAN`
- Git hygiene: `remotion/` clean (`git status --short` empty); `rsmotion` `git remote -v` empty (strictly local)
