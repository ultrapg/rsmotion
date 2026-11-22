import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

export const GradientsShadows: React.FC = () => {
	const frame = useCurrentFrame();
	const angle = interpolate(frame, [0, 59], [120, 240]);
	const offsetY = interpolate(frame, [0, 30, 59], [0, -24, 0]);

	return (
		<AbsoluteFill
			style={{
				background: `radial-gradient(circle at 50% 40%, #1d4ed8 0%, #0f172a 75%)`,
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div
				style={{
					display: 'flex',
					flexDirection: 'row',
					gap: 48,
				}}
			>
				{[0, 1, 2].map((idx) => (
					<div
						key={idx}
						style={{
							width: 420,
							height: 520,
							borderRadius: 36,
							border: '4px solid rgba(255, 255, 255, 0.25)',
							background: `linear-gradient(${angle + idx * 30}deg, #ec4899 0%, #8b5cf6 50%, #06b6d4 100%)`,
							boxShadow: '0px 28px 60px rgba(0, 0, 0, 0.65)',
							display: 'flex',
							flexDirection: 'column',
							justifyContent: 'center',
							alignItems: 'center',
							transform: `translateY(${offsetY * (idx === 1 ? -1 : 1)}px)`,
						}}
					>
						<span
							style={{
								fontSize: 64,
								fontWeight: 700,
								color: '#ffffff',
								textShadow: '0px 6px 18px rgba(0, 0, 0, 0.5)',
							}}
						>
							Card {idx + 1}
						</span>
					</div>
				))}
			</div>
		</AbsoluteFill>
	);
};
