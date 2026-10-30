import type { Status } from "../lib/status";
import type { Command } from "../lib/commands";

interface Props {
  status: Status;
  fontsLoading: boolean;
  notesOpen: boolean;
  viewMode: "normal" | "sorter";
  /** Thumbnail scale in the slide sorter (1 = 100%). */
  sorterZoom: number;
  run: (c: Command) => void;
}

export function StatusBar({ status, fontsLoading, notesOpen, viewMode, sorterZoom, run }: Props) {
  const view = (mode: "normal" | "sorter", label: string) => (
    <button className={viewMode === mode ? "toggle on" : "toggle"} onClick={() => run({ kind: "viewMode", mode })} onMouseDown={(e) => e.preventDefault()} aria-pressed={viewMode === mode}>
      {label}
    </button>
  );
  return (
    <footer className="status-bar">
      <span>
        投影片 {status.slideCount ? status.slideIndex + 1 : 0} / {status.slideCount}
      </span>
      {status.editingText && <span>編輯文字</span>}
      {!status.editingText && status.selectionName && <span>已選取：{status.selectionName}</span>}
      {fontsLoading && <span>正在載入字型…</span>}
      <span className="spacer" />
      <span className="muted">WebGPU · vello · {status.frameMs.toFixed(1)} ms</span>
      <button className={notesOpen ? "toggle on" : "toggle"} onClick={() => run({ kind: "toggleNotes" })} onMouseDown={(e) => e.preventDefault()} aria-pressed={notesOpen} title="顯示或隱藏備忘稿">
        備忘稿
      </button>
      {view("normal", "標準")}
      {view("sorter", "投影片瀏覽")}
      <button onClick={() => run({ kind: "zoom", factor: 0.8 })} onMouseDown={(e) => e.preventDefault()} aria-label="縮小">
        −
      </button>
      <button className="zoom-value" onClick={() => run({ kind: "zoom", factor: "fit" })} onMouseDown={(e) => e.preventDefault()} title="符合視窗">
        {Math.round((viewMode === "sorter" ? sorterZoom : status.zoom) * 100)}%
      </button>
      <button onClick={() => run({ kind: "zoom", factor: 1.25 })} onMouseDown={(e) => e.preventDefault()} aria-label="放大">
        +
      </button>
    </footer>
  );
}
