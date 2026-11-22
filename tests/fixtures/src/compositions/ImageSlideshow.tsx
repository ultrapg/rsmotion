import React from 'react';
import {
	AbsoluteFill,
	Img,
	Sequence,
	interpolate,
	staticFile,
	useCurrentFrame,
} from 'remotion';

export const ImageSlideshow: React.FC = () => {
	const frame = useCurrentFrame();
	const crossfade = interpolate(frame, [24, 36], [0, 1], {
		extrapolateLeft: 'clamp',
		extrapolateRight: 'clamp',
	});

	return (
		<AbsoluteFill style={{backgroundColor: '#000000'}}>
			<Sequence from={0} durationInFrames={40}>
				<AbsoluteFill style={{opacity: 1 - crossfade}}>
					<Img
						src={staticFile('sample-image.png')}
						style={{
							width: '100%',
							height: '100%',
							objectFit: 'cover',
						}}
					/>
				</AbsoluteFill>
			</Sequence>
			<Sequence from={20} durationInFrames={40}>
				<AbsoluteFill style={{opacity: crossfade}}>
					<Img
						src={staticFile('sample-image-2.png')}
						style={{
							width: '100%',
							height: '100%',
							objectFit: 'cover',
						}}
					/>
				</AbsoluteFill>
			</Sequence>
		</AbsoluteFill>
	);
};
