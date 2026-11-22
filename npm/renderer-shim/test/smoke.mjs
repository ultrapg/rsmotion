import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {
  bundle,
  extractAudio,
  getCompositions,
  getSilentParts,
  getVideoMetadata,
  makeCancelSignal,
  RenderInternals,
  renderFrames,
  renderMedia,
  renderStill,
  selectComposition,
  validateOutputFilename,
  validateSelectedPixelFormatAndImageFormatCombination,
} from '../dist/index.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../../..');
const BUNDLE_DIR = path.join(REPO_ROOT, 'tests', 'fixtures', 'build');
const SAMPLE_VIDEO = path.join(
  REPO_ROOT,
  'tests',
  'fixtures',
  'public',
  'sample-video.mp4',
);
const GOLDEN_FRAME = path.join(
  REPO_ROOT,
  'tests',
  'golden',
  'text-basic',
  'frame_0030.png',
);
const RSMOTION_BIN = path.join(REPO_ROOT, 'target', 'release', 'rsmotion');
const OUT_DIR = '/tmp/rsmotion-shim-smoke';

fs.mkdirSync(OUT_DIR, {recursive: true});

const meta = getVideoMetadata(SAMPLE_VIDEO);
assert.equal(meta.width, 640);
assert.equal(meta.height, 360);
assert.equal(Math.round(meta.fps), 30);
assert.equal(meta.hasAudio, true);

validateOutputFilename(path.join(OUT_DIR, 'ok.mp4'));
assert.throws(() => validateOutputFilename(''));
validateSelectedPixelFormatAndImageFormatCombination('yuva420p', 'png');
assert.throws(() =>
  validateSelectedPixelFormatAndImageFormatCombination('yuva420p', 'jpeg'),
);
assert.equal(typeof RenderInternals.internalRenderFrames, 'function');

const comps = await getCompositions({serveUrl: BUNDLE_DIR});
assert.equal(comps.length, 18, `expected 18 compositions, got ${comps.length}`);

const comp = await selectComposition({serveUrl: BUNDLE_DIR, id: 'text-basic'});
assert.equal(comp.width, 1920);
assert.equal(comp.height, 1080);

const stillPath = path.join(OUT_DIR, 'text-basic-30.png');
await renderStill({
  composition: comp,
  serveUrl: BUNDLE_DIR,
  frame: 30,
  output: stillPath,
  overwrite: true,
});
assert.ok(fs.existsSync(stillPath), 'still PNG must exist');

const jpegPath = path.join(OUT_DIR, 'text-basic-30.jpeg');
await renderStill({
  composition: comp,
  serveUrl: BUNDLE_DIR,
  frame: 30,
  imageFormat: 'jpeg',
  jpegQuality: 85,
  scale: 0.5,
  output: jpegPath,
  overwrite: true,
});
assert.ok(fs.existsSync(jpegPath) && fs.statSync(jpegPath).size > 1000);

const webpPath = path.join(OUT_DIR, 'text-basic-30.webp');
await renderStill({
  composition: comp,
  serveUrl: BUNDLE_DIR,
  frame: 30,
  imageFormat: 'webp',
  output: webpPath,
  overwrite: true,
});
assert.ok(fs.existsSync(webpPath) && fs.statSync(webpPath).size > 1000);

const diffOut = execFileSync(
  RSMOTION_BIN,
  ['diff', GOLDEN_FRAME, stillPath, '--min-ssim', '0.98'],
  {encoding: 'utf8'},
);
const diffReport = JSON.parse(diffOut);
assert.ok(diffReport.ssim >= 0.98, `SSIM too low: ${diffReport.ssim}`);

const framesDir = path.join(OUT_DIR, 'frames-seq');
fs.rmSync(framesDir, {recursive: true, force: true});
const framesRes = await renderFrames({
  composition: comp,
  serveUrl: BUNDLE_DIR,
  outputDir: framesDir,
  frameRange: [0, 4],
  overwrite: true,
});
assert.equal(framesRes.assets.length, 5);
for (const asset of framesRes.assets) {
  assert.ok(fs.existsSync(asset), `missing rendered frame ${asset}`);
}

const {cancelSignal, cancel} = makeCancelSignal();
assert.equal(typeof cancelSignal, 'function');
assert.equal(typeof cancel, 'function');

let progressCalls = 0;
let lastProgress = 0;
const mp4Path = path.join(OUT_DIR, 'text-basic.mp4');
const mediaRes = await renderMedia({
  composition: comp,
  serveUrl: BUNDLE_DIR,
  codec: 'h264',
  outputLocation: mp4Path,
  concurrency: 2,
  overwrite: true,
  cancelSignal,
  onProgress: (p) => {
    progressCalls++;
    lastProgress = p.progress;
  },
});

assert.ok(progressCalls > 0, 'expected onProgress callbacks');
assert.equal(lastProgress, 1);
const probe = execFileSync(
  'ffprobe',
  [
    '-v',
    'error',
    '-select_streams',
    'v:0',
    '-count_frames',
    '-show_entries',
    'stream=width,height,nb_read_frames',
    '-of',
    'csv=p=0',
    mp4Path,
  ],
  {encoding: 'utf8'},
).trim();
assert.equal(probe, '1920,1080,60', `unexpected ffprobe output: ${probe}`);

// Auto-tier fallback through @rsmotion/renderer on unsupported-webgl (WebGL canvas).
const u3dComp = await selectComposition({
  serveUrl: BUNDLE_DIR,
  id: 'unsupported-webgl',
});
const u3dPath = path.join(OUT_DIR, 'unsupported-webgl-subrange.mp4');
await renderMedia({
  composition: u3dComp,
  serveUrl: BUNDLE_DIR,
  codec: 'h264',
  frameRange: [0, 4],
  muted: true,
  outputLocation: u3dPath,
  overwrite: true,
});
const u3dProbe = execFileSync(
  'ffprobe',
  [
    '-v',
    'error',
    '-select_streams',
    'v:0',
    '-count_frames',
    '-show_entries',
    'stream=width,height,nb_read_frames',
    '-of',
    'csv=p=0',
    u3dPath,
  ],
  {encoding: 'utf8'},
).trim();
assert.equal(u3dProbe, '1920,1080,5', `unexpected u3d ffprobe: ${u3dProbe}`);

// Phase 10 checks: bundle(), getSilentParts(), extractAudio(), and renderMedia() bitrate/audioCodec flags.
const entrySrc = path.join(REPO_ROOT, 'tests', 'fixtures', 'src', 'index.ts');
const bundledDir = await bundle({entryPoint: entrySrc});
assert.ok(
  fs.existsSync(path.join(bundledDir, 'index.html')),
  'bundle() missing index.html',
);
assert.ok(
  fs.existsSync(path.join(bundledDir, 'bundle.js')),
  'bundle() missing bundle.js',
);
const bundledComps = await getCompositions({serveUrl: bundledDir});
assert.equal(
  bundledComps.length,
  comps.length,
  'Rolldown bundle should match Webpack composition count',
);

const vidComp = await selectComposition({
  serveUrl: bundledDir,
  id: 'video-overlay',
});
const bitrateMp4 = path.join(OUT_DIR, 'video-overlay-bitrate.mp4');
await renderMedia({
  composition: vidComp,
  serveUrl: bundledDir,
  codec: 'h264',
  frameRange: [0, 14],
  videoBitrate: '4M',
  audioCodec: 'aac',
  colorSpace: 'bt709',
  outputLocation: bitrateMp4,
  overwrite: true,
});
const vidAudioCodec = execFileSync(
  'ffprobe',
  [
    '-v',
    'error',
    '-select_streams',
    'a:0',
    '-show_entries',
    'stream=codec_name',
    '-of',
    'csv=p=0',
    bitrateMp4,
  ],
  {encoding: 'utf8'},
).trim();
assert.equal(vidAudioCodec, 'aac', `expected aac audio stream, got ${vidAudioCodec}`);

const silenceRes = await getSilentParts({
  src: path.join(REPO_ROOT, 'tests', 'fixtures', 'public', 'sample-audio.wav'),
});
assert.ok(
  silenceRes.durationInSeconds > 0,
  `expected positive audio duration, got ${silenceRes.durationInSeconds}`,
);
assert.ok(Array.isArray(silenceRes.silentParts));
assert.ok(Array.isArray(silenceRes.audibleParts));

const extractedAudio = path.join(OUT_DIR, 'extracted-audio.m4a');
await extractAudio({
  src: SAMPLE_VIDEO,
  output: extractedAudio,
  overwrite: true,
});
assert.ok(fs.existsSync(extractedAudio) && fs.statSync(extractedAudio).size > 0);
const extractedStream = execFileSync(
  'ffprobe',
  [
    '-v',
    'error',
    '-select_streams',
    'a:0',
    '-show_entries',
    'stream=codec_type',
    '-of',
    'csv=p=0',
    extractedAudio,
  ],
  {encoding: 'utf8'},
).trim();
assert.equal(extractedStream, 'audio');

console.log(
  JSON.stringify(
    {
      ok: true,
      compositionsCount: comps.length,
      bundledCompositionsCount: bundledComps.length,
      videoMeta: meta,
      stillSsim: diffReport.ssim,
      framesSeqCount: framesRes.assets.length,
      silenceDuration: silenceRes.durationInSeconds,
      progressCalls,
      durationInMilliseconds: mediaRes.durationInMilliseconds,
      rsmotionReport: mediaRes.rsmotionReport,
    },
    null,
    2,
  ),
);
