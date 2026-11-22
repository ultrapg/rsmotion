import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

export const CssEffects3d: React.FC = () => {
	const frame = useCurrentFrame();
	const rotY = interpolate(frame, [0, 59], [-18, 18], {extrapolateRight: 'clamp'});
	const rotX = interpolate(frame, [0, 59], [8, -8], {extrapolateRight: 'clamp'});
	const orbX = interpolate(frame, [0, 59], [280, 620], {extrapolateRight: 'clamp'});

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#0f172a',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			{/* Filtered & Clipped Backdrop Scene Shapes */}
			<div
				style={{
					position: 'absolute',
					left: 140,
					top: 140,
					width: 520,
					height: 520,
					background: 'linear-gradient(135deg, #ec4899 0%, #8b5cf6 100%)',
					clipPath: 'polygon(15% 0%, 100% 10%, 85% 100%, 0% 88%)',
					filter: 'brightness(1.15) contrast(1.1) drop-shadow(0px 18px 24px rgba(0, 0, 0, 0.55))',
				}}
			/>

			<div
				style={{
					position: 'absolute',
					left: orbX,
					top: 260,
					width: 440,
					height: 440,
					background: 'radial-gradient(circle, #38bdf8 0%, #2563eb 100%)',
					clipPath: 'circle(48% at 50% 50%)',
					mixBlendMode: 'screen',
				}}
			/>

			<div
				style={{
					position: 'absolute',
					right: 160,
					bottom: 140,
					width: 480,
					height: 280,
					backgroundColor: '#f59e0b',
					borderRadius: 36,
					maskImage: 'linear-gradient(to bottom, #ffffff 0%, #ffffff 45%, transparent 100%)',
					WebkitMaskImage:
						'linear-gradient(to bottom, #ffffff 0%, #ffffff 45%, transparent 100%)',
					display: 'flex',
					justifyContent: 'center',
					alignItems: 'center',
				}}
			>
				<span style={{fontSize: 44, fontWeight: 700, color: '#0f172a'}}>MASK-IMAGE</span>
			</div>

			{/* Backdrop Blur Glass Card */}
			<div
				style={{
					position: 'absolute',
					left: 160,
					top: 320,
					width: 760,
					height: 360,
					borderRadius: 28,
					border: '2px solid rgba(255, 255, 255, 0.28)',
					backgroundColor: 'rgba(255, 255, 255, 0.16)',
					backdropFilter: 'blur(14px) saturate(1.4)',
					WebkitBackdropFilter: 'blur(14px) saturate(1.4)',
					padding: 44,
					display: 'flex',
					flexDirection: 'column',
					justifyContent: 'center',
					gap: 18,
				}}
			>
				<div
					style={{
						fontSize: 56,
						fontWeight: 700,
						background: 'linear-gradient(90deg, #38bdf8 0%, #c084fc 50%, #fb7185 100%)',
						backgroundClip: 'text',
						WebkitBackgroundClip: 'text',
						color: 'transparent',
					}}
				>
					BACKDROP + CLIP TEXT
				</div>
				<div style={{fontSize: 28, color: '#f8fafc'}}>
					blur(14px) · saturate(1.4) · screen blend · Frame {frame}
				</div>
			</div>

			{/* 3D Perspective Card */}
			<div
				style={{
					position: 'absolute',
					right: 160,
					top: 160,
					width: 620,
					height: 380,
					perspective: 900,
					display: 'flex',
					justifyContent: 'center',
					alignItems: 'center',
				}}
			>
				<div
					style={{
						width: 560,
						height: 320,
						borderRadius: 28,
						background: 'linear-gradient(145deg, #1e293b 0%, #0f172a 100%)',
						border: '3px solid #38bdf8',
						padding: 40,
						transform: `rotateY(${rotY.toFixed(2)}deg) rotateX(${rotX.toFixed(2)}deg) translateZ(24px)`,
						display: 'flex',
						flexDirection: 'column',
						justifyContent: 'space-between',
					}}
				>
					<div style={{fontSize: 22, color: '#38bdf8', fontWeight: 700}}>
						3D M44 PERSPECTIVE
					</div>
					<div style={{fontSize: 42, color: '#ffffff', fontWeight: 700}}>
						rotateY({Math.round(rotY)}deg)
					</div>
					<div
						style={{
							alignSelf: 'flex-start',
							padding: '10px 20px',
							borderRadius: 12,
							backgroundColor: '#f97316',
							mixBlendMode: 'difference',
							color: '#ffffff',
							fontSize: 22,
							fontWeight: 700,
						}}
					>
						DIFFERENCE BLEND
					</div>
				</div>
			</div>
		</AbsoluteFill>
	);
};
