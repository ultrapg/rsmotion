import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

export const TransformsOpacity: React.FC = () => {
	const frame = useCurrentFrame();
	const angle = interpolate(frame, [0, 59], [0, 180]);
	const scale = interpolate(frame, [0, 30, 59], [0.85, 1.25, 0.9]);
	const skewX = interpolate(frame, [0, 59], [-12, 12]);
	const tx = interpolate(frame, [0, 59], [-180, 180]);
	const opacity = interpolate(frame, [0, 30, 59], [0.45, 1, 0.6]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#09090b',
				justifyContent: 'center',
				alignItems: 'center',
			}}
		>
			<div
				style={{
					width: 900,
					height: 540,
					borderRadius: 48,
					backgroundColor: '#27272a',
					overflow: 'hidden',
					display: 'flex',
					justifyContent: 'center',
					alignItems: 'center',
					border: '6px solid #52525b',
				}}
			>
				<div
					style={{
						width: 360,
						height: 240,
						borderRadius: 32,
						backgroundColor: '#10b981',
						opacity,
						transformOrigin: 'center center',
						transform: `translate(${tx}px, 0px) rotate(${angle}deg) scale(${scale}) skewX(${skewX}deg)`,
						display: 'flex',
						justifyContent: 'center',
						alignItems: 'center',
					}}
				>
					<div
						style={{
							width: 140,
							height: 140,
							borderRadius: 24,
							backgroundColor: '#f43f5e',
							transform: `rotate(${-angle * 1.5}deg)`,
						}}
					/>
				</div>
			</div>
		</AbsoluteFill>
	);
};
