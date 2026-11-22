import React from 'react';
import {Lottie, type LottieAnimationData} from '@remotion/lottie';
import {AbsoluteFill} from 'remotion';

const GEOMETRIC_LOTTIE: LottieAnimationData = {
	v: '5.7.4',
	fr: 30,
	ip: 0,
	op: 60,
	w: 500,
	h: 500,
	nm: 'NativeGeometric',
	ddd: 0,
	assets: [],
	layers: [
		{
			ddd: 0,
			ind: 1,
			ty: 4,
			nm: 'RotatingStar',
			sr: 1,
			ks: {
				o: {a: 0, k: 100},
				r: {
					a: 1,
					k: [
						{t: 0, s: [0], i: {x: [1], y: [1]}, o: {x: [0], y: [0]}},
						{t: 59, s: [180]},
					],
				},
				p: {a: 0, k: [250, 250, 0]},
				a: {a: 0, k: [0, 0, 0]},
				s: {
					a: 1,
					k: [
						{t: 0, s: [70, 70, 100], i: {x: [0.5, 0.5, 0.5], y: [1, 1, 1]}, o: {x: [0.5, 0.5, 0.5], y: [0, 0, 0]}},
						{t: 30, s: [115, 115, 100], i: {x: [0.5, 0.5, 0.5], y: [1, 1, 1]}, o: {x: [0.5, 0.5, 0.5], y: [0, 0, 0]}},
						{t: 59, s: [70, 70, 100]},
					],
				},
			},
			ao: 0,
			shapes: [
				{
					ty: 'gr',
					nm: 'StarGroup',
					np: 2,
					it: [
						{
							ty: 'sr',
							sy: 1,
							pt: {a: 0, k: 5},
							p: {a: 0, k: [0, 0]},
							r: {a: 0, k: 0},
							ir: {a: 0, k: 46},
							or: {a: 0, k: 105},
							is: {a: 0, k: 0},
							os: {a: 0, k: 0},
							nm: 'StarPath',
						},
						{
							ty: 'fl',
							c: {a: 0, k: [0.98, 0.98, 0.99, 1]},
							o: {a: 0, k: 100},
							r: 1,
							nm: 'StarFill',
						},
						{
							ty: 'tr',
							p: {a: 0, k: [0, 0]},
							a: {a: 0, k: [0, 0]},
							s: {a: 0, k: [100, 100]},
							r: {a: 0, k: 0},
							o: {a: 0, k: 100},
						},
					],
				},
			],
			ip: 0,
			op: 60,
			st: 0,
			bm: 0,
		},
		{
			ddd: 0,
			ind: 2,
			ty: 4,
			nm: 'OrbCircle',
			sr: 1,
			ks: {
				o: {a: 0, k: 100},
				r: {a: 0, k: 0},
				p: {
					a: 1,
					k: [
						{t: 0, s: [130, 250, 0], i: {x: 0.42, y: 1}, o: {x: 0.58, y: 0}},
						{t: 30, s: [370, 250, 0], i: {x: 0.42, y: 1}, o: {x: 0.58, y: 0}},
						{t: 59, s: [130, 250, 0]},
					],
				},
				a: {a: 0, k: [0, 0, 0]},
				s: {a: 0, k: [100, 100, 100]},
			},
			ao: 0,
			shapes: [
				{
					ty: 'gr',
					nm: 'OrbGroup',
					np: 2,
					it: [
						{
							ty: 'el',
							d: 1,
							s: {a: 0, k: [110, 110]},
							p: {a: 0, k: [0, 0]},
							nm: 'OrbEllipse',
						},
						{
							ty: 'fl',
							c: {a: 0, k: [0.96, 0.62, 0.04, 1]},
							o: {a: 0, k: 95},
							r: 1,
							nm: 'OrbFill',
						},
						{
							ty: 'tr',
							p: {a: 0, k: [0, 0]},
							a: {a: 0, k: [0, 0]},
							s: {a: 0, k: [100, 100]},
							r: {a: 0, k: 0},
							o: {a: 0, k: 100},
						},
					],
				},
			],
			ip: 0,
			op: 60,
			st: 0,
			bm: 0,
		},
		{
			ddd: 0,
			ind: 3,
			ty: 4,
			nm: 'RoundedCard',
			sr: 1,
			ks: {
				o: {a: 0, k: 100},
				r: {
					a: 1,
					k: [
						{t: 0, s: [-12], i: {x: [1], y: [1]}, o: {x: [0], y: [0]}},
						{t: 30, s: [12], i: {x: [1], y: [1]}, o: {x: [0], y: [0]}},
						{t: 59, s: [-12]},
					],
				},
				p: {a: 0, k: [250, 250, 0]},
				a: {a: 0, k: [0, 0, 0]},
				s: {a: 0, k: [100, 100, 100]},
			},
			ao: 0,
			shapes: [
				{
					ty: 'gr',
					nm: 'CardGroup',
					np: 3,
					it: [
						{
							ty: 'rc',
							d: 1,
							s: {a: 0, k: [290, 290]},
							p: {a: 0, k: [0, 0]},
							r: {a: 0, k: 48},
							nm: 'CardRect',
						},
						{
							ty: 'fl',
							c: {a: 0, k: [0.055, 0.647, 0.914, 1]},
							o: {a: 0, k: 100},
							r: 1,
							nm: 'CardFill',
						},
						{
							ty: 'st',
							c: {a: 0, k: [0.88, 0.95, 1, 1]},
							o: {a: 0, k: 100},
							w: {a: 0, k: 10},
							lc: 2,
							lj: 2,
							nm: 'CardStroke',
						},
						{
							ty: 'tr',
							p: {a: 0, k: [0, 0]},
							a: {a: 0, k: [0, 0]},
							s: {a: 0, k: [100, 100]},
							r: {a: 0, k: 0},
							o: {a: 0, k: 100},
						},
					],
				},
			],
			ip: 0,
			op: 60,
			st: 0,
			bm: 0,
		},
	],
};

export const LottieNative: React.FC = () => {
	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#1e1b4b',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div
				style={{
					color: '#e0e7ff',
					fontSize: 48,
					fontWeight: 700,
					marginBottom: 24,
				}}
			>
				Skottie Native Lottie
			</div>
			<Lottie
				animationData={GEOMETRIC_LOTTIE}
				style={{
					width: 760,
					height: 760,
				}}
			/>
		</AbsoluteFill>
	);
};
