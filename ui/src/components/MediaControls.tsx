import { useEffect, useReducer, useRef } from "react";
import { t } from "../i18n";
import type { App as Engine } from "../wasm/ppt_wasm";
import { EditMedia } from "../lib/editMedia";
import { canvasSlideBox } from "../lib/showMedia";
import type { Status } from "../lib/status";

const BAR_HEIGHT = 34;
const GAP = 6;
const MIN_WIDTH = 280;

function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/**
 * Plays the selected video in place: a bar under the picture (or inside it when there is no room
 * below) with play / pause, position, time and volume. The video is put over the picture by
 * `EditMedia`; nothing is read from the file until the first play.
 */
export function MediaControls({ engine, status }: { engine: Engine; status: Status }) {
  const media = useRef<EditMedia | null>(null);
  const [, redraw] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    const m = new EditMedia(engine, canvasSlideBox(engine));
    media.current = m;
    const off = m.subscribe(redraw);
    return () => {
      off();
      m.stop();
      media.current = null;
    };
  }, [engine]);

  // Space plays or pauses the selected video while the canvas has the keyboard (nothing else uses it then).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (e.key !== " " || e.metaKey || e.ctrlKey || e.altKey || !target?.classList.contains("ime-input")) return;
      const m = media.current;
      if (!m?.state().available || (engine.status() as Status).editingText) return;
      e.preventDefault();
      e.stopPropagation();
      m.toggle();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [engine]);

  // Follows the selection, and the picture's frame and the slide's place on every change.
  const scroll = status.scroll.join(",");
  const seen = useRef({ slide: -1, shape: 0, frame: status.frame });
  useEffect(() => {
    // Moving, resizing or rotating the picture (a drag, the pane, an undo) pauses the video, which
    // then follows the picture's new place.
    const was = seen.current;
    if (was.slide === status.slideIndex && was.shape === status.selectedShapeId && was.frame !== status.frame) media.current?.pause();
    seen.current = { slide: status.slideIndex, shape: status.selectedShapeId, frame: status.frame };
    media.current?.select(status.slideIndex, status.selectedShapeId);
    redraw();
  }, [status.slideIndex, status.selectedShapeId, status.revision, status.zoom, scroll, status.frame]);

  const m = media.current;
  const state = m?.state();
  const frame = m?.frame();
  const host = document.querySelector<HTMLElement>(".canvas-host");
  if (!m || !state?.available || !frame || !host) return null;

  const width = Math.min(Math.max(frame.width, MIN_WIDTH), host.clientWidth);
  const left = Math.max(0, Math.min(frame.left + (frame.width - width) / 2, host.clientWidth - width));
  const below = frame.top + frame.height + GAP;
  // Under the picture when it fits; otherwise at the bottom edge of the picture, inside it.
  const top = below + BAR_HEIGHT <= host.clientHeight ? below : Math.max(0, Math.min(frame.top + frame.height - BAR_HEIGHT - GAP, host.clientHeight - BAR_HEIGHT));
  const shown = state.muted ? 0 : state.volume;

  // Keeps keyboard focus on the canvas when a button is pressed.
  const keepFocus = (e: React.MouseEvent) => e.preventDefault();

  return (
    <div
      className="media-controls"
      role="group"
      aria-label={t("media.controls")}
      style={{ left: host.offsetLeft + left, top: host.offsetTop + top, width }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <button type="button" onMouseDown={keepFocus} onClick={() => m.toggle()} aria-label={state.playing ? t("media.pause") : t("media.play")} title={state.playing ? t("media.pause") : t("media.play")}>
        {state.playing ? (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true">
            <rect x="3" y="2" width="3" height="10" rx="0.5" />
            <rect x="8" y="2" width="3" height="10" rx="0.5" />
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true">
            <path d="M3.5 2l8 5-8 5z" />
          </svg>
        )}
      </button>
      <span className="media-time">{clock(state.time)}</span>
      <input
        type="range"
        className="media-seek"
        min={0}
        max={Math.max(state.length, 0.01)}
        step={0.1}
        value={Math.min(state.time, Math.max(state.length, 0.01))}
        disabled={state.length <= 0 && state.loaded}
        aria-label={t("media.position")}
        aria-valuetext={`${clock(state.time)} / ${clock(state.length)}`}
        onChange={(e) => m.seek(Number(e.target.value))}
      />
      <span className="media-time">{clock(state.length)}</span>
      <button type="button" onMouseDown={keepFocus} onClick={() => m.setMuted(!state.muted)} aria-label={state.muted ? t("media.unmute") : t("media.mute")} title={state.muted ? t("media.unmute") : t("media.mute")}>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true">
          <path d="M2 5h2.5L8 2v10L4.5 9H2z" />
          {shown > 0 ? <path d="M10 4.5a3.5 3.5 0 010 5" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" /> : <path d="M10 5l3 4M13 5l-3 4" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />}
        </svg>
      </button>
      <input type="range" className="media-volume" min={0} max={1} step={0.05} value={shown} aria-label={t("media.volume")} onChange={(e) => m.setVolume(Number(e.target.value))} />
    </div>
  );
}
