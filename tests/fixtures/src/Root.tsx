import React from 'react';
import {Composition} from 'remotion';
import {AudioMix} from './compositions/AudioMix';
import {AudioViz} from './compositions/AudioViz';
import {Canvas2dDemo} from './compositions/Canvas2dDemo';
import {Captions} from './compositions/Captions';
import {ChartBars} from './compositions/ChartBars';
import {CssEffects3d} from './compositions/CssEffects3d';
import {CssTailwindGrid} from './compositions/CssTailwindGrid';
import {FontsCustom} from './compositions/FontsCustom';
import {GradientsShadows} from './compositions/GradientsShadows';
import {ImageSlideshow} from './compositions/ImageSlideshow';
import {LottieNative} from './compositions/LottieNative';
import {SvgShapes} from './compositions/SvgShapes';
import {TextAnimated} from './compositions/TextAnimated';
import {TextBasic} from './compositions/TextBasic';
import {TransformsOpacity} from './compositions/TransformsOpacity';
import {Unsupported3d} from './compositions/Unsupported3d';
import {UnsupportedWebgl} from './compositions/UnsupportedWebgl';
import {VideoOverlay} from './compositions/VideoOverlay';

const COMMON = {
	width: 1920,
	height: 1080,
	fps: 30,
	durationInFrames: 60,
};

export const RemotionRoot: React.FC = () => {
	return (
		<>
			<Composition id="text-basic" component={TextBasic} {...COMMON} />
			<Composition id="text-animated" component={TextAnimated} {...COMMON} />
			<Composition id="captions" component={Captions} {...COMMON} />
			<Composition id="image-slideshow" component={ImageSlideshow} {...COMMON} />
			<Composition id="transforms-opacity" component={TransformsOpacity} {...COMMON} />
			<Composition id="gradients-shadows" component={GradientsShadows} {...COMMON} />
			<Composition id="svg-shapes" component={SvgShapes} {...COMMON} />
			<Composition id="chart-bars" component={ChartBars} {...COMMON} />
			<Composition id="video-overlay" component={VideoOverlay} {...COMMON} />
			<Composition id="audio-mix" component={AudioMix} {...COMMON} />
			<Composition id="fonts-custom" component={FontsCustom} {...COMMON} />
			<Composition id="css-tailwind-grid" component={CssTailwindGrid} {...COMMON} />
			<Composition id="css-effects-3d" component={CssEffects3d} {...COMMON} />
			<Composition id="unsupported-3d" component={Unsupported3d} {...COMMON} />
			<Composition id="canvas-2d" component={Canvas2dDemo} {...COMMON} />
			<Composition id="lottie-native" component={LottieNative} {...COMMON} />
			<Composition id="audio-viz" component={AudioViz} {...COMMON} />
			<Composition id="unsupported-webgl" component={UnsupportedWebgl} {...COMMON} />
		</>
	);
};
