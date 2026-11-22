import React, {useEffect, useState} from 'react';
import {
	AbsoluteFill,
	continueRender,
	delayRender,
	staticFile,
	useCurrentFrame,
} from 'remotion';

export const FontsCustom: React.FC = () => {
	const frame = useCurrentFrame();
	const [handle] = useState(() => delayRender('Loading Space Grotesk font'));

	useEffect(() => {
		const face = new FontFace(
			'SpaceGroteskLocal',
			`url(${staticFile('fonts/SpaceGrotesk-Regular.ttf')}) format('truetype')`,
		);
		face
			.load()
			.then((loaded) => {
				(document.fonts as unknown as {add: (font: FontFace) => void}).add(
					loaded,
				);
				continueRender(handle);
			})
			.catch((err) => {
				console.error(err);
				continueRender(handle);
			});
	}, [handle]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#18181b',
				justifyContent: 'center',
				alignItems: 'flex-start',
				padding: 120,
				fontFamily: '"SpaceGroteskLocal", sans-serif',
			}}
		>
			<div style={{fontSize: 96, color: '#fafafa', marginBottom: 24}}>
				Space Grotesk Custom Font
			</div>
			<div style={{fontSize: 48, color: '#a1a1aa', marginBottom: 16}}>
				0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ
			</div>
			<div style={{fontSize: 32, color: '#38bdf8'}}>
				Loaded via FontFace + delayRender • Frame {frame}
			</div>
		</AbsoluteFill>
	);
};
