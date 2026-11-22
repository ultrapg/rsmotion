import React from 'react';
import {
	AbsoluteFill,
	Audio,
	interpolate,
	staticFile,
	useCurrentFrame,
} from 'remotion';

export const AudioMix: React.FC = () => {
	const frame = useCurrentFrame();
	const level = interpolate(frame, [0, 30, 59], [0.2, 1.0, 0.2]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#09090b',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<Audio
				src={staticFile('sample-audio.wav')}
				volume={(f) => interpolate(f, [0, 30, 59], [0.2, 1.0, 0.2])}
			/>
			<div
				style={{
					display: 'flex',
					flexDirection: 'row',
					alignItems: 'flex-end',
					gap: 24,
					height: 400,
				}}
			>
				{Array.from({length: 12}).map((_, idx) => {
					const wave = Math.abs(Math.sin((frame + idx * 3) * 0.2));
					const barHeight = 60 + wave * 300 * level;
					return (
						<div
							key={idx}
							style={{
								width: 48,
								height: barHeight,
								borderRadius: 12,
								backgroundColor: '#22c55e',
							}}
						/>
					);
				})}
			</div>
			<div
				style={{
					marginTop: 48,
					fontSize: 40,
					color: '#e4e4e7',
				}}
			>
				Audio Gain: {level.toFixed(2)} • Frame {frame}
			</div>
		</AbsoluteFill>
	);
};
