import { useEffect, useRef } from "react";
import type { App } from "../wasm/ppt_wasm";

/** Ruler thickness in CSS pixels. */
export const RULER_SIZE = 18;
const EMU_PER_CM = 360_000;

/**
 * PowerPoint-style rulers along the top and left of the canvas, in centimeters
 * from the slide's center. The selected shape (or text box being edited) is
 * shown as a white band.
 */
export function Rulers({ engine, slideSize }: { engine: App | null; slideSize: [number, number] }) {
  const top = useRef<HTMLCanvasElement>(null);
  const left = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (!engine) return;
    let raf = 0;
    let lastKey = "";
    const frame = () => {
      raf = requestAnimationFrame(frame);
      const info = Array.from(engine.rulerInfo());
      const h = top.current;
      const v = left.current;
      if (!h || !v) return;
      const key = `${info.join(",")}|${h.clientWidth}x${v.clientHeight}|${slideSize}`;
      if (key === lastKey) return;
      lastKey = key;
      const [sx, sy, sw, shh] = info;
      const sel = info.length >= 8 ? info.slice(4, 8) : null;
      draw(h, true, sx, sw, slideSize[0], sel && [sel[0], sel[2]]);
      draw(v, false, sy, shh, slideSize[1], sel && [sel[1], sel[3]]);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [engine, slideSize]);
  return (
    <>
      <div className="ruler-corner" />
      <canvas ref={top} className="ruler ruler-h" />
      <canvas ref={left} className="ruler ruler-v" />
    </>
  );
}

/** Draws one ruler: `start`/`length` are the slide's span in CSS px, `extent` its size in EMU. */
function draw(canvas: HTMLCanvasElement, horizontal: boolean, start: number, length: number, extent: number, band: number[] | null) {
  const dpr = window.devicePixelRatio || 1;
  const along = horizontal ? canvas.clientWidth : canvas.clientHeight;
  canvas.width = Math.round((horizontal ? along : RULER_SIZE) * dpr);
  canvas.height = Math.round((horizontal ? RULER_SIZE : along) * dpr);
  const ctx = canvas.getContext("2d");
  if (!ctx || length <= 0) return;
  ctx.scale(dpr, dpr);
  // Draw as if horizontal; the vertical ruler is the same picture rotated.
  if (!horizontal) {
    ctx.translate(RULER_SIZE, 0);
    ctx.rotate(Math.PI / 2);
  }
  const pxPerEmu = length / extent;
  const toPx = (emu: number) => start + emu * pxPerEmu;
  ctx.fillStyle = "#e9e9e9";
  ctx.fillRect(0, 0, along, RULER_SIZE);
  ctx.fillStyle = "#fbfbfb";
  ctx.fillRect(start, 2, length, RULER_SIZE - 4);
  if (band) {
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(toPx(band[0]), 2, toPx(band[1]) - toPx(band[0]), RULER_SIZE - 4);
    ctx.fillStyle = "#4a7fd6";
    ctx.fillRect(toPx(band[0]) - 0.5, 2, 1, RULER_SIZE - 4);
    ctx.fillRect(toPx(band[1]) - 0.5, 2, 1, RULER_SIZE - 4);
  }
  // Ticks every quarter centimeter (fewer when zoomed out), numbers every centimeter.
  const pxPerCm = EMU_PER_CM * pxPerEmu;
  const labelEvery = pxPerCm >= 22 ? 1 : pxPerCm >= 11 ? 2 : 5;
  const sub = pxPerCm >= 24 ? 4 : pxPerCm >= 10 ? 2 : 1;
  const center = extent / 2;
  const first = Math.floor(-center / EMU_PER_CM) - 1;
  const last = Math.ceil(center / EMU_PER_CM) + 1;
  ctx.strokeStyle = "#8a8a8a";
  ctx.fillStyle = "#555";
  ctx.font = "9px -apple-system, Helvetica, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let cm = first; cm <= last; cm++) {
    for (let k = 0; k < sub; k++) {
      const emu = center + (cm + k / sub) * EMU_PER_CM;
      if (emu < 0 || emu > extent) continue;
      const x = Math.round(toPx(emu)) + 0.5;
      if (k === 0 && cm % labelEvery === 0) {
        if (cm === 0) {
          ctx.moveTo(x, 4);
          ctx.lineTo(x, RULER_SIZE - 4);
        } else {
          ctx.save();
          ctx.translate(x, RULER_SIZE / 2);
          if (!horizontal) ctx.rotate(-Math.PI / 2);
          ctx.fillText(String(Math.abs(cm)), 0, 0.5);
          ctx.restore();
        }
      } else {
        const tick = k === 0 ? 5 : sub === 4 && k === 2 ? 3.5 : 2;
        ctx.moveTo(x, RULER_SIZE / 2 - tick / 2);
        ctx.lineTo(x, RULER_SIZE / 2 + tick / 2);
      }
    }
  }
  ctx.stroke();
}
