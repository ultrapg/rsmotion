import React from 'react';
import {AbsoluteFill, interpolate, useCurrentFrame} from 'remotion';

const TAILWIND_GRID_CSS = `
@layer base, components, utilities;

@property --card-radius {
  syntax: "<length>";
  inherits: false;
  initial-value: 24px;
}

:root {
  --spacing: 0.25rem;
  --card-surface: oklch(0.24 0.03 260);
  --accent-sky: oklch(0.72 0.16 220);
  --accent-pink: oklch(0.68 0.22 350);
  --soft-tint: color-mix(in oklab, #38bdf8 22%, transparent);
}

@layer base {
  .tw-stage {
    font-family: "DejaVu Sans", sans-serif;
    color: #f8fafc;
  }
}

@layer components {
  .grid-card {
    background-color: var(--card-surface);
    border: 2px solid color-mix(in oklab, var(--accent-sky) 45%, transparent);
    border-radius: var(--card-radius);
    padding: calc(var(--spacing) * 8);
  }
  .grid-card:first-child {
    border-color: var(--accent-pink);
  }
  .grid-card:not(:first-child) {
    background-color: oklch(0.21 0.03 255);
  }
}

@layer utilities {
  .bg-\\[\\#0f172a\\] { background-color: #0f172a; }
  .p-12 { padding: calc(var(--spacing) * 12); }
  .gap-6 { gap: calc(var(--spacing) * 6); }
  .grid { display: grid; }
  .col-span-2 { grid-column: span 2; }
  .place-center { place-items: center; }
  .text-sky { color: var(--accent-sky); }
  .bg-tint { background-color: var(--soft-tint); }
}

@media (min-width: 1280px) {
  .md\\:grid-cols-3 {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

.metric-row:nth-child(2) .metric-label {
  color: #f472b6;
}
`;

export const CssTailwindGrid: React.FC = () => {
	const frame = useCurrentFrame();
	const pct1 = interpolate(frame, [0, 59], [28, 92], {extrapolateRight: 'clamp'});
	const pct2 = interpolate(frame, [0, 59], [45, 78], {extrapolateRight: 'clamp'});
	const pct3 = interpolate(frame, [0, 59], [18, 64], {extrapolateRight: 'clamp'});

	return (
		<AbsoluteFill className="tw-stage bg-[#0f172a] p-12">
			<style>{TAILWIND_GRID_CSS}</style>
			<div
				className="grid md:grid-cols-3 gap-6"
				style={{width: '100%', height: '100%', gridTemplateRows: '180px 1fr 220px'}}
			>
				<div
					className="grid-card col-span-2"
					style={{display: 'flex', flexDirection: 'column', justifyContent: 'center'}}
				>
					<div className="text-sky" style={{fontSize: 24, fontWeight: 700, letterSpacing: 2}}>
						RSMOTION CSS ENGINE · TAILWIND V4 + GRID
					</div>
					<div style={{fontSize: 52, fontWeight: 700, marginTop: 10}}>
						@layer · var() · calc() · oklch()
					</div>
				</div>

				<div className="grid-card bg-tint grid place-center">
					<div style={{textAlign: 'center'}}>
						<div style={{fontSize: 22, color: '#94a3b8'}}>FRAME</div>
						<div style={{fontSize: 58, fontWeight: 700, color: '#38bdf8'}}>{frame}</div>
					</div>
				</div>

				<div className="grid-card col-span-2" style={{display: 'flex', flexDirection: 'column', gap: 22, justifyContent: 'center'}}>
					{[
						{label: 'CSS Cascade + Layers', pct: pct1, color: '#38bdf8'},
						{label: 'Taffy CSS Grid Solver', pct: pct2, color: '#f472b6'},
						{label: 'Custom Vars + calc()', pct: pct3, color: '#a78bfa'},
					].map((row) => (
						<div key={row.label} className="metric-row" style={{display: 'flex', flexDirection: 'column', gap: 10}}>
							<div style={{display: 'flex', justifyContent: 'space-between', fontSize: 26, fontWeight: 700}}>
								<span className="metric-label">{row.label}</span>
								<span>{Math.round(row.pct)}%</span>
							</div>
							<div style={{height: 22, backgroundColor: '#020617', borderRadius: 11, overflow: 'hidden'}}>
								<div style={{width: `${row.pct}%`, height: '100%', backgroundColor: row.color, borderRadius: 11}} />
							</div>
						</div>
					))}
				</div>

				<div className="grid-card" style={{display: 'flex', flexDirection: 'column', justifyContent: 'space-between'}}>
					<div style={{fontSize: 28, fontWeight: 700}}>Escaped Classes</div>
					<div style={{fontSize: 22, color: '#cbd5e1', lineHeight: 1.5}}>
						.bg-[#0f172a]
						{'\n'}md:grid-cols-3
						{'\n'}repeat(3, minmax(0, 1fr))
					</div>
					<div style={{padding: '14px 20px', borderRadius: 14, backgroundColor: '#1e3a5f', fontSize: 22, fontWeight: 700, color: '#7dd3fc'}}>
						Tier 1 Native
					</div>
				</div>

				<div className="grid-card" style={{display: 'flex', flexDirection: 'column', justifyContent: 'center'}}>
					<div style={{fontSize: 20, color: '#94a3b8'}}>SPAN 1</div>
					<div style={{fontSize: 34, fontWeight: 700, marginTop: 8}}>color-mix()</div>
				</div>

				<div className="grid-card col-span-2" style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center'}}>
					<div style={{fontSize: 32, fontWeight: 700}}>Grid Auto Placement + Span 2</div>
					<div style={{fontSize: 24, color: '#38bdf8', fontWeight: 700}}>1920 × 1080</div>
				</div>
			</div>
		</AbsoluteFill>
	);
};
