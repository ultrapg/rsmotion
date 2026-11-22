import {execFileSync, spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {bundle} from '@remotion/bundler';
import {
	ensureBrowser,
	openBrowser,
	renderMedia,
	renderStill,
	selectComposition,
} from '@remotion/renderer';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURES_DIR = path.resolve(__dirname, '..');
const RSMOTION_ROOT = path.resolve(FIXTURES_DIR, '../..');
const BUILD_DIR = path.join(FIXTURES_DIR, 'build');
const GOLDEN_DIR = path.join(RSMOTION_ROOT, 'tests', 'golden');
const BENCH_DIR = path.join(RSMOTION_ROOT, 'bench');
const TMP_OUT_DIR = '/tmp/rsmotion-baseline';

const FIXTURE_IDS = [
	'text-basic',
	'text-animated',
	'captions',
	'image-slideshow',
	'transforms-opacity',
	'gradients-shadows',
	'svg-shapes',
	'chart-bars',
	'video-overlay',
	'audio-mix',
	'fonts-custom',
	'css-tailwind-grid',
	'css-effects-3d',
	'unsupported-3d',
	'canvas-2d',
	'lottie-native',
	'audio-viz',
	'unsupported-webgl',
];

const KEYFRAMES = [0, 15, 30, 45, 59];
const CONCURRENCY = 2;

function parseArgs(argv) {
	const args = {};
	for (let i = 0; i < argv.length; i++) {
		const k = argv[i];
		if (k.startsWith('--')) {
			const name = k.slice(2);
			const next = argv[i + 1];
			if (next && !next.startsWith('--')) {
				args[name] = next;
				i++;
			} else {
				args[name] = true;
			}
		}
	}
	return args;
}

async function runWorker(compId, serveUrl) {
	fs.mkdirSync(TMP_OUT_DIR, {recursive: true});
	const outMp4 = path.join(TMP_OUT_DIR, `${compId}.mp4`);
	const goldenCompDir = path.join(GOLDEN_DIR, compId);
	fs.mkdirSync(goldenCompDir, {recursive: true});

	const composition = await selectComposition({
		serveUrl,
		id: compId,
		logLevel: 'warn',
	});

	const t0 = performance.now();
	await renderMedia({
		composition,
		serveUrl,
		codec: 'h264',
		outputLocation: outMp4,
		concurrency: CONCURRENCY,
		overwrite: true,
		logLevel: 'warn',
	});
	const renderMediaMs = performance.now() - t0;

	// Render 5 golden PNG frames using one shared browser instance to save RAM/time.
	const browser = await openBrowser('chrome-headless-shell', {
		logLevel: 'warn',
	});
	try {
		for (const frame of KEYFRAMES) {
			const padded = String(frame).padStart(4, '0');
			const outPng = path.join(goldenCompDir, `frame_${padded}.png`);
			await renderStill({
				composition,
				serveUrl,
				frame,
				imageFormat: 'png',
				output: outPng,
				puppeteerInstance: browser,
				overwrite: true,
				logLevel: 'warn',
			});
		}
	} finally {
		await browser.close({silent: true});
	}

	const probeJson = execFileSync(
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
			'json',
			outMp4,
		],
		{encoding: 'utf8'},
	);
	const probe = JSON.parse(probeJson);
	const stream = probe.streams?.[0] ?? {};
	const stat = fs.statSync(outMp4);

	const result = {
		id: compId,
		frames: composition.durationInFrames,
		width: Number(stream.width ?? composition.width),
		height: Number(stream.height ?? composition.height),
		encodedFrames: Number(stream.nb_read_frames ?? 0),
		renderMediaMs: Number(renderMediaMs.toFixed(1)),
		fps: Number((composition.durationInFrames / (renderMediaMs / 1000)).toFixed(2)),
		msPerFrame: Number((renderMediaMs / composition.durationInFrames).toFixed(2)),
		outputBytes: stat.size,
	};

	console.log(`__RSMOTION_RESULT__:${JSON.stringify(result)}`);
}

async function runCoordinator() {
	fs.mkdirSync(BENCH_DIR, {recursive: true});
	fs.mkdirSync(GOLDEN_DIR, {recursive: true});
	fs.mkdirSync(TMP_OUT_DIR, {recursive: true});

	console.log('Ensuring Remotion browser...');
	const browserInfo = await ensureBrowser({logLevel: 'info'});
	console.log('Browser:', browserInfo);

	console.log('Bundling fixtures once...');
	const bundleStart = performance.now();
	fs.rmSync(BUILD_DIR, {recursive: true, force: true});
	const serveUrl = await bundle({
		entryPoint: path.join(FIXTURES_DIR, 'src', 'index.ts'),
		outDir: BUILD_DIR,
	});
	const bundleMs = Number((performance.now() - bundleStart).toFixed(1));
	console.log(`Bundled into ${serveUrl} in ${bundleMs} ms`);

	const results = [];
	for (const compId of FIXTURE_IDS) {
		console.log(`\n--- Rendering baseline fixture: ${compId} ---`);
		const proc = spawnSync(
			'/usr/bin/time',
			[
				'-v',
				process.execPath,
				__filename,
				'--worker',
				compId,
				'--serve-url',
				serveUrl,
			],
			{
				cwd: FIXTURES_DIR,
				encoding: 'utf8',
				maxBuffer: 10 * 1024 * 1024,
			},
		);

		if (proc.status !== 0) {
			console.error(proc.stdout);
			console.error(proc.stderr);
			throw new Error(`Worker failed for ${compId} (exit ${proc.status})`);
		}

		const resultLine = proc.stdout
			.split('\n')
			.find((line) => line.startsWith('__RSMOTION_RESULT__:'));
		if (!resultLine) {
			console.error(proc.stdout);
			console.error(proc.stderr);
			throw new Error(`Missing result JSON for ${compId}`);
		}
		const row = JSON.parse(resultLine.replace('__RSMOTION_RESULT__:', ''));

		const rssMatch = proc.stderr.match(
			/Maximum resident set size \(kbytes\):\s*(\d+)/,
		);
		const peakRssMb = rssMatch
			? Number((Number(rssMatch[1]) / 1024).toFixed(1))
			: null;

		row.peakRssMb = peakRssMb;
		results.push(row);
		console.log(
			`${compId}: ${row.renderMediaMs} ms (${row.fps} fps, ${row.msPerFrame} ms/frame, peak RSS ${peakRssMb} MB, frames=${row.encodedFrames})`,
		);
	}

	const totalMs = results.reduce((acc, r) => acc + r.renderMediaMs, 0);
	const totalFrames = results.reduce((acc, r) => acc + r.frames, 0);
	const avgFps = Number((totalFrames / (totalMs / 1000)).toFixed(2));
	const maxRssMb = Math.max(...results.map((r) => r.peakRssMb ?? 0));

	const summary = {
		generatedAt: new Date().toISOString(),
		remotionVersion: '4.0.545',
		nodeVersion: process.version,
		concurrency: CONCURRENCY,
		browserType: browserInfo.type,
		browserPath: browserInfo.path ?? null,
		bundleMs,
		keyframes: KEYFRAMES,
		totals: {
			fixtures: results.length,
			frames: totalFrames,
			renderMediaMs: Number(totalMs.toFixed(1)),
			avgFps,
			maxRssMb,
		},
		results,
	};

	fs.writeFileSync(
		path.join(BENCH_DIR, 'baseline.json'),
		JSON.stringify(summary, null, 2) + '\n',
	);

	const lines = [
		'# Remotion 4.0.545 Baseline Benchmark',
		'',
		`- **Date:** ${summary.generatedAt}`,
		`- **Node:** ${summary.nodeVersion}`,
		`- **Browser:** \`${summary.browserPath}\` (${summary.browserType})`,
		`- **Concurrency:** ${CONCURRENCY}`,
		`- **One-time Bundle Time:** ${bundleMs} ms (\`tests/fixtures/build/\`)`,
		`- **Keyframes (Golden PNGs):** ${KEYFRAMES.join(', ')}`,
		'',
		'| Fixture | Frames | Render (ms) | FPS | ms/Frame | Peak RSS (MB) | MP4 Size (KB) |',
		'|---|---:|---:|---:|---:|---:|---:|',
	];

	for (const r of results) {
		lines.push(
			`| ${r.id} | ${r.encodedFrames} | ${r.renderMediaMs.toFixed(1)} | ${r.fps.toFixed(2)} | ${r.msPerFrame.toFixed(2)} | ${r.peakRssMb ?? '-'} | ${(r.outputBytes / 1024).toFixed(1)} |`,
		);
	}

	lines.push(
		`| **Total / Avg** | **${totalFrames}** | **${totalMs.toFixed(1)}** | **${avgFps.toFixed(2)}** | **${(totalMs / totalFrames).toFixed(2)}** | **${maxRssMb}** | - |`,
		'',
	);

	fs.writeFileSync(path.join(BENCH_DIR, 'baseline.md'), lines.join('\n'));
	console.log(`\nWrote ${path.join(BENCH_DIR, 'baseline.md')} and baseline.json`);
}

const args = parseArgs(process.argv.slice(2));
if (args.worker) {
	await runWorker(args.worker, args['serve-url']);
} else {
	await runCoordinator();
}
