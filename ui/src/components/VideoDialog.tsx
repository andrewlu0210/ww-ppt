import { t } from "../i18n";
import { useEffect, useRef, useState } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";
import { cancelVideoExport, exportVideo, onVideoEvents, pickSavePath } from "../lib/backend";

const QUALITIES: [number, string][] = [
  [3840, t("video.uhd")],
  [1920, t("video.fullHd")],
  [1280, t("video.hd")],
  [852, t("video.standard480")],
];

interface Props {
  engine: Engine | null;
  /** Document name, for the default file name. */
  name: string;
  onClose: () => void;
}

type Phase = { kind: "setup" } | { kind: "running"; done: number; total: number } | { kind: "finished"; path: string } | { kind: "failed"; error: string };

/**
 * 匯出為視訊: quality, time per slide and recorded timings, as PowerPoint's
 * 建立視訊; then the export runs in the background with a progress bar.
 */
export function VideoDialog({ engine, name, onClose }: Props) {
  const [width, setWidth] = useState(1920);
  const [seconds, setSeconds] = useState(5);
  const [timings, setTimings] = useState(true);
  const [phase, setPhase] = useState<Phase>({ kind: "setup" });
  const running = phase.kind === "running";
  const runningRef = useRef(false);
  runningRef.current = running;

  useEffect(() => {
    const off = onVideoEvents(
      (done, total) => setPhase((p) => (p.kind === "running" ? { kind: "running", done, total } : p)),
      (r) => setPhase(r.ok ? { kind: "finished", path: r.path } : r.cancelled ? { kind: "setup" } : { kind: "failed", error: r.error ?? t("video.unknownError") }),
    );
    return () => void off.then((f) => f());
  }, []);

  // Closing the window mid-export stops it.
  useEffect(() => () => void (runningRef.current && cancelVideoExport()), []);

  const start = async () => {
    if (!engine) return;
    const path = await pickSavePath(name, "mp4");
    if (!path) return;
    setPhase({ kind: "running", done: 0, total: 0 });
    try {
      await exportVideo(path, engine.savePptx(), { width, fps: 30, seconds, timings });
    } catch (e) {
      setPhase({ kind: "failed", error: String(e) });
    }
  };

  const percent = phase.kind === "running" && phase.total > 0 ? Math.floor((phase.done * 100) / phase.total) : 0;

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !running && onClose()}>
      <div className="hf-dialog video-dialog" role="dialog" aria-label={t("ribbon.exportVideo")} aria-modal="true" onKeyDown={(e) => e.key === "Escape" && !running && onClose()}>
        <div className="hf-title">{t("ribbon.exportVideo")}</div>
        <label className="hf-row">
          {t("video.quality")}
          <select className="video-select" value={width} disabled={running} onChange={(e) => setWidth(Number(e.target.value))}>
            {QUALITIES.map(([w, label]) => (
              <option key={w} value={w}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label className="hf-row">
          <input type="checkbox" checked={timings} disabled={running} onChange={(e) => setTimings(e.target.checked)} />
          {t("video.useTimings")}
        </label>
        <label className="hf-row">
          {t("video.secondsPerSlide")}
          <input className="hf-text video-seconds" type="number" min={1} max={600} step={0.5} value={seconds} disabled={running} onChange={(e) => setSeconds(Math.max(0.5, Number(e.target.value) || 5))} />
        </label>
        <div className="hf-note">{t("video.note")}</div>
        {phase.kind === "running" && (
          <div className="video-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-label={t("video.progressAria")}>
            <div className="video-bar" style={{ width: `${percent}%` }} />
            <span>{phase.total ? t("video.progress", { percent, done: phase.done, total: phase.total }) : t("video.preparing")}</span>
          </div>
        )}
        {phase.kind === "finished" && <div className="hf-note video-done">{t("video.exported", { path: phase.path })}</div>}
        {phase.kind === "failed" && <div className="hf-note video-error">{t("video.failed", { error: phase.error })}</div>}
        <div className="hf-buttons">
          {running ? (
            <button onClick={() => void cancelVideoExport()}>{t("save.cancel")}</button>
          ) : (
            <>
              <button onClick={onClose}>{phase.kind === "finished" ? t("common.done") : t("save.cancel")}</button>
              <button className="primary" disabled={!engine} onClick={() => void start()} autoFocus>
                {phase.kind === "finished" ? t("video.exportAgain") : t("video.create")}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
