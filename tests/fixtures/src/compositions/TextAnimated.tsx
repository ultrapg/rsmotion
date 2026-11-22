import React from 'react';
import {
	AbsoluteFill,
	interpolate,
	spring,
	useCurrentFrame,
	useVideoConfig,
} from 'remotion';

const WORDS = ['Fast', 'Native', 'Rust', 'Motion', 'Engine'];

export const TextAnimated: React.FC = () => {
	const frame = useCurrentFrame();
	const {fps} = useVideoConfig();

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#111827',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div
				style={{
					display: 'flex',
					flexDirection: 'row',
					flexWrap: 'wrap',
					gap: 32,
					justifyContent: 'center',
					padding: 80,
				}}
			>
				{WORDS.map((word, idx) => {
					const progress = spring({
						fps,
						frame: frame - idx * 6,
						config: {damping: 12, stiffness: 140},
					});
					const translateY = interpolate(progress, [0, 1], [80, 0]);
					const scale = interpolate(progress, [0, 1], [0.7, 1]);
					return (
						<span
							key={word}
							style={{
								display: 'inline-block',
								fontSize: 88,
								fontWeight: 700,
								color: idx % 2 === 0 ? '#f9fafb' : '#fbbf24',
								opacity: progress,
								transform: `translateY(${translateY}px) scale(${scale})`,
							}}
						>
							{word}
						</span>
					);
				})}
			</div>
		</AbsoluteFill>
	);
};
