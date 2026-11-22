import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

export const TextBasic: React.FC = () => {
	const frame = useCurrentFrame();
	const opacity = interpolate(frame, [0, 15, 45, 59], [0, 1, 1, 0.3], {
		extrapolateLeft: 'clamp',
		extrapolateRight: 'clamp',
	});

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#0f172a',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div
				style={{
					display: 'flex',
					flexDirection: 'column',
					alignItems: 'center',
					gap: 24,
					opacity,
				}}
			>
				<h1
					style={{
						margin: 0,
						fontSize: 96,
						fontWeight: 700,
						color: '#f8fafc',
						letterSpacing: -2,
					}}
				>
					rsmotion Renderer
				</h1>
				<p
					style={{
						margin: 0,
						fontSize: 40,
						color: '#38bdf8',
					}}
				>
					Native Rust Drop-in Replacement • Frame {frame}
				</p>
			</div>
		</AbsoluteFill>
	);
};
