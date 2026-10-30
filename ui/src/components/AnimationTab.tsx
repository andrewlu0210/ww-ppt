import { useState, type ReactNode } from "react";
import { Popover } from "./Popover";
import type { Command } from "../lib/commands";
import type { AnimationInfo, AnimationPreset, Status } from "../lib/status";

/** PowerPoint preset names by class and `presetID` (the common ones). */
const PRESETS: Record<string, Record<number, string>> = {
  entr: {
    1: "出現",
    2: "飛入",
    3: "百葉窗",
    4: "方塊",
    5: "棋盤式",
    6: "圓形",
    7: "慢速推入",
    8: "菱形",
    9: "溶解",
    10: "淡出",
    12: "窺視",
    13: "十字形",
    14: "隨機線條",
    16: "分割",
    17: "伸展",
    18: "階梯狀",
    19: "旋轉",
    20: "楔形",
    21: "輪子",
    22: "擦去",
    23: "縮放",
    26: "彈跳",
    53: "縮放",
    55: "放大旋轉",
  },
  exit: {
    1: "消失",
    2: "飛出",
    3: "百葉窗",
    4: "方塊",
    5: "棋盤式",
    6: "圓形",
    8: "菱形",
    9: "溶解",
    10: "淡出",
    14: "隨機線條",
    16: "分割",
    21: "輪子",
    22: "擦去",
    23: "縮放",
    53: "縮放",
  },
  emph: {
    1: "變更填滿色彩",
    3: "變更字型色彩",
    6: "放大/縮小",
    7: "變更線條色彩",
    8: "陀螺轉",
    9: "透明",
    14: "閃爍",
    26: "脈衝",
    32: "蹺蹺板",
  },
  path: {},
};

const CLASS_LABEL: Record<string, string> = { entr: "進入", exit: "結束", emph: "強調", path: "移動路徑" };
const TRIGGERS: [string, string][] = [
  ["clickEffect", "按一下"],
  ["withEffect", "與前一動畫同時"],
  ["afterEffect", "接續前一動畫"],
];
const TRIGGER_LABEL: Record<string, string> = Object.fromEntries(TRIGGERS);
/** Direction options (`presetSubtype`): where the shape comes from, or goes to on exits. */
const DIRECTIONS: [number, string, string][] = [
  [4, "自下方", "到下方"],
  [8, "自左方", "到左方"],
  [2, "自右方", "到右方"],
  [1, "自上方", "到上方"],
];

function presetName(cls: string, id: number | null): string {
  const named = id !== null ? PRESETS[cls]?.[id] : undefined;
  return named ?? (cls === "path" ? "自訂路徑" : "自訂");
}

const seconds = (ms: number) => `${(ms / 1000).toFixed(2).replace(/\.?0+$/, "")} 秒`;
const keepFocus = (e: React.MouseEvent) => e.preventDefault();

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);

/** The effects the editor can add, by class, for adding or replacing one. */
function EffectGallery({ presets, label, title, disabled, large, current, onPick }: { presets: AnimationPreset[]; label: string; title: string; disabled: boolean; large?: boolean; current?: AnimationInfo; onPick: (p: AnimationPreset) => void }) {
  return (
    <Popover
      button={(open, toggle) => (
        <button className={["rb-btn", large && "large", open && "active"].filter(Boolean).join(" ")} disabled={disabled} onMouseDown={keepFocus} onClick={toggle} title={title}>
          {large && (
            <Svg>
              <path d="M5 19l9-9" />
              <path d="M15 4l1 2.5 2.5 1-2.5 1L15 11l-1-2.5L11.5 7.5 14 6.5z" fill="currentColor" stroke="none" />
              <path d="M19 13l.6 1.4 1.4.6-1.4.6L19 17l-.6-1.4-1.4-.6 1.4-.6z" fill="currentColor" stroke="none" />
            </Svg>
          )}
          <span>{label} ▾</span>
        </button>
      )}
    >
      {(close) => (
        <div className="anim-gallery">
          {(["entr", "emph", "exit"] as const).map((cls) => (
            <div key={cls} className="anim-gallery-section">
              <div className={`anim-gallery-title ${cls}`}>{CLASS_LABEL[cls]}</div>
              <div className="anim-gallery-items">
                {presets
                  .filter((p) => p.class === cls)
                  .map((p) => (
                    <button
                      key={p.id}
                      className={current && current.class === p.class && current.presetId === p.id ? "anim-preset active" : "anim-preset"}
                      onMouseDown={keepFocus}
                      onClick={() => {
                        onPick(p);
                        close();
                      }}
                    >
                      {presetName(p.class, p.id)}
                    </button>
                  ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </Popover>
  );
}

export function AnimationTab({ status, run, presets }: { status: Status; run: (c: Command) => void; presets: AnimationPreset[] }) {
  const list = status.animations ?? [];
  // The chosen effect follows the canvas selection: a row click selects its shapes.
  const [picked, setPicked] = useState(-1);
  const index = list[picked]?.selected ? picked : list.findIndex((a) => a.selected);
  const cur: AnimationInfo | undefined = list[index];
  const preset = cur && presets.find((p) => p.class === cur.class && p.id === cur.presetId);
  const instant = !cur || cur.durMs <= 1;
  const pick = (i: number) => {
    setPicked(i);
    run({ kind: "selectAnimation", index: i });
  };
  const move = (to: number) => {
    run({ kind: "moveAnimation", from: index, to });
    setPicked(to);
  };

  return (
    <>
      <div className="rb-group">
        <div className="rb-items">
          <button className="rb-btn large" disabled={list.length === 0} onMouseDown={keepFocus} onClick={() => run({ kind: "previewAnimations" })} title="在編輯畫面播放這張投影片的動畫">
            <Svg>
              <rect x="3" y="3" width="18" height="18" rx="2" strokeOpacity="0.35" />
              <path d="M10 8.5v7l5.5-3.5z" fill="currentColor" />
            </Svg>
            <span>預覽</span>
          </button>
        </div>
        <div className="rb-label">預覽</div>
      </div>
      <div className="rb-group">
        <div className="rb-items">
          <EffectGallery presets={presets} large label="新增動畫" title="為選取的圖形新增動畫" disabled={!status.canAnimate} onPick={(p) => {
              // New effects go to the end of the list; show the first of them.
              setPicked(list.length);
              run({ kind: "addAnimation", class: p.class, id: p.id });
            }}
          />
          <div className="rb-col">
            <EffectGallery
              presets={presets}
              label="變更效果"
              title="把選取的動畫換成另一個效果"
              disabled={!cur}
              current={cur}
              onPick={(p) => run({ kind: "changeAnimation", index, class: p.class, id: p.id })}
            />
            <label className="rb-field">
              效果選項
              <select
                disabled={!preset?.directional}
                value={preset?.directional ? (cur?.subtype ?? preset.subtype) : ""}
                onChange={(e) => cur && run({ kind: "changeAnimation", index, class: cur.class, id: cur.presetId ?? 0, subtype: Number(e.target.value) })}
              >
                {preset?.directional ? (
                  DIRECTIONS.map(([value, from, to]) => (
                    <option key={value} value={value}>
                      {cur?.class === "exit" ? to : from}
                    </option>
                  ))
                ) : (
                  <option value="">—</option>
                )}
              </select>
            </label>
          </div>
        </div>
        <div className="rb-label">動畫</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-field">
            開始
            <select disabled={!cur} value={cur?.trigger ?? ""} onChange={(e) => run({ kind: "setAnimationTrigger", index, trigger: e.target.value })}>
              {!cur && <option value="">—</option>}
              {TRIGGERS.map(([value, label]) => (
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
              disabled={instant}
              value={instant ? "" : (cur.durMs / 1000).toFixed(2)}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s > 0) run({ kind: "setAnimationTiming", index, durMs: Math.round(s * 1000) });
              }}
            />
          </label>
          <label className="rb-field">
            延遲（秒）
            <input
              type="number"
              min={0}
              max={59}
              step={0.25}
              disabled={!cur}
              value={cur ? (cur.delayMs / 1000).toFixed(2) : ""}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s >= 0) run({ kind: "setAnimationTiming", index, delayMs: Math.round(s * 1000) });
              }}
            />
          </label>
        </div>
        <div className="rb-label">預存時間</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <button className="rb-btn" disabled={!cur || index === 0} onMouseDown={keepFocus} onClick={() => move(index - 1)} title="讓這個動畫早一點播放">
            ▲ 往前移
          </button>
          <button className="rb-btn" disabled={!cur || index === list.length - 1} onMouseDown={keepFocus} onClick={() => move(index + 1)} title="讓這個動畫晚一點播放">
            ▼ 往後移
          </button>
          <button className="rb-btn" disabled={!cur} onMouseDown={keepFocus} onClick={() => run({ kind: "removeAnimation", index })} title="移除這個動畫">
            ✕ 移除
          </button>
        </div>
        <div className="rb-label">重新排列</div>
      </div>
      <div className="rb-group anim-list-group">
        <div className="anim-list" role="listbox" aria-label="動畫清單">
          {list.length === 0 && <div className="anim-empty">這張投影片沒有動畫。選取圖形後按「新增動畫」。</div>}
          {list.map((a, i) => (
            <div key={i} className={i === index ? "anim-row active" : "anim-row"} role="option" aria-selected={i === index} onMouseDown={keepFocus} onClick={() => pick(i)}>
              <span className="anim-click">{a.trigger === "clickEffect" || (i === 0 && a.click === 0) ? a.click || "▶" : ""}</span>
              <span className={`anim-class ${a.class}`}>{CLASS_LABEL[a.class] ?? a.class}</span>
              <span className="anim-name">{presetName(a.class, a.presetId)}</span>
              <span className="anim-shape">{a.shapes.join("、")}</span>
              <span className="anim-meta">
                {TRIGGER_LABEL[a.trigger] ?? a.trigger}
                {a.startMs > 0 ? ` · 開始於 ${seconds(a.startMs)}` : ""}
                {a.durMs > 1 ? ` · ${seconds(a.durMs)}` : ""}
              </span>
            </div>
          ))}
        </div>
        <div className="rb-label">動畫清單</div>
      </div>
    </>
  );
}

/** Click numbers next to animated shapes, as PowerPoint shows them on the 動畫 tab. */
export function AnimationBadges({ animations }: { animations: AnimationInfo[] }) {
  const spots = new Map<string, { x: number; y: number; labels: number[]; selected: boolean }>();
  for (const a of animations) {
    if (!a.badge) continue;
    const key = `${Math.round(a.badge[0])},${Math.round(a.badge[1])}`;
    const spot = spots.get(key) ?? { x: a.badge[0], y: a.badge[1], labels: [], selected: false };
    if (!spot.labels.includes(a.click)) spot.labels.push(a.click);
    spot.selected ||= a.selected;
    spots.set(key, spot);
  }
  return (
    <div className="anim-badges" aria-hidden="true">
      {[...spots.entries()].map(([key, s]) => (
        <span key={key} className={s.selected ? "anim-badge selected" : "anim-badge"} style={{ left: s.x, top: s.y }}>
          {s.labels.join(", ")}
        </span>
      ))}
    </div>
  );
}
