import React from 'react';
import {
	AbsoluteFill,
	interpolate,
	spring,
	useCurrentFrame,
	useVideoConfig,
} from 'remotion';

const DATA = [
	{label: 'Q1', value: 42, color: '#38bdf8'},
	{label: 'Q2', value: 68, color: '#818cf8'},
	{label: 'Q3', value: 55, color: '#34d399'},
	{label: 'Q4', value: 92, color: '#fbbf24'},
	{label: 'Q5', value: 78, color: '#f472b6'},
];

export const ChartBars: React.FC = () => {
	const frame = useCurrentFrame();
	const {fps} = useVideoConfig();

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#0f172a',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<svg width={1500} height={820} viewBox="0 0 1500 820">
				{[0, 25, 50, 75, 100].map((tick) => {
					const y = 700 - tick * 5.5;
					return (
						<g key={tick}>
							<line
								x1={140}
								y1={y}
								x2={1400}
								y2={y}
								stroke="#334155"
								strokeWidth={2}
							/>
							<text
								x={110}
								y={y + 8}
								fill="#94a3b8"
								fontSize={24}
								textAnchor="end"
							>
								{tick}
							</text>
						</g>
					);
				})}

				{DATA.map((item, idx) => {
					const prog = spring({
						fps,
						frame: frame - idx * 5,
						config: {damping: 14, stiffness: 120},
					});
					const val = interpolate(prog, [0, 1], [0, item.value]);
					const barH = val * 5.5;
					const x = 220 + idx * 230;
					const y = 700 - barH;
					return (
						<g key={item.label}>
							<rect
								x={x}
								y={y}
								width={140}
								height={barH}
								rx={14}
								fill={item.color}
							/>
							<text
								x={x + 70}
								y={y - 16}
								fill="#f8fafc"
								fontSize={28}
								fontWeight="bold"
								textAnchor="middle"
							>
								{Math.round(val)}
							</text>
							<text
								x={x + 70}
								y={748}
								fill="#e2e8f0"
								fontSize={30}
								textAnchor="middle"
							>
								{item.label}
							</text>
						</g>
					);
				})}
			</svg>
		</AbsoluteFill>
	);
};
