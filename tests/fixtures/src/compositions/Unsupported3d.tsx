import React, {useEffect, useRef} from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';

export const Unsupported3d: React.FC = () => {
	const frame = useCurrentFrame();
	const canvasRef = useRef<HTMLCanvasElement | null>(null);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;
		const ctx = canvas.getContext('2d');
		if (!ctx) return;

		ctx.clearRect(0, 0, canvas.width, canvas.height);
		ctx.fillStyle = '#0f172a';
		ctx.fillRect(0, 0, canvas.width, canvas.height);

		const cx = 400;
		const cy = 300;
		const angle = (frame / 60) * Math.PI * 2;
		ctx.save();
		ctx.translate(cx, cy);
		ctx.rotate(angle);
		ctx.fillStyle = '#f97316';
		ctx.fillRect(-140, -140, 280, 280);
		ctx.restore();
	}, [frame]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#1e293b',
				justifyContent: 'center',
				alignItems: 'center',
			}}
		>
			<canvas
				ref={canvasRef}
				width={800}
				height={600}
				style={{borderRadius: 24}}
			/>
			<div
				style={{
					position: 'absolute',
					padding: '32px 56px',
					borderRadius: 24,
					backgroundColor: 'rgba(255, 255, 255, 0.2)',
					backdropFilter: 'blur(12px)',
					mixBlendMode: 'difference',
					color: '#ffffff',
					fontSize: 56,
					fontFamily: '"DejaVu Sans", sans-serif',
					fontWeight: 700,
				}}
			>
				Fallback Trigger: Canvas + BackdropBlur
			</div>
		</AbsoluteFill>
	);
};
