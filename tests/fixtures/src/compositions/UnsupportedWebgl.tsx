import React, {useLayoutEffect, useRef} from 'react';
import {AbsoluteFill, useCurrentFrame, useVideoConfig} from 'remotion';

export const UnsupportedWebgl: React.FC = () => {
	const frame = useCurrentFrame();
	const {durationInFrames} = useVideoConfig();
	const canvasRef = useRef<HTMLCanvasElement | null>(null);

	useLayoutEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) {
			return;
		}
		const gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
		if (!gl) {
			return;
		}

		const t = frame / Math.max(1, durationInFrames - 1);
		const vsSrc = `
			attribute vec2 aPos;
			attribute vec3 aColor;
			varying vec3 vColor;
			uniform float uAngle;
			void main() {
				float c = cos(uAngle);
				float s = sin(uAngle);
				vec2 rot = vec2(aPos.x * c - aPos.y * s, aPos.x * s + aPos.y * c);
				gl_Position = vec4(rot, 0.0, 1.0);
				vColor = aColor;
			}
		`;
		const fsSrc = `
			precision mediump float;
			varying vec3 vColor;
			void main() {
				gl_FragColor = vec4(vColor, 1.0);
			}
		`;

		const compile = (type: number, src: string) => {
			const shader = gl.createShader(type);
			if (!shader) return null;
			gl.shaderSource(shader, src);
			gl.compileShader(shader);
			return shader;
		};

		const vs = compile(gl.VERTEX_SHADER, vsSrc);
		const fs = compile(gl.FRAGMENT_SHADER, fsSrc);
		const prog = gl.createProgram();
		if (!vs || !fs || !prog) {
			return;
		}
		gl.attachShader(prog, vs);
		gl.attachShader(prog, fs);
		gl.linkProgram(prog);
		gl.useProgram(prog);

		const verts = new Float32Array([
			0.0, 0.65, 1.0, 0.2, 0.4,
			-0.6, -0.45, 0.2, 0.9, 0.5,
			0.6, -0.45, 0.2, 0.5, 1.0,
		]);
		const buf = gl.createBuffer();
		gl.bindBuffer(gl.ARRAY_BUFFER, buf);
		gl.bufferData(gl.ARRAY_BUFFER, verts, gl.STATIC_DRAW);

		const aPos = gl.getAttribLocation(prog, 'aPos');
		const aColor = gl.getAttribLocation(prog, 'aColor');
		gl.enableVertexAttribArray(aPos);
		gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 20, 0);
		gl.enableVertexAttribArray(aColor);
		gl.vertexAttribPointer(aColor, 3, gl.FLOAT, false, 20, 8);

		const uAngle = gl.getUniformLocation(prog, 'uAngle');
		gl.uniform1f(uAngle, t * Math.PI * 2);

		gl.viewport(0, 0, canvas.width, canvas.height);
		gl.clearColor(0.05, 0.08, 0.16, 1.0);
		gl.clear(gl.COLOR_BUFFER_BIT);
		gl.drawArrays(gl.TRIANGLES, 0, 3);
	}, [frame, durationInFrames]);

	return (
		<AbsoluteFill
			style={{
				backgroundColor: '#0d1328',
				justifyContent: 'center',
				alignItems: 'center',
				fontFamily: '"DejaVu Sans", sans-serif',
			}}
		>
			<div style={{color: '#f8fafc', fontSize: 44, marginBottom: 24}}>
				WebGL Fallback Fixture • Frame {frame}
			</div>
			<canvas
				ref={canvasRef}
				width={1280}
				height={720}
				style={{width: 1280, height: 720}}
			/>
		</AbsoluteFill>
	);
};
