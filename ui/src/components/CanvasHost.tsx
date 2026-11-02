import { lang, t } from "../i18n";
import { useEffect, useRef } from "react";
import init, { createApp, setLanguage, type App } from "../wasm/ppt_wasm";
import { SHAPES_MARKER } from "../lib/commands";
import { setWasmMemory } from "../lib/perf";

/** Returns keyboard focus to the canvas input (after toolbar clicks, dialogs, ...). */
export function focusCanvas() {
  document.querySelector<HTMLTextAreaElement>(".ime-input")?.focus({ preventScroll: true });
}

interface Props {
  onReady: (app: App) => void;
  onError: (message: string) => void;
  /// Called after any interaction that may change UI state.
  onChange: () => void;
  /** Scroll bars `[x ratio, x position, y ratio, y position]`; a ratio of 1 hides that bar. */
  scroll?: number[];
}

const DOUBLE_CLICK_MS = 450;
const DOUBLE_CLICK_PX = 5;

/**
 * Hosts the WebGPU canvas and the hidden textarea that receives keyboard and
 * IME input. The textarea follows the text caret so the macOS candidate window
 * appears next to the text being composed.
 */
export function CanvasHost({ onReady, onError, onChange, scroll }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const labelRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<App | null>(null);

  useEffect(() => {
    const container = containerRef.current!;
    const canvas = canvasRef.current!;
    const input = inputRef.current!;
    let disposed = false;
    let raf = 0;
    const cleanups: (() => void)[] = [];

    const sizeCanvas = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, Math.round(container.clientWidth * dpr));
      const h = Math.max(1, Math.round(container.clientHeight * dpr));
      canvas.width = w;
      canvas.height = h;
      canvas.style.width = `${container.clientWidth}px`;
      canvas.style.height = `${container.clientHeight}px`;
      return { w, h, dpr };
    };

    // Keeps the textarea on the caret so IME candidates show up in the right place.
    const placeInput = () => {
      const app = appRef.current;
      const r = app?.imeRect();
      if (r && r.length === 4) {
        input.style.left = `${r[0]}px`;
        input.style.top = `${r[1]}px`;
        input.style.height = `${Math.max(12, r[3])}px`;
        input.style.fontSize = `${Math.max(10, r[3] * 0.8)}px`;
      }
    };

    const changed = () => {
      placeInput();
      onChange();
    };

    (async () => {
      if (!("gpu" in navigator)) {
        onError(t("canvas.noWebGpu"));
        return;
      }
      setWasmMemory((await init()).memory);
      setLanguage(lang);
      const { dpr } = sizeCanvas();
      let app: App;
      try {
        app = await createApp(canvas, dpr);
      } catch (e) {
        onError(t("canvas.initFailed", { error: String(e) }));
        return;
      }
      if (disposed) return;
      appRef.current = app;

      const ro = new ResizeObserver(() => {
        const { w, h, dpr } = sizeCanvas();
        app.resize(w, h, dpr);
        // Draw right away: resizing clears the canvas, and waiting for the next frame shows
        // stale or blank content while the window is dragged.
        try {
          app.render();
        } catch (e) {
          console.error(e);
        }
        placeInput();
        // The absolute zoom shown in the status bar follows the window size.
        onChange();
      });
      ro.observe(container);
      cleanups.push(() => ro.disconnect());

      const frame = () => {
        try {
          if (app.render()) placeInput();
        } catch (e) {
          console.error(e);
        }
        raf = requestAnimationFrame(frame);
      };
      raf = requestAnimationFrame(frame);

      const blink = window.setInterval(() => app.blink(), 530);
      cleanups.push(() => window.clearInterval(blink));

      // --- Pointer ---------------------------------------------------------
      let last = { t: 0, x: 0, y: 0, clicks: 0 };
      let dragging = false;
      const local = (e: PointerEvent) => {
        const rect = canvas.getBoundingClientRect();
        return { x: e.clientX - rect.left, y: e.clientY - rect.top };
      };
      const onDown = (e: PointerEvent) => {
        if (e.button !== 0) return;
        e.preventDefault();
        const { x, y } = local(e);
        const now = performance.now();
        const near = Math.hypot(x - last.x, y - last.y) < DOUBLE_CLICK_PX;
        const clicks = now - last.t < DOUBLE_CLICK_MS && near ? last.clicks + 1 : 1;
        last = { t: now, x, y, clicks };
        canvas.setPointerCapture(e.pointerId);
        dragging = true;
        app.pointerDown(x, y, e.shiftKey, e.altKey, e.metaKey, clicks);
        input.focus({ preventScroll: true });
        changed();
      };
      const onMove = (e: PointerEvent) => {
        const { x, y } = local(e);
        if (dragging) {
          if (app.pointerMove(x, y, e.shiftKey, e.altKey, e.metaKey)) {
            placeInput();
            onChange();
          }
        }
        canvas.style.cursor = app.cursor(x, y);
        showGuideLabel(dragging ? app.guideLabel() : "", x, y);
      };
      // The distance of a dragged guide from the slide's center, next to the pointer.
      const showGuideLabel = (text: string, x: number, y: number) => {
        const label = labelRef.current;
        if (!label) return;
        label.hidden = !text;
        label.textContent = text;
        label.style.left = `${x + 14}px`;
        label.style.top = `${y + 14}px`;
      };
      const onUp = (e: PointerEvent) => {
        if (!dragging) return;
        dragging = false;
        const { x, y } = local(e);
        app.pointerUp(x, y, e.shiftKey, e.altKey, e.metaKey);
        showGuideLabel("", 0, 0);
        changed();
      };
      // Scrolls a zoomed-in slide with the wheel or a two-finger swipe.
      const onWheel = (e: WheelEvent) => {
        e.preventDefault();
        const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? canvas.clientHeight : 1;
        app.scrollBy(e.deltaX * unit, e.deltaY * unit);
        onChange();
      };
      // A trackpad pinch arrives as a wheel event with ctrl held (and as WebKit gesture events).
      const pinch = (factor: number, x: number, y: number) => {
        const r = canvas.getBoundingClientRect();
        app.zoomAt(factor, x - r.left, y - r.top);
        onChange();
      };
      const onWheelPinch = (e: WheelEvent) => {
        if (!e.ctrlKey) return onWheel(e);
        e.preventDefault();
        pinch(Math.exp(-e.deltaY * 0.01), e.clientX, e.clientY);
      };
      let gestureScale = 1;
      const onGestureStart = (e: Event) => {
        e.preventDefault();
        gestureScale = 1;
      };
      const onGestureChange = (e: Event) => {
        e.preventDefault();
        const g = e as unknown as { scale: number; clientX: number; clientY: number };
        pinch(g.scale / gestureScale, g.clientX, g.clientY);
        gestureScale = g.scale;
      };
      canvas.addEventListener("wheel", onWheelPinch, { passive: false });
      canvas.addEventListener("gesturestart", onGestureStart);
      canvas.addEventListener("gesturechange", onGestureChange);
      canvas.addEventListener("gestureend", onGestureStart);
      cleanups.push(() => {
        canvas.removeEventListener("wheel", onWheelPinch);
        canvas.removeEventListener("gesturestart", onGestureStart);
        canvas.removeEventListener("gesturechange", onGestureChange);
        canvas.removeEventListener("gestureend", onGestureStart);
      });
      canvas.addEventListener("pointerdown", onDown);
      canvas.addEventListener("pointermove", onMove);
      canvas.addEventListener("pointerup", onUp);
      canvas.addEventListener("pointercancel", onUp);
      cleanups.push(() => {
        canvas.removeEventListener("pointerdown", onDown);
        canvas.removeEventListener("pointermove", onMove);
        canvas.removeEventListener("pointerup", onUp);
        canvas.removeEventListener("pointercancel", onUp);
      });

      // --- Keyboard & IME --------------------------------------------------
      const onKeyDown = (e: KeyboardEvent) => {
        // Keys pressed while composing belong to the IME (e.g. Enter picks a candidate).
        if (e.isComposing || e.keyCode === 229) return;
        if (app.keyDown(e.key, e.shiftKey, e.altKey, e.metaKey, e.ctrlKey)) {
          e.preventDefault();
          changed();
        }
      };
      const onCompositionUpdate = (e: CompositionEvent) => {
        // WebKit may fire this before the textarea updates; -1 means "end of preedit".
        const caret = input.value === e.data ? (input.selectionEnd ?? -1) : -1;
        app.composeUpdate(e.data ?? "", caret);
        changed();
      };
      const onCompositionEnd = (e: CompositionEvent) => {
        app.composeEnd(e.data ?? "");
        input.value = "";
        changed();
      };
      const onInput = (e: Event) => {
        const ev = e as InputEvent;
        // Composition text arrives through the composition events above.
        if (ev.isComposing || ev.inputType.includes("Composition")) return;
        if (ev.inputType === "insertText" && ev.data) app.insertText(ev.data);
        input.value = "";
        changed();
      };
      const onPaste = async (e: ClipboardEvent) => {
        e.preventDefault();
        const data = e.clipboardData;
        const text = data?.getData("text/plain") ?? "";
        const image = Array.from(data?.files ?? []).find((f) => f.type.startsWith("image/"));
        if (app.isEditingText()) {
          if (text && text !== SHAPES_MARKER) app.insertText(text.replace(/\r\n?/g, "\n"));
        } else if (image) {
          app.insertPicture(new Uint8Array(await image.arrayBuffer()), image.type.split("/")[1] ?? "png");
        } else if (text === SHAPES_MARKER || (!text && (app.status() as { canPaste: boolean }).canPaste)) {
          app.pasteShapes();
        } else if (text) {
          app.pasteTextBox(text.replace(/\r\n?/g, "\n"));
        }
        changed();
      };
      const onCopy = (e: ClipboardEvent) => {
        if (app.isEditingText()) {
          const text = app.selectedText();
          if (text) {
            e.clipboardData?.setData("text/plain", text);
            e.preventDefault();
          }
        } else if (app.copyShapes() > 0) {
          e.clipboardData?.setData("text/plain", SHAPES_MARKER);
          e.preventDefault();
        }
      };
      const onCut = (e: ClipboardEvent) => {
        const editing = app.isEditingText();
        onCopy(e);
        if (editing ? app.deleteTextSelection() : app.deleteSelected()) changed();
      };
      input.addEventListener("keydown", onKeyDown);
      input.addEventListener("compositionupdate", onCompositionUpdate);
      input.addEventListener("compositionend", onCompositionEnd);
      input.addEventListener("input", onInput);
      input.addEventListener("paste", onPaste);
      input.addEventListener("copy", onCopy);
      input.addEventListener("cut", onCut);
      cleanups.push(() => {
        input.removeEventListener("keydown", onKeyDown);
        input.removeEventListener("compositionupdate", onCompositionUpdate);
        input.removeEventListener("compositionend", onCompositionEnd);
        input.removeEventListener("input", onInput);
        input.removeEventListener("paste", onPaste);
        input.removeEventListener("copy", onCopy);
        input.removeEventListener("cut", onCut);
      });

      input.focus({ preventScroll: true });
      onReady(app);
    })();

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      cleanups.forEach((c) => c());
    };
    // The canvas and its app live for the whole session.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Scroll bar drag: `dx`/`dy` in CSS px of slide content. */
  const scrollApp = (dx: number, dy: number) => {
    appRef.current?.scrollBy(dx, dy);
    onChange();
  };

  return (
    <div className="canvas-host" ref={containerRef}>
      {/* Read through the slide layer (A11yMirror) instead. */}
      <canvas ref={canvasRef} aria-hidden="true" />
      <div ref={labelRef} className="guide-label" hidden />
      {scroll && scroll.length === 4 && (
        <>
          {scroll[0] < 1 && <ScrollBar axis="x" ratio={scroll[0]} position={scroll[1]} scrollBy={(d) => scrollApp(d, 0)} />}
          {scroll[2] < 1 && <ScrollBar axis="y" ratio={scroll[2]} position={scroll[3]} scrollBy={(d) => scrollApp(0, d)} />}
        </>
      )}
      <textarea
        ref={inputRef}
        className="ime-input"
        aria-label={t("canvas.textInput")}
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
      />
    </div>
  );
}

/** An overlay scroll bar; dragging the thumb scrolls the zoomed-in slide. */
function ScrollBar({ axis, ratio, position, scrollBy }: { axis: "x" | "y"; ratio: number; position: number; scrollBy: (content: number) => void }) {
  const track = useRef<HTMLDivElement>(null);
  const horizontal = axis === "x";
  const onDown = (e: React.PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const el = track.current;
    if (!el) return;
    const length = horizontal ? el.clientWidth : el.clientHeight;
    let last = horizontal ? e.clientX : e.clientY;
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const now = horizontal ? m.clientX : m.clientY;
      // Moving the thumb by the whole free track length scrolls the whole content range.
      scrollBy(((now - last) / Math.max(1, length * (1 - ratio))) * (length / ratio - length));
      last = now;
    };
    const up = () => {
      target.removeEventListener("pointermove", move);
      target.removeEventListener("pointerup", up);
      target.removeEventListener("pointercancel", up);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerup", up);
    target.addEventListener("pointercancel", up);
  };
  const style = horizontal ? { left: `${position * (1 - ratio) * 100}%`, width: `${ratio * 100}%` } : { top: `${position * (1 - ratio) * 100}%`, height: `${ratio * 100}%` };
  return (
    <div ref={track} className={`scrollbar scrollbar-${axis}`} aria-hidden="true">
      <div className="scrollbar-thumb" style={style} onPointerDown={onDown} />
    </div>
  );
}
