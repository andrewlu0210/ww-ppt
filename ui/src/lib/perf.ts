// Timing marks and WebAssembly memory, for performance measurement (the
// automation channel's `perf` command prints them).

let memory: WebAssembly.Memory | null = null;
const marks: [string, number][] = [];

/** Called once the WASM module is instantiated. */
export function setWasmMemory(m: WebAssembly.Memory) {
  memory = m;
}

/** The WASM heap size in MB (it only grows). */
export function wasmMemoryMB(): number {
  return memory ? memory.buffer.byteLength / 1048576 : 0;
}

/** Records how long `label` took (ms). */
export function mark(label: string, ms: number) {
  marks.push([label, Math.round(ms * 10) / 10]);
  if (marks.length > 200) marks.shift();
}

/** Times an async step. */
export async function timed<T>(label: string, f: () => Promise<T> | T): Promise<T> {
  const t = performance.now();
  try {
    return await f();
  } finally {
    mark(label, performance.now() - t);
  }
}

/** The marks so far (oldest first), and clears them. */
export function takeMarks(): [string, number][] {
  return marks.splice(0, marks.length);
}

/** Resolves after the next frame is painted (or after 250 ms: hidden windows get no frames). */
export const nextFrame = () =>
  new Promise<void>((r) => {
    const t = window.setTimeout(r, 250);
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        window.clearTimeout(t);
        r();
      }),
    );
  });
