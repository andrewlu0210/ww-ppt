import { useEffect, useRef, useState } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";
import { cancelVideoExport, exportVideo, onVideoEvents, pickSavePath } from "../lib/backend";

const QUALITIES: [number, string][] = [
  [3840, "Ultra HD (4K)：3840 × 2160"],
  [1920, "Full HD (1080p)：1920 × 1080"],
  [1280, "HD (720p)：1280 × 720"],
  [852, "標準 (480p)：852 × 480"],
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
      (r) => setPhase(r.ok ? { kind: "finished", path: r.path } : r.cancelled ? { kind: "setup" } : { kind: "failed", error: r.error ?? "未知的錯誤" }),
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
      <div className="hf-dialog video-dialog" role="dialog" aria-label="匯出為視訊" aria-modal="true" onKeyDown={(e) => e.key === "Escape" && !running && onClose()}>
        <div className="hf-title">匯出為視訊</div>
        <label className="hf-row">
          品質
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
          使用錄製的時間（投影片有設定「之後」時）
        </label>
        <label className="hf-row">
          每張投影片的秒數
          <input className="hf-text video-seconds" type="number" min={1} max={600} step={0.5} value={seconds} disabled={running} onChange={(e) => setSeconds(Math.max(0.5, Number(e.target.value) || 5))} />
        </label>
        <div className="hf-note">轉場與動畫會依序自動播放；隱藏的投影片不會匯出。</div>
        {phase.kind === "running" && (
          <div className="video-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-label="匯出進度">
            <div className="video-bar" style={{ width: `${percent}%` }} />
            <span>{phase.total ? `${percent}%（${phase.done} / ${phase.total} 格）` : "準備中…"}</span>
          </div>
        )}
        {phase.kind === "finished" && <div className="hf-note video-done">已匯出到 {phase.path}</div>}
        {phase.kind === "failed" && <div className="hf-note video-error">無法匯出視訊：{phase.error}</div>}
        <div className="hf-buttons">
          {running ? (
            <button onClick={() => void cancelVideoExport()}>取消</button>
          ) : (
            <>
              <button onClick={onClose}>{phase.kind === "finished" ? "完成" : "取消"}</button>
              <button className="primary" disabled={!engine} onClick={() => void start()} autoFocus>
                {phase.kind === "finished" ? "再匯出一次…" : "建立視訊…"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
