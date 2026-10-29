import type { ReactNode } from "react";
import type { Command } from "../lib/commands";
import type { Status, TransitionInfo } from "../lib/status";

/** Transition types in gallery order, with PowerPoint's default durations (ms). */
const TYPES: { id: string; label: string; ms: number; icon: ReactNode }[] = [
  { id: "none", label: "無", ms: 0, icon: <path d="M6 6l12 12" /> },
  { id: "cut", label: "切換", ms: 100, icon: <path d="M12 4v16" /> },
  { id: "fade", label: "淡出", ms: 700, icon: <rect x="6" y="7" width="12" height="10" strokeDasharray="2 2" /> },
  { id: "push", label: "推入", ms: 1000, icon: <path d="M12 18V7M8.5 10.5 12 7l3.5 3.5" /> },
  { id: "wipe", label: "擦去", ms: 1000, icon: <path d="M17 6v12M7 12h8M12 9l3 3-3 3" /> },
  { id: "cover", label: "覆蓋", ms: 1000, icon: <path d="M10 8h9v9h-9zM5 6h9" /> },
  { id: "pull", label: "揭露", ms: 1000, icon: <path d="M5 8h9v9H5zM10 6h9" /> },
  { id: "split", label: "分割", ms: 1500, icon: <path d="M12 5v14M9 12H5M15 12h4M7 10l-2 2 2 2M17 10l2 2-2 2" /> },
  { id: "dissolve", label: "溶解", ms: 700, icon: <path d="M7 7h2v2H7zM12 9h2v2h-2zM9 13h2v2H9zM15 14h2v2h-2zM16 7h1v1h-1z" /> },
  { id: "randomBar", label: "隨機線條", ms: 1000, icon: <path d="M7 6v12M9.5 6v12M14 6v12M17 6v12" /> },
  { id: "zoom", label: "縮放", ms: 500, icon: <path d="M9 9h6v6H9zM5 5l3 3M19 5l-3 3M5 19l3-3M19 19l-3-3" /> },
  { id: "circle", label: "圓形", ms: 1000, icon: <circle cx="12" cy="12" r="6" /> },
];

/** Effect options per type: [option id, label]. Directions are where the slide moves. */
const OPTIONS: Record<string, [string, string][]> = {
  cut: [
    ["", "平滑"],
    ["thruBlk", "穿過黑色"],
  ],
  fade: [
    ["", "平滑"],
    ["thruBlk", "穿過黑色"],
  ],
  push: [
    ["u", "從下"],
    ["l", "從右"],
    ["d", "從上"],
    ["r", "從左"],
  ],
  wipe: [
    ["l", "從右"],
    ["u", "從下"],
    ["r", "從左"],
    ["d", "從上"],
  ],
  cover: [
    ["l", "從右"],
    ["u", "從下"],
    ["r", "從左"],
    ["d", "從上"],
    ["ld", "從右上"],
    ["lu", "從右下"],
    ["rd", "從左上"],
    ["ru", "從左下"],
  ],
  split: [
    ["vertOut", "垂直向外"],
    ["vertIn", "垂直向內"],
    ["horzOut", "水平向外"],
    ["horzIn", "水平向內"],
  ],
  randomBar: [
    ["vert", "垂直"],
    ["horz", "水平"],
  ],
  zoom: [
    ["in", "拉近"],
    ["out", "拉遠"],
  ],
};
OPTIONS.pull = OPTIONS.cover;

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="3" y="3" width="18" height="18" rx="2" strokeOpacity="0.35" />
    {children}
  </svg>
);

const keepFocus = (e: React.MouseEvent) => e.preventDefault();

export function TransitionTab({ status, run }: { status: Status; run: (c: Command) => void }) {
  const t: TransitionInfo | undefined = status.transition;
  const id = t?.id ?? "none";
  const set = (patch: Partial<TransitionInfo>, all = false) => {
    const next: TransitionInfo = { id, option: t?.option ?? "", durationMs: t?.durationMs ?? 700, onClick: t?.onClick ?? true, afterMs: t?.afterMs ?? -1, ...patch };
    run({ kind: "setTransition", ...next, all });
  };
  const choose = (type: (typeof TYPES)[number]) => {
    // A new type starts with its default option and duration, keeping the advance settings.
    set({ id: type.id, option: OPTIONS[type.id]?.[0][0] ?? "", durationMs: type.ms });
    if (type.id !== "none") run({ kind: "previewTransition" });
  };
  const options = OPTIONS[id];
  const known = TYPES.some((x) => x.id === id);

  return (
    <>
      <div className="rb-group">
        <div className="rb-items">
          <button className="rb-btn large" disabled={id === "none"} onMouseDown={keepFocus} onClick={() => run({ kind: "previewTransition" })} title="預覽">
            <Svg>
              <path d="M10 8.5v7l5.5-3.5z" fill="currentColor" />
            </Svg>
            <span>預覽</span>
          </button>
        </div>
        <div className="rb-label">預覽</div>
      </div>
      <div className="rb-group">
        <div className="rb-items transition-gallery">
          {TYPES.map((type) => (
            <button key={type.id} className={type.id === id ? "rb-btn active" : "rb-btn"} aria-pressed={type.id === id} onMouseDown={keepFocus} onClick={() => choose(type)} title={type.label}>
              <Svg>{type.icon}</Svg>
              <span>{type.label}</span>
            </button>
          ))}
          {!known && (
            <span className="transition-other" title="這個轉場類型會以淡出播放">
              其他（{id}）
            </span>
          )}
        </div>
        <div className="rb-label">切換到此投影片</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-field">
            效果選項
            <select disabled={!options} value={t?.option ?? ""} onChange={(e) => set({ option: e.target.value })}>
              {(options ?? [["", "—"]]).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label className="rb-field">
            期間（秒）
            <input
              type="number"
              min={0.01}
              max={59}
              step={0.25}
              disabled={id === "none"}
              value={((t?.durationMs ?? 0) / 1000).toFixed(2)}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s > 0) set({ durationMs: Math.round(s * 1000) });
              }}
            />
          </label>
          <button className="rb-btn" onMouseDown={keepFocus} onClick={() => set({}, true)} title="把這張投影片的轉場套用到所有投影片">
            全部套用
          </button>
        </div>
        <div className="rb-label">預存時間</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-check">
            <input type="checkbox" checked={t?.onClick ?? true} onChange={(e) => set({ id: id === "none" ? "cut" : id, durationMs: id === "none" ? 100 : (t?.durationMs ?? 0), onClick: e.target.checked })} />
            按一下滑鼠時
          </label>
          <label className="rb-check">
            <input
              type="checkbox"
              checked={(t?.afterMs ?? -1) >= 0}
              onChange={(e) => set({ id: id === "none" ? "cut" : id, durationMs: id === "none" ? 100 : (t?.durationMs ?? 0), afterMs: e.target.checked ? 5000 : -1 })}
            />
            之後
            <input
              type="number"
              min={0}
              step={0.5}
              className="narrow"
              disabled={(t?.afterMs ?? -1) < 0}
              value={((t?.afterMs ?? -1) >= 0 ? (t!.afterMs / 1000).toFixed(1) : "")}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s >= 0) set({ afterMs: Math.round(s * 1000) });
              }}
            />
            秒
          </label>
        </div>
        <div className="rb-label">換頁</div>
      </div>
    </>
  );
}
