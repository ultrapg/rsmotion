import React, {useLayoutEffect, useRef} from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame, useVideoConfig} from 'remotion';

export const Canvas2dDemo: React.FC = () => {
	const frame = useCurrentFrame();
	const {width, height, durationInFrames} = useVideoConfig();
	const canvasRef = useRef<HTMLCanvasElement | null>(null);

	useLayoutEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) {
			return;
		}
		const ctx = canvas.getContext('2d');
		if (!ctx) {
			return;
		}

		const progress = frame / Math.max(1, durationInFrames - 1);
		ctx.clearRect(0, 0, width, height);

		// Background linear gradient
		const bg = ctx.createLinearGradient(0, 0, width, height);
		bg.addColorStop(0, '#0f172a');
		bg.addColorStop(0.5, '#1e293b');
		bg.addColorStop(1, '#090d16');
		ctx.fillStyle = bg;
		ctx.fillRect(0, 0, width, height);

		// Animated radial gradient orb
		const orbX = interpolate(progress, [0, 1], [380, 1540]);
		const orbY = 320 + Math.sin(progress * Math.PI * 2) * 90;
		const rad = ctx.createRadialGradient(orbX, orbY, 20, orbX, orbY, 260);
		rad.addColorStop(0, 'rgba(56, 189, 248, 0.95)');
		rad.addColorStop(0.55, 'rgba(14, 165, 233, 0.45)');
		rad.addColorStop(1, 'rgba(14, 165, 233, 0)');
		ctx.fillStyle = rad;
		ctx.beginPath();
		ctx.arc(orbX, orbY, 260, 0, Math.PI * 2);
		ctx.fill();

		// Shadowed rounded card
		ctx.save();
		ctx.shadowColor = 'rgba(0, 0, 0, 0.6)';
		ctx.shadowBlur = 32;
		ctx.shadowOffsetX = 0;
		ctx.shadowOffsetY = 16;
		ctx.fillStyle = '#1e293b';
		ctx.strokeStyle = '#38bdf8';
		ctx.lineWidth = 4;
		ctx.beginPath();
		ctx.roundRect(180, 440, 1560, 460, 32);
		ctx.fill();
		ctx.shadowColor = 'transparent';
		ctx.stroke();
		ctx.restore();

		// Dashed bezier ribbon across the card
		ctx.save();
		ctx.strokeStyle = '#f59e0b';
		ctx.lineWidth = 8;
		ctx.lineCap = 'round';
		ctx.setLineDash([24, 14]);
		ctx.beginPath();
		ctx.moveTo(260, 740);
		ctx.bezierCurveTo(
			620,
			500 + Math.sin(progress * Math.PI * 2) * 120,
			1280,
			860 - Math.cos(progress * Math.PI * 2) * 120,
			1660,
			620,
		);
		ctx.stroke();
		ctx.restore();

		// Rotating geometric badge
		ctx.save();
		ctx.translate(460, 670);
		ctx.rotate(progress * Math.PI * 2);
		const pulse = 1 + 0.14 * Math.sin(progress * Math.PI * 4);
		ctx.scale(pulse, pulse);
		ctx.fillStyle = '#22c55e';
		ctx.beginPath();
		for (let i = 0; i < 6; i++) {
			const ang = (i / 6) * Math.PI * 2;
			const rx = Math.cos(ang) * 90;
			const ry = Math.sin(ang) * 90;
			if (i === 0) ctx.moveTo(rx, ry);
			else ctx.lineTo(rx, ry);
		}
		ctx.closePath();
		ctx.fill();
		ctx.restore();

		// Partial progress ring arc
		ctx.save();
		ctx.strokeStyle = '#a855f7';
		ctx.lineWidth = 18;
		ctx.lineCap = 'round';
		ctx.beginPath();
		ctx.arc(1460, 670, 96, -Math.PI / 2, -Math.PI / 2 + Math.max(0.15, progress) * Math.PI * 2);
		ctx.stroke();
		ctx.restore();

		// Canvas typography with measureText + fillText
		ctx.save();
		ctx.font = '700 54px "DejaVu Sans", sans-serif';
		ctx.fillStyle = '#f8fafc';
		ctx.textBaseline = 'middle';
		const title = `Canvas 2D Native • Frame ${frame}`;
		const measured = ctx.measureText(title);
		ctx.fillText(title, (width - measured.width) / 2, 200);
		ctx.restore();
	}, [frame, width, height, durationInFrames]);

	return (
		<AbsoluteFill style={{backgroundColor: '#090d16'}}>
			<canvas
				ref={canvasRef}
				width={width}
				height={height}
				style={{width, height}}
			/>
		</AbsoluteFill>
	);
};
