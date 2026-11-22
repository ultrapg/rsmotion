import React from 'react';
import {
	AbsoluteFill,
	OffthreadVideo,
	interpolate,
	staticFile,
	useCurrentFrame,
} from 'remotion';

export const VideoOverlay: React.FC = () => {
	const frame = useCurrentFrame();
	const badgeX = interpolate(frame, [0, 20], [-200, 60], {
		extrapolateRight: 'clamp',
	});

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#000000',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<OffthreadVideo
				src={staticFile('sample-video.mp4')}
				style={{
					width: '100%',
					height: '100%',
					objectFit: 'cover',
				}}
			/>
			<div
				style={{
					position: 'absolute',
					top: 60,
					left: badgeX,
					backgroundColor: 'rgba(15, 23, 42, 0.88)',
					color: '#f8fafc',
					padding: '20px 36px',
					borderRadius: 18,
					border: '3px solid #38bdf8',
					fontSize: 38,
					fontWeight: 700,
				}}
			>
				LIVE OVERLAY • Frame {frame}
			</div>
		</AbsoluteFill>
	);
};
