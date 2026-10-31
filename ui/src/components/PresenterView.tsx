import { locale, t } from "../i18n";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";
import { formatDuration, showKeyCommand, type ShowCommand, type ShowState } from "../lib/show";

/** Elapsed show time; owned by the app so switching views keeps it running. */
export interface ShowTimer {
  /** Accumulated milliseconds before `since`. */
  elapsed: number;
  /** When the timer was last started; null while paused. */
  since: number | null;
}

export function timerMs(t: ShowTimer): number {
  return t.elapsed + (t.since === null ? 0 : Date.now() - t.since);
}

interface Props {
  engine: Engine;
  state: ShowState;
  command: (c: ShowCommand) => void;
  timer: ShowTimer;
  /** ⌥↩ / the switch button; absent when the slides are on another display. */
  onToggleView?: () => void;
}

const NOTES_SIZE_KEY = "ww-ppt.presenterNotesSize";
const NOTES_SIZES = [14, 16, 18, 20, 24, 28, 32, 40];

function loadNotesSize(): number {
  try {
    const v = Number(localStorage.getItem(NOTES_SIZE_KEY));
    return NOTES_SIZES.includes(v) ? v : 20;
  } catch {
    return 20;
  }
}

/**
 * Presenter view: the current slide (drawn by the engine into the canvas
 * under the transparent `.pv-current` box), the next slide, notes and timers.
 */
export function PresenterView({ engine, state, command, timer, onToggleView }: Props) {
  const rootRef = useRef<HTMLDivElement>(null);
  const currentRef = useRef<HTMLDivElement>(null);
  const nextRef = useRef<HTMLCanvasElement>(null);
  const digits = useRef("");
  const [, tick] = useState(0);
  const [notesSize, setNotesSize] = useState(loadNotesSize);

  useEffect(() => {
    rootRef.current?.focus({ preventScroll: true });
    const id = window.setInterval(() => tick((n) => n + 1), 500);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem(NOTES_SIZE_KEY, String(notesSize));
    } catch {
      // Not remembered; fine.
    }
  }, [notesSize]);

  // The engine draws the current slide where the transparent box is.
  useLayoutEffect(() => {
    const box = currentRef.current;
    if (!box) return;
    const place = () => {
      const canvas = document.querySelector<HTMLCanvasElement>(".canvas-host canvas");
      const origin = canvas?.getBoundingClientRect() ?? { left: 0, top: 0 };
      const r = box.getBoundingClientRect();
      engine.setShowViewport(r.left - origin.left, r.top - origin.top, r.width, r.height);
    };
    place();
    const ro = new ResizeObserver(place);
    ro.observe(box);
    window.addEventListener("resize", place);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", place);
      engine.setShowViewport(0, 0, 0, 0);
    };
  }, [engine]);

  // Next slide preview.
  useEffect(() => {
    const canvas = nextRef.current;
    if (!canvas) return;
    let cancelled = false;
    const ctx = canvas.getContext("2d");
    if (state.next < 0) {
      ctx?.clearRect(0, 0, canvas.width, canvas.height);
      return;
    }
    const width = Math.max(64, Math.round(canvas.clientWidth * (window.devicePixelRatio || 1)));
    void (engine.renderThumbnail(state.next, width, false) as Promise<{ width: number; height: number; pixels: Uint8Array }>).then((t) => {
      if (cancelled || !ctx) return;
      canvas.width = t.width;
      canvas.height = t.height;
      ctx.putImageData(new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height), 0, 0);
    });
    return () => {
      cancelled = true;
    };
  }, [engine, state.next]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    const c = showKeyCommand(e, digits);
    if (c === "toggleView") onToggleView?.();
    else if (c) command(c);
  };

  const elapsed = formatDuration(timerMs(timer) / 1000);
  const paused = timer.since === null;
  const clock = new Date().toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  const notes = state.slide >= 0 ? engine.notes(state.slide) : "";
  const [slideW, slideH] = engine.slideSize();
  const aspectRatio = `${slideW} / ${slideH}`;
  const step = (dir: number) => {
    const i = NOTES_SIZES.indexOf(notesSize) + dir;
    setNotesSize(NOTES_SIZES[Math.max(0, Math.min(NOTES_SIZES.length - 1, i))]);
  };
  const keep = (e: React.MouseEvent) => e.preventDefault();

  return (
    <div ref={rootRef} className="presenter" tabIndex={0} onKeyDown={onKeyDown} onContextMenu={(e) => e.preventDefault()} aria-label={t("ribbon.presenterView")}>
      <header className="pv-bar">
        <span className="pv-timer" aria-label={t("presenter.elapsed")}>
          {elapsed}
        </span>
        <button
          onMouseDown={keep}
          onClick={() => {
            if (paused) timer.since = Date.now();
            else {
              timer.elapsed = timerMs(timer);
              timer.since = null;
            }
            tick((n) => n + 1);
          }}
        >
          {paused ? t("presenter.resume") : t("presenter.pause")}
        </button>
        <button
          onMouseDown={keep}
          onClick={() => {
            timer.elapsed = 0;
            timer.since = paused ? null : Date.now();
            tick((n) => n + 1);
          }}
        >
          {t("ribbon.reset")}
        </button>
        <span className="pv-clock">{clock}</span>
        <span className="spacer" />
        {onToggleView && (
          <button onMouseDown={keep} onClick={onToggleView} title="⌥↩">
            {t("presenter.switchToShow")}
          </button>
        )}
        <button className="pv-end" onMouseDown={keep} onClick={() => command({ name: "end" })}>
          {t("presenter.endShow")}
        </button>
      </header>
      <div className="pv-main">
        <section className="pv-left">
          <div
            ref={currentRef}
            className="pv-current"
            style={{ aspectRatio }}
            onPointerDown={(e) => {
              if (e.button === 0 && state.onClick) command({ name: "next" });
            }}
          >
            {state.slide === -1 && !state.blank && <div className="pv-end-label">{t("presenter.showEnded")}</div>}
          </div>
          <div className="pv-controls">
            <button onMouseDown={keep} onClick={() => command({ name: "prev" })} aria-label={t("presenter.previous")}>
              ◀
            </button>
            <span className="pv-count">
              {state.slide >= 0 ? t("status.slide", { current: state.slide + 1, total: state.count }) : t("presenter.endScreen")}
              {state.slide >= 0 && state.steps > 0 && <span className="pv-steps">{t("presenter.steps", { step: state.step, steps: state.steps })}</span>}
            </span>
            <button onMouseDown={keep} onClick={() => command({ name: "next" })} aria-label={t("presenter.next")}>
              ▶
            </button>
            <button className={state.blank === "black" ? "on" : ""} onMouseDown={keep} onClick={() => command({ name: "black" })} title="B">
              {t("presenter.black")}
            </button>
          </div>
        </section>
        <section className="pv-right">
          <div className="pv-label">{t("presenter.next")}</div>
          <div className="pv-next" style={{ aspectRatio }}>
            <canvas ref={nextRef} />
            {state.next < 0 && <div className="pv-next-end">{state.slide === -1 ? "" : t("presenter.showEnded")}</div>}
          </div>
          <div className="pv-notes-head">
            <span className="pv-label">{t("ribbon.notes")}</span>
            <span className="spacer" />
            <button onMouseDown={keep} onClick={() => step(-1)} aria-label={t("presenter.smallerNotes")}>
              A−
            </button>
            <button onMouseDown={keep} onClick={() => step(1)} aria-label={t("presenter.largerNotes")}>
              A+
            </button>
          </div>
          <div className="pv-notes" style={{ fontSize: notesSize }}>
            {notes || <span className="pv-muted">{t("presenter.noNotes")}</span>}
          </div>
        </section>
      </div>
    </div>
  );
}
