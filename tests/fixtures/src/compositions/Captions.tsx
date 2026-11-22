import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';

const TOKENS = [
	{text: 'Rendering', from: 0, to: 10},
	{text: 'video', from: 10, to: 20},
	{text: 'frames', from: 20, to: 30},
	{text: 'without', from: 30, to: 42},
	{text: 'Chrome', from: 42, to: 52},
	{text: 'overhead', from: 52, to: 60},
];

export const Captions: React.FC = () => {
	const frame = useCurrentFrame();

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#1e1b4b',
				justifyContent: 'space-between',
				alignItems: 'center',
				padding: 80,
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div
				style={{
					fontSize: 42,
					color: '#c7d2fe',
					fontWeight: 600,
				}}
			>
				Caption Highlight Pattern • Frame {frame}
			</div>

			<div
				style={{
					display: 'flex',
					flexDirection: 'row',
					flexWrap: 'wrap',
					justifyContent: 'center',
					alignItems: 'center',
					gap: 16,
					backgroundColor: 'rgba(15, 23, 42, 0.85)',
					padding: '28px 44px',
					borderRadius: 24,
					marginBottom: 40,
				}}
			>
				{TOKENS.map((token) => {
					const active = frame >= token.from && frame < token.to;
					return (
						<span
							key={token.text}
							style={{
								fontSize: 56,
								fontWeight: 700,
								padding: '10px 22px',
								borderRadius: 16,
								backgroundColor: active ? '#facc15' : 'transparent',
								color: active ? '#0f172a' : '#f8fafc',
							}}
						>
							{token.text}
						</span>
					);
				})}
			</div>
		</AbsoluteFill>
	);
};
