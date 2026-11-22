import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

export const SvgShapes: React.FC = () => {
	const frame = useCurrentFrame();
	const rot = interpolate(frame, [0, 59], [0, 360]);
	const dashOffset = interpolate(frame, [0, 59], [400, 0]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#020617',
				justifyContent: 'center',
				alignItems: 'center',
			}}
		>
			<svg width={1400} height={800} viewBox="0 0 1400 800">
				<defs>
					<linearGradient id="svgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
						<stop offset="0%" stopColor="#22d3ee" />
						<stop offset="100%" stopColor="#a855f7" />
					</linearGradient>
					<clipPath id="circleClip">
						<circle cx={350} cy={400} r={180} />
					</clipPath>
				</defs>

				<rect
					x={170}
					y={220}
					width={360}
					height={360}
					fill="url(#svgGrad)"
					clipPath="url(#circleClip)"
				/>
				<ellipse cx={720} cy={220} rx={140} ry={70} fill="#f97316" />
				<line
					x1={560}
					y1={360}
					x2={900}
					y2={360}
					stroke="#e2e8f0"
					strokeWidth={8}
					strokeDasharray="24 12"
				/>
				<polygon
					points="720,420 840,620 600,620"
					fill="#10b981"
					transform={`rotate(${rot} 720 540)`}
				/>
				<path
					d="M 980 200 C 1180 120, 1280 460, 1020 600 Z"
					fill="none"
					stroke="#f43f5e"
					strokeWidth={14}
					strokeDasharray={400}
					strokeDashoffset={dashOffset}
				/>
			</svg>
		</AbsoluteFill>
	);
};
