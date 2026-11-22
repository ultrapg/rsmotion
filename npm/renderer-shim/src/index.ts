import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import {
  resolveRsmotionBin,
  runRsmotion,
  type CancelCallback,
  type CancelSignal,
  type RenderMediaOnProgress,
  type RenderMediaProgress,
} from './bridge';

export type {CancelSignal, RenderMediaOnProgress, RenderMediaProgress};

export type Codec =
  | 'h264'
  | 'h265'
  | 'vp8'
  | 'vp9'
  | 'prores'
  | 'gif'
  | 'mp3'
  | 'aac'
  | 'wav';

export type PixelFormat =
  | 'yuv420p'
  | 'yuv444p'
  | 'yuva420p'
  | 'rgb24'
  | 'rgba';

export type StillImageFormat = 'png' | 'jpeg' | 'webp';
export type OpenGlRenderer = 'angle-egl' | 'swangle' | 'angle' | null;
export type RasterBackend = 'auto' | 'gpu' | 'cpu';
export type ForceTier = 1 | 2 | 3;
export type AudioCodec = 'aac' | 'mp3' | 'opus' | 'pcm-16';
export type ColorSpace = 'default' | 'bt709' | 'bt2020-ncl';
export type ProResProfile =
  | 'proxy'
  | 'light'
  | 'standard'
  | 'hq'
  | '4444'
  | '4444-xq';

export type BundleOptions = {
  entryPoint: string;
  outDir?: string;
  publicDir?: string | null;
  enableTailwind?: boolean;
  ignoreCache?: boolean;
};

export type GetSilentPartsOptions = {
  src: string;
  noiseThresholdInDecibels?: number;
  minDurationInSeconds?: number;
};

export type SilentPart = {
  startInSeconds: number;
  endInSeconds: number;
};

export type GetSilentPartsResult = {
  silentParts: SilentPart[];
  audibleParts: SilentPart[];
  durationInSeconds: number;
};

export type ExtractAudioOptions = {
  src: string;
  output: string;
  overwrite?: boolean;
};

export type ChromiumOptions = {
  gl?: OpenGlRenderer;
  headless?: boolean;
};

export type VideoConfig = {
  id: string;
  width: number;
  height: number;
  fps: number;
  durationInFrames: number;
  props: Record<string, unknown>;
  defaultProps: Record<string, unknown>;
  defaultCodec: Codec | null;
};

export type GetCompositionsOptions = {
  serveUrl: string;
  inputProps?: Record<string, unknown>;
  timeoutInMilliseconds?: number;
  browserExecutable?: string | null;
  chromiumOptions?: ChromiumOptions;
  logLevel?: string;
  forceTier?: ForceTier;
  offline?: boolean;
};

export type SelectCompositionOptions = GetCompositionsOptions & {
  id: string;
};

export type RenderMediaOptions = {
  composition: VideoConfig;
  serveUrl: string;
  codec?: Codec;
  outputLocation: string | null;
  crf?: number;
  concurrency?: number | string | null;
  frameRange?: number | [number, number] | null;
  everyNthFrame?: number;
  scale?: number;
  muted?: boolean;
  pixelFormat?: PixelFormat;
  imageFormat?: StillImageFormat;
  jpegQuality?: number;
  inputProps?: Record<string, unknown>;
  timeoutInMilliseconds?: number;
  browserExecutable?: string | null;
  chromiumOptions?: ChromiumOptions;
  overwrite?: boolean;
  onProgress?: RenderMediaOnProgress;
  cancelSignal?: CancelSignal;
  x264Preset?: string;
  videoBitrate?: string | null;
  audioBitrate?: string | null;
  audioCodec?: AudioCodec | null;
  colorSpace?: ColorSpace;
  proResProfile?: ProResProfile | null;
  numberOfGifLoops?: number | null;
  logLevel?: string;
  forceTier?: ForceTier;
  raster?: RasterBackend;
  explain?: boolean;
  offline?: boolean;
};

export type RenderMediaResult = {
  buffer: Buffer | null;
  slowestFrames: unknown[];
  durationInMilliseconds: number;
  rsmotionReport: Record<string, unknown>;
};

export type RenderStillOptions = {
  composition: VideoConfig;
  serveUrl: string;
  output: string;
  frame?: number;
  imageFormat?: StillImageFormat;
  jpegQuality?: number;
  scale?: number;
  inputProps?: Record<string, unknown>;
  timeoutInMilliseconds?: number;
  browserExecutable?: string | null;
  chromiumOptions?: ChromiumOptions;
  overwrite?: boolean;
  logLevel?: string;
  forceTier?: ForceTier;
  raster?: RasterBackend;
  explain?: boolean;
  offline?: boolean;
};

export type RenderFramesOptions = {
  composition: VideoConfig;
  serveUrl: string;
  outputDir: string;
  imageFormat?: StillImageFormat;
  jpegQuality?: number;
  frameRange?: number | [number, number] | null;
  everyNthFrame?: number;
  scale?: number;
  inputProps?: Record<string, unknown>;
  timeoutInMilliseconds?: number;
  browserExecutable?: string | null;
  chromiumOptions?: ChromiumOptions;
  overwrite?: boolean;
  onProgress?: RenderMediaOnProgress;
  cancelSignal?: CancelSignal;
  logLevel?: string;
  forceTier?: ForceTier;
  raster?: RasterBackend;
  explain?: boolean;
  offline?: boolean;
};

export type VideoMetadata = {
  width: number;
  height: number;
  fps: number;
  durationInSeconds: number;
  codec: string | null;
  hasAudio: boolean;
};

function warnUnsupported(name: string, options: Record<string, unknown>) {
  const unsupportedKeys = [
    'onBrowserLog',
    'onArtifact',
    'envVariables',
    'ffmpegOverride',
  ];
  for (const k of unsupportedKeys) {
    if (options[k] !== undefined) {
      console.warn(
        `[@rsmotion/renderer] Warning: ${name}() option "${k}" currently falls back or is ignored by rsmotion.`,
      );
    }
  }
}

function mapGl(gl?: OpenGlRenderer): string | null {
  if (gl === undefined) return null;
  if (gl === null) return 'none';
  return gl;
}

function serializeFrameRange(frameRange?: number | [number, number] | null): string | null {
  if (frameRange === undefined || frameRange === null) return null;
  if (typeof frameRange === 'number') return String(frameRange);
  return `${frameRange[0]}-${frameRange[1]}`;
}

export const makeCancelSignal = (): {
  cancelSignal: (callback: CancelCallback) => void;
  cancel: () => void;
} => {
  const callbacks: CancelCallback[] = [];
  let cancelled = false;
  return {
    cancelSignal: (callback: CancelCallback) => {
      callbacks.push(callback);
      if (cancelled) {
        callback();
      }
    },
    cancel: () => {
      if (cancelled) return;
      cancelled = true;
      for (const cb of callbacks) {
        cb();
      }
    },
  };
};

export async function ensureBrowser(): Promise<{path: string; type: string}> {
  // Discovery is handled internally by rsmotion; return resolved binary path info.
  return {
    path: resolveRsmotionBin(),
    type: 'rsmotion-managed',
  };
}

export async function getCompositions(
  serveUrlOrOptions: string | GetCompositionsOptions,
  legacyOptions?: Omit<GetCompositionsOptions, 'serveUrl'>,
): Promise<VideoConfig[]> {
  const opts: GetCompositionsOptions =
    typeof serveUrlOrOptions === 'string'
      ? {...(legacyOptions ?? {}), serveUrl: serveUrlOrOptions}
      : serveUrlOrOptions;

  warnUnsupported('getCompositions', opts as unknown as Record<string, unknown>);

  const args = ['compositions', opts.serveUrl, '--json'];
  if (opts.inputProps) {
    args.push('--props', JSON.stringify(opts.inputProps));
  }
  if (opts.browserExecutable) {
    args.push('--browser-executable', opts.browserExecutable);
  }
  const gl = mapGl(opts.chromiumOptions?.gl);
  if (gl) {
    args.push('--gl', gl);
  }
  if (opts.timeoutInMilliseconds !== undefined) {
    args.push('--timeout', String(opts.timeoutInMilliseconds));
  }
  if (opts.forceTier !== undefined) {
    args.push('--force-tier', String(opts.forceTier));
  }
  if (opts.logLevel) {
    args.push('--log', opts.logLevel);
  }
  if (opts.offline) {
    args.push('--offline');
  }

  return await runRsmotion<VideoConfig[]>(args);
}

export async function selectComposition(
  opts: SelectCompositionOptions,
): Promise<VideoConfig> {
  const list = await getCompositions(opts);
  const found = list.find((c) => c.id === opts.id);
  if (!found) {
    throw new Error(`Could not find composition with ID ${opts.id}`);
  }
  return found;
}

export async function renderStill(opts: RenderStillOptions): Promise<{
  output: string;
  durationInMilliseconds: number;
}> {
  warnUnsupported('renderStill', opts as unknown as Record<string, unknown>);
  const started = Date.now();

  const args = [
    'still',
    opts.serveUrl,
    opts.composition.id,
    opts.output,
    '--frame',
    String(opts.frame ?? 0),
    '--image-format',
    opts.imageFormat ?? 'png',
    '--jpeg-quality',
    String(opts.jpegQuality ?? 80),
    '--scale',
    String(opts.scale ?? 1),
    '--json',
  ];

  if (opts.overwrite ?? true) {
    args.push('--overwrite');
  }
  const props = opts.inputProps ?? opts.composition.props;
  if (props && Object.keys(props).length > 0) {
    args.push('--props', JSON.stringify(props));
  }
  if (opts.timeoutInMilliseconds !== undefined) {
    args.push('--timeout', String(opts.timeoutInMilliseconds));
  }
  if (opts.browserExecutable) {
    args.push('--browser-executable', opts.browserExecutable);
  }
  const gl = mapGl(opts.chromiumOptions?.gl);
  if (gl) {
    args.push('--gl', gl);
  }
  if (opts.raster) {
    args.push('--raster', opts.raster);
  }
  if (opts.forceTier !== undefined) {
    args.push('--force-tier', String(opts.forceTier));
  }
  if (opts.explain) {
    args.push('--explain');
  }
  if (opts.logLevel) {
    args.push('--log', opts.logLevel);
  }
  if (opts.offline) {
    args.push('--offline');
  }

  await runRsmotion(args);
  return {
    output: opts.output,
    durationInMilliseconds: Date.now() - started,
  };
}

export async function renderMedia(
  opts: RenderMediaOptions,
): Promise<RenderMediaResult> {
  warnUnsupported('renderMedia', opts as unknown as Record<string, unknown>);
  if (!opts.outputLocation) {
    throw new Error(
      '[@rsmotion/renderer] renderMedia() requires a non-null outputLocation.',
    );
  }
  validateOutputFilename(opts.outputLocation);
  if (opts.pixelFormat && opts.imageFormat) {
    validateSelectedPixelFormatAndImageFormatCombination(
      opts.pixelFormat,
      opts.imageFormat,
    );
  }

  const started = Date.now();
  const args = [
    'render',
    opts.serveUrl,
    opts.composition.id,
    opts.outputLocation,
    '--codec',
    opts.codec ?? opts.composition.defaultCodec ?? 'h264',
    '--json-progress',
  ];

  if (opts.crf !== undefined) {
    args.push('--crf', String(opts.crf));
  }
  if (opts.concurrency !== undefined && opts.concurrency !== null) {
    args.push('--concurrency', String(opts.concurrency));
  }
  const rangeStr = serializeFrameRange(opts.frameRange);
  if (rangeStr !== null) {
    args.push('--frames', rangeStr);
  }
  if (opts.everyNthFrame !== undefined) {
    args.push('--every-nth-frame', String(opts.everyNthFrame));
  }
  if (opts.scale !== undefined) {
    args.push('--scale', String(opts.scale));
  }
  if (opts.muted) {
    args.push('--muted');
  }
  if (opts.pixelFormat) {
    args.push('--pixel-format', opts.pixelFormat);
  }
  if (opts.x264Preset) {
    args.push('--x264-preset', opts.x264Preset);
  }
  if (opts.videoBitrate) {
    args.push('--video-bitrate', opts.videoBitrate);
  }
  if (opts.audioBitrate) {
    args.push('--audio-bitrate', opts.audioBitrate);
  }
  if (opts.audioCodec) {
    args.push('--audio-codec', opts.audioCodec);
  }
  if (opts.colorSpace) {
    args.push('--color-space', opts.colorSpace);
  }
  if (opts.proResProfile) {
    args.push('--prores-profile', opts.proResProfile);
  }
  if (
    opts.numberOfGifLoops !== undefined &&
    opts.numberOfGifLoops !== null
  ) {
    args.push('--number-of-gif-loops', String(opts.numberOfGifLoops));
  }
  if (opts.overwrite ?? true) {
    args.push('--overwrite');
  }
  const props = opts.inputProps ?? opts.composition.props;
  if (props && Object.keys(props).length > 0) {
    args.push('--props', JSON.stringify(props));
  }
  if (opts.timeoutInMilliseconds !== undefined) {
    args.push('--timeout', String(opts.timeoutInMilliseconds));
  }
  if (opts.browserExecutable) {
    args.push('--browser-executable', opts.browserExecutable);
  }
  const gl = mapGl(opts.chromiumOptions?.gl);
  if (gl) {
    args.push('--gl', gl);
  }
  if (opts.raster) {
    args.push('--raster', opts.raster);
  }
  if (opts.forceTier !== undefined) {
    args.push('--force-tier', String(opts.forceTier));
  }
  if (opts.explain) {
    args.push('--explain');
  }
  if (opts.logLevel) {
    args.push('--log', opts.logLevel);
  }
  if (opts.offline) {
    args.push('--offline');
  }

  const report = await runRsmotion<Record<string, unknown>>(args, {
    onProgress: opts.onProgress,
    cancelSignal: opts.cancelSignal,
  });

  if (!fs.existsSync(opts.outputLocation)) {
    throw new Error(
      `[@rsmotion/renderer] expected output file missing: ${opts.outputLocation}`,
    );
  }

  return {
    buffer: null,
    slowestFrames: [],
    durationInMilliseconds: Date.now() - started,
    rsmotionReport: report ?? {},
  };
}

export async function renderFrames(opts: RenderFramesOptions): Promise<{
  assets: string[];
  assetsInfo: Array<{path: string}>;
  durationInMilliseconds: number;
  rsmotionReport: Record<string, unknown>;
}> {
  warnUnsupported('renderFrames', opts as unknown as Record<string, unknown>);
  const started = Date.now();
  const args = [
    'frames',
    opts.serveUrl,
    opts.composition.id,
    opts.outputDir,
    '--image-format',
    opts.imageFormat ?? 'png',
    '--jpeg-quality',
    String(opts.jpegQuality ?? 80),
    '--json-progress',
  ];

  const rangeStr = serializeFrameRange(opts.frameRange);
  if (rangeStr !== null) {
    args.push('--frames', rangeStr);
  }
  if (opts.everyNthFrame !== undefined) {
    args.push('--every-nth-frame', String(opts.everyNthFrame));
  }
  if (opts.scale !== undefined) {
    args.push('--scale', String(opts.scale));
  }
  if (opts.overwrite ?? true) {
    args.push('--overwrite');
  }
  const props = opts.inputProps ?? opts.composition.props;
  if (props && Object.keys(props).length > 0) {
    args.push('--props', JSON.stringify(props));
  }
  if (opts.timeoutInMilliseconds !== undefined) {
    args.push('--timeout', String(opts.timeoutInMilliseconds));
  }
  if (opts.browserExecutable) {
    args.push('--browser-executable', opts.browserExecutable);
  }
  const gl = mapGl(opts.chromiumOptions?.gl);
  if (gl) {
    args.push('--gl', gl);
  }
  if (opts.raster) {
    args.push('--raster', opts.raster);
  }
  if (opts.forceTier !== undefined) {
    args.push('--force-tier', String(opts.forceTier));
  }
  if (opts.explain) {
    args.push('--explain');
  }
  if (opts.logLevel) {
    args.push('--log', opts.logLevel);
  }
  if (opts.offline) {
    args.push('--offline');
  }

  const report = await runRsmotion<Record<string, unknown>>(args, {
    onProgress: opts.onProgress,
    cancelSignal: opts.cancelSignal,
  });
  const assets = Array.isArray(report?.assets)
    ? (report.assets as string[])
    : [];
  return {
    assets,
    assetsInfo: assets.map((p) => ({path: p})),
    durationInMilliseconds: Date.now() - started,
    rsmotionReport: report ?? {},
  };
}

export async function bundle(options: BundleOptions): Promise<string> {
  const args = ['bundle', options.entryPoint, '--json'];
  if (options.outDir) {
    args.splice(2, 0, options.outDir);
  }
  if (options.enableTailwind) {
    args.push('--enable-tailwind');
  }
  if (options.publicDir) {
    args.push('--public-dir', options.publicDir);
  }
  if (options.ignoreCache) {
    args.push('--no-cache');
  }
  const report = await runRsmotion<{out_dir: string}>(args);
  if (!report?.out_dir) {
    throw new Error('[@rsmotion/renderer] bundle() did not return out_dir.');
  }
  return report.out_dir;
}

export async function getSilentParts(
  options: GetSilentPartsOptions,
): Promise<GetSilentPartsResult> {
  const args = ['silent-parts', options.src, '--json'];
  if (options.noiseThresholdInDecibels !== undefined) {
    args.push('--noise-threshold', String(options.noiseThresholdInDecibels));
  }
  if (options.minDurationInSeconds !== undefined) {
    args.push('--min-duration', String(options.minDurationInSeconds));
  }
  return await runRsmotion<GetSilentPartsResult>(args);
}

export async function extractAudio(
  options: ExtractAudioOptions,
): Promise<void> {
  const args = ['extract-audio', options.src, options.output, '--json'];
  if (options.overwrite ?? true) {
    args.push('--overwrite');
  }
  await runRsmotion(args);
}

export function getVideoMetadata(src: string): VideoMetadata {
  const out = execFileSync(
    'ffprobe',
    [
      '-v',
      'error',
      '-show_entries',
      'format=duration:stream=codec_type,codec_name,width,height,r_frame_rate',
      '-of',
      'json',
      src,
    ],
    {encoding: 'utf8'},
  );
  const parsed = JSON.parse(out) as {
    streams?: Array<Record<string, unknown>>;
    format?: {duration?: string};
  };
  const streams = parsed.streams ?? [];
  const vStream = streams.find((s) => s.codec_type === 'video');
  const aStream = streams.find((s) => s.codec_type === 'audio');
  const rateRaw = String(vStream?.r_frame_rate ?? '30/1');
  let fps = 30;
  if (rateRaw.includes('/')) {
    const [num, den] = rateRaw.split('/').map(Number);
    if (den > 0 && num > 0) fps = num / den;
  } else if (Number(rateRaw) > 0) {
    fps = Number(rateRaw);
  }
  const durationInSeconds = Number(
    vStream?.duration ?? parsed.format?.duration ?? 0,
  );
  return {
    width: Number(vStream?.width ?? 0),
    height: Number(vStream?.height ?? 0),
    fps,
    durationInSeconds,
    codec: vStream?.codec_name ? String(vStream.codec_name) : null,
    hasAudio: Boolean(aStream),
  };
}

export function validateOutputFilename(outputLocation: string): void {
  if (!outputLocation || outputLocation.trim() === '') {
    throw new Error('outputLocation must be a non-empty file path.');
  }
  if (outputLocation.endsWith('/') || outputLocation.endsWith('\\')) {
    throw new Error(`outputLocation cannot be a directory: ${outputLocation}`);
  }
}

export function validateSelectedPixelFormatAndImageFormatCombination(
  pixelFormat: PixelFormat,
  imageFormat: StillImageFormat,
): void {
  if (pixelFormat === 'yuva420p' && imageFormat !== 'png') {
    throw new Error(
      `pixelFormat "${pixelFormat}" requires imageFormat "png" (got "${imageFormat}").`,
    );
  }
}

export const RenderInternals = {
  internalRenderMedia: renderMedia,
  internalRenderStill: renderStill,
  internalRenderFrames: renderFrames,
  internalGetCompositions: getCompositions,
  internalSelectComposition: selectComposition,
  internalEnsureBrowser: ensureBrowser,
};
