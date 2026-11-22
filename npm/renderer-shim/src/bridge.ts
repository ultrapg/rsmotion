import {spawn, type ChildProcess} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

export type CancelCallback = () => void;
export type CancelSignal = ((callback: CancelCallback) => void) | AbortSignal;

export type RenderMediaProgress = {
  renderedFrames: number;
  encodedFrames: number;
  renderedDoneIn: number | null;
  encodedDoneIn: number | null;
  stitchStage: 'encoding' | 'muxing';
  progress: number;
};

export type RenderMediaOnProgress = (progress: RenderMediaProgress) => void;

export function resolveRsmotionBin(): string {
  const envBin = process.env.RSMOTION_BIN;
  if (envBin && fs.existsSync(envBin)) {
    return envBin;
  }

  const repoRoot = path.resolve(__dirname, '../../..');
  const candidates = [
    path.join(repoRoot, 'target', 'release', 'rsmotion'),
    path.join(repoRoot, 'target', 'debug', 'rsmotion'),
  ];
  for (const cand of candidates) {
    if (fs.existsSync(cand)) {
      return cand;
    }
  }
  return 'rsmotion';
}

export function runRsmotion<T = unknown>(
  args: string[],
  options?: {
    onProgress?: RenderMediaOnProgress;
    cancelSignal?: CancelSignal;
  },
): Promise<T> {
  return new Promise((resolve, reject) => {
    const bin = resolveRsmotionBin();
    const child: ChildProcess = spawn(bin, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let stdoutBuf = '';
    let stderrBuf = '';
    let lastJsonResult: T | undefined;
    let cancelled = false;
    let killTimer: NodeJS.Timeout | undefined;
    const renderStart = Date.now();

    const triggerCancel = () => {
      if (cancelled) return;
      cancelled = true;
      child.kill('SIGTERM');
      killTimer = setTimeout(() => {
        if (!child.killed) {
          child.kill('SIGKILL');
        }
      }, 2000);
    };

    if (options?.cancelSignal) {
      const sig = options.cancelSignal;
      if (typeof sig === 'function') {
        sig(triggerCancel);
      } else if (typeof sig === 'object' && 'aborted' in sig) {
        if (sig.aborted) {
          triggerCancel();
        } else {
          sig.addEventListener('abort', triggerCancel, {once: true});
        }
      }
    }

    child.stdout?.setEncoding('utf8');
    child.stdout?.on('data', (chunk: string) => {
      stdoutBuf += chunk;
      let nl = stdoutBuf.indexOf('\n');
      while (nl !== -1) {
        const line = stdoutBuf.slice(0, nl).trim();
        stdoutBuf = stdoutBuf.slice(nl + 1);
        if (line.startsWith('{')) {
          try {
            const parsed = JSON.parse(line) as Record<string, unknown>;
            if (parsed.type === 'progress' && options?.onProgress) {
              const renderedFrames = Number(parsed.renderedFrames ?? 0);
              const totalFrames = Math.max(1, Number(parsed.totalFrames ?? 1));
              const progress = Number(parsed.progress ?? renderedFrames / totalFrames);
              options.onProgress({
                renderedFrames,
                encodedFrames: renderedFrames,
                renderedDoneIn: progress >= 1 ? Date.now() - renderStart : null,
                encodedDoneIn: null,
                stitchStage: 'encoding',
                progress,
              });
            } else if (parsed.type === 'result') {
              lastJsonResult = parsed.data as T;
            } else {
              lastJsonResult = parsed as unknown as T;
            }
          } catch {
            // Non-JSON informational line
          }
        }
        nl = stdoutBuf.indexOf('\n');
      }
    });

    child.stderr?.setEncoding('utf8');
    child.stderr?.on('data', (chunk: string) => {
      stderrBuf += chunk;
    });

    child.on('error', (err) => {
      if (killTimer) clearTimeout(killTimer);
      reject(err);
    });

    child.on('close', (code) => {
      if (killTimer) clearTimeout(killTimer);
      const tail = stdoutBuf.trim();
      if (tail.startsWith('{')) {
        try {
          const parsed = JSON.parse(tail) as Record<string, unknown>;
          lastJsonResult = (parsed.type === 'result' ? parsed.data : parsed) as T;
        } catch {
          // Ignore trailing non-JSON
        }
      }

      if (cancelled) {
        reject(new Error('renderMedia() got cancelled'));
        return;
      }

      if (code !== 0) {
        reject(
          new Error(
            `rsmotion exited with code ${code}: ${stderrBuf.trim() || '(no stderr)'}`,
          ),
        );
        return;
      }

      resolve(lastJsonResult as T);
    });
  });
}
