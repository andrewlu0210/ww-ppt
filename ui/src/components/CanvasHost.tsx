import { useEffect, useRef } from "react";
import init, { createApp, type App } from "../wasm/ppt_wasm";
import { SHAPES_MARKER } from "../lib/commands";

/** Returns keyboard focus to the canvas input (after toolbar clicks, dialogs, ...). */
export function focusCanvas() {
  document.querySelector<HTMLTextAreaElement>(".ime-input")?.focus({ preventScroll: true });
}

interface Props {
  onReady: (app: App) => void;
  onError: (message: string) => void;
  /// Called after any interaction that may change UI state.
  onChange: () => void;
}

const DOUBLE_CLICK_MS = 450;
const DOUBLE_CLICK_PX = 5;

/**
 * Hosts the WebGPU canvas and the hidden textarea that receives keyboard and
 * IME input. The textarea follows the text caret so the macOS candidate window
 * appears next to the text being composed.
 */
export function CanvasHost({ onReady, onError, onChange }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
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
        onError("此 WebView 不支援 WebGPU（需要 macOS 26 以上）。");
        return;
      }
      await init();
      const { dpr } = sizeCanvas();
      let app: App;
      try {
        app = await createApp(canvas, dpr);
      } catch (e) {
        onError(`無法初始化 WebGPU 渲染器：${String(e)}`);
        return;
      }
      if (disposed) return;
      appRef.current = app;

      const ro = new ResizeObserver(() => {
        const { w, h, dpr } = sizeCanvas();
        app.resize(w, h, dpr);
        placeInput();
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
      };
      const onUp = (e: PointerEvent) => {
        if (!dragging) return;
        dragging = false;
        const { x, y } = local(e);
        app.pointerUp(x, y, e.shiftKey, e.altKey, e.metaKey);
        changed();
      };
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

  return (
    <div className="canvas-host" ref={containerRef}>
      <canvas ref={canvasRef} />
      <textarea
        ref={inputRef}
        className="ime-input"
        aria-label="投影片文字輸入"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
      />
    </div>
  );
}
