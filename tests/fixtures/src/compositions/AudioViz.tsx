import {
	createSmoothSvgPath,
	useAudioData,
	visualizeAudio,
	visualizeAudioWaveform,
} from '@remotion/media-utils';
import React from 'react';
import {
	AbsoluteFill,
	Audio,
	staticFile,
	useCurrentFrame,
	useVideoConfig,
} from 'remotion';

export const AudioViz: React.FC = () => {
	const frame = useCurrentFrame();
	const {fps} = useVideoConfig();
	const audioSrc = staticFile('sample-audio.wav');
	const audioData = useAudioData(audioSrc);

	if (!audioData) {
		return <AbsoluteFill style={{backgroundColor: '#09090b'}} />;
	}

	const frequencies = visualizeAudio({
		fps,
		frame,
		audioData,
		numberOfSamples: 32,
		optimizeFor: 'speed',
	});

	const waveform = visualizeAudioWaveform({
		fps,
		frame,
		audioData,
		numberOfSamples: 96,
		windowInSeconds: 0.25,
		channel: 0,
	});

	const wavePath = createSmoothSvgPath({
		points: waveform.map((sample, idx) => ({
			x: 160 + (idx / Math.max(1, waveform.length - 1)) * 1600,
			y: 790 + sample * 140,
		})),
	});

	const bars = frequencies.slice(0, 16);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#09090b',
				fontFamily: '"DejaVu Sans", sans-serif',
				padding: 80,
				justifyContent: 'space-between',
			}}
		>
			<Audio src={audioSrc} />
			<div style={{color: '#f4f4f5', fontSize: 48, fontWeight: 700}}>
				Audio Viz • Frame {frame}
			</div>
			<div
				style={{
					display: 'flex',
					flexDirection: 'row',
					alignItems: 'flex-end',
					justifyContent: 'center',
					gap: 24,
					height: 360,
				}}
			>
				{bars.map((amp, i) => {
					const h = 36 + Math.min(1, amp * 4) * 320;
					return (
						<div
							key={i}
							style={{
								width: 56,
								height: h,
								borderRadius: 12,
								backgroundColor: '#38bdf8',
							}}
						/>
					);
				})}
			</div>
			<svg width={1760} height={280} viewBox="0 650 1760 280">
				<path
					d={wavePath}
					fill="none"
					stroke="#22c55e"
					strokeWidth={6}
					strokeLinecap="round"
				/>
			</svg>
		</AbsoluteFill>
	);
};
