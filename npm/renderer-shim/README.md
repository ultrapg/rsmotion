# `@rsmotion/renderer`

Drop-in TypeScript/Node shim matching the `@remotion/renderer` (and `bundle()`) API surface while delegating bundling, rendering, probing, and audio utilities to the native `rsmotion` Rust binary.

## Supported APIs

- `bundle()`
- `getCompositions()` / `selectComposition()`
- `renderMedia()` (including `videoBitrate`, `audioBitrate`, `audioCodec`, `colorSpace`, `proResProfile`, `numberOfGifLoops`, `offline`, `onProgress`, `cancelSignal`)
- `renderStill()` (`png`, `jpeg`, `webp`)
- `renderFrames()`
- `getVideoMetadata()`
- `getSilentParts()`
- `extractAudio()`
- `makeCancelSignal()`
- `validateOutputFilename()` / `validateSelectedPixelFormatAndImageFormatCombination()`

## Build & Smoke Test

Ensure the release binary is built from the repository root (`cargo build --release`), then run:

```bash
../../tests/fixtures/node_modules/.bin/tsc
node test/smoke.mjs
```
