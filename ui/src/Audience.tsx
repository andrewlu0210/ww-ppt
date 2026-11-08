import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import init, { createApp, setLanguage, type App as Engine } from "./wasm/ppt_wasm";
import { lang } from "./i18n";
import { SlideShow } from "./components/SlideShow";
import { EMOJI_FAMILY, ensureFonts, FALLBACK_FAMILIES } from "./lib/fonts";
import type { ShowCommand, ShowState } from "./lib/show";
import { domSlideBox, ShowMedia, type ControlMsg } from "./lib/showMedia";

/**
 * Root of the audience window: the slides full screen on the second display.
 * It has its own engine with a copy of the deck; the main window owns the
 * show and sends the state to mirror, and this window forwards its input.
 */
export function Audience() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [engine, setEngine] = useState<Engine | null>(null);
  const [state, setState] = useState<ShowState | null>(null);
  // Videos of the show, silent: the main window plays the sound.
  const mediaRef = useRef<ShowMedia | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current!;
    let raf = 0;
    let disposed = false;
    const cleanups: (() => void)[] = [];
    (async () => {
      await init();
      setLanguage(lang);
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
      const [slideW, slideH] = Array.from(app.slideSize());
      const media = new ShowMedia(app, domSlideBox(slideW / Math.max(1, slideH)), { audio: false, muted: true });
      mediaRef.current = media;
      cleanups.push(() => media.stop());
      const unlisten = await listen<ShowState>("show-sync", (e) => {
        app.showSet(e.payload.slide, e.payload.blank, e.payload.transition, e.payload.step, e.payload.stepAnimate);
        setState(e.payload);
        // Dev automation only (a no-op otherwise).
        void invoke("automation_log", { line: `audience ${JSON.stringify(e.payload)}` });
      });
      cleanups.push(unlisten);
      // The main window's viewer paused, played or moved a video: this window follows.
      const unmedia = await listen<ControlMsg>("show-media", (e) => media.apply(e.payload));
      cleanups.push(unmedia);
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

  // Videos follow the show the main window sends (and are tried again once the screen is up).
  useEffect(() => {
    const media = mediaRef.current;
    if (!engine || !media || !state) return;
    media.update(state.slide, state.step, state.steps);
    media.setBlank(state.blank !== "");
    // Dev automation only (a no-op otherwise).
    const videos = [...document.querySelectorAll<HTMLVideoElement>(".show-video")].map((v) => ({ rect: [v.offsetLeft, v.offsetTop, v.offsetWidth, v.offsetHeight].map(Math.round), t: Number(v.currentTime.toFixed(2)), paused: v.paused, muted: v.muted }));
    void invoke("automation_log", { line: `audience-media ${JSON.stringify({ ...media.describe(), videos })}` });
  }, [engine, state]);

  const command = (c: ShowCommand) => void emitTo("main", "show-input", c);

  return (
    <div className="audience">
      <canvas ref={canvasRef} />
      {engine && <SlideShow state={state} command={command} />}
    </div>
  );
}
