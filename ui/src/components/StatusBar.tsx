import type { Status } from "../lib/status";
import type { Command } from "../lib/commands";

export function StatusBar({ status, fontsLoading, run }: { status: Status; fontsLoading: boolean; run: (c: Command) => void }) {
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
      <button onClick={() => run({ kind: "zoom", factor: 0.8 })} onMouseDown={(e) => e.preventDefault()} aria-label="縮小">
        −
      </button>
      <button className="zoom-value" onClick={() => run({ kind: "zoom", factor: "fit" })} onMouseDown={(e) => e.preventDefault()} title="符合視窗">
        {Math.round(status.zoom * 100)}%
      </button>
      <button onClick={() => run({ kind: "zoom", factor: 1.25 })} onMouseDown={(e) => e.preventDefault()} aria-label="放大">
        +
      </button>
    </footer>
  );
}
