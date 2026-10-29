import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import init, { createApp, type App as Engine } from "./wasm/ppt_wasm";
import { SlideShow } from "./components/SlideShow";
import { EMOJI_FAMILY, ensureFonts, FALLBACK_FAMILIES } from "./lib/fonts";
import type { ShowCommand, ShowState } from "./lib/show";

/**
 * Root of the audience window: the slides full screen on the second display.
 * It has its own engine with a copy of the deck; the main window owns the
 * show and sends the state to mirror, and this window forwards its input.
 */
export function Audience() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [engine, setEngine] = useState<Engine | null>(null);
  const [state, setState] = useState<ShowState | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current!;
    let raf = 0;
    let disposed = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await init();
      const size = () => {
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.max(1, Math.round(window.innerWidth * dpr));
        canvas.height = Math.max(1, Math.round(window.innerHeight * dpr));
        canvas.style.width = `${window.innerWidth}px`;
        canvas.style.height = `${window.innerHeight}px`;
        return dpr;
      };
      const app = await createApp(canvas, size());
      if (disposed) return;
      const onResize = () => app.resize(canvas.width, canvas.height, size());
      window.addEventListener("resize", onResize);
      cleanups.push(() => window.removeEventListener("resize", onResize));
      const frame = () => {
        try {
          app.render();
        } catch (e) {
          console.error(e);
        }
        raf = requestAnimationFrame(frame);
      };
      raf = requestAnimationFrame(frame);

      app.setFallbackFonts([...FALLBACK_FAMILIES, EMOJI_FAMILY]);
      const deck = new Uint8Array(await invoke<ArrayBuffer>("get_show_deck"));
      app.loadPptx(deck);
      app.startShow(false);
      const unlisten = await listen<ShowState>("show-sync", (e) => {
        app.showSet(e.payload.slide, e.payload.blank, e.payload.transition);
        setState(e.payload);
        // Dev automation only (a no-op otherwise).
        void invoke("automation_log", { line: `audience ${JSON.stringify(e.payload)}` });
      });
      cleanups.push(unlisten);
      const unshot = await listen<string>("audience-shot", async (e) => {
        app.invalidate();
        app.render();
        const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
        if (blob) await invoke("automation_write", new Uint8Array(await blob.arrayBuffer()), { headers: { name: e.payload } });
      });
      cleanups.push(unshot);
      setEngine(app);
      await emitTo("main", "audience-ready");
      // Fonts load progressively; the slides repaint as each one arrives.
      await ensureFonts(app, [...FALLBACK_FAMILIES, ...(app.fontFamilies() as string[])], () => app.invalidate());
    })().catch((e) => console.error("audience window", e));
    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      cleanups.forEach((c) => c());
    };
  }, []);

  const command = (c: ShowCommand) => void emitTo("main", "show-input", c);

  return (
    <div className="audience">
      <canvas ref={canvasRef} />
      {engine && <SlideShow state={state} command={command} />}
    </div>
  );
}
