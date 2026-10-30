import type { Command } from "../lib/commands";
import type { AnimationInfo, Status } from "../lib/status";

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
const TRIGGER_LABEL: Record<string, string> = { clickEffect: "按一下", withEffect: "與前一動畫同時", afterEffect: "接續前一動畫" };

function effectName(a: AnimationInfo): string {
  const named = a.presetId !== null ? PRESETS[a.class]?.[a.presetId] : undefined;
  return named ?? (a.class === "path" ? "自訂路徑" : "自訂");
}

const seconds = (ms: number) => `${(ms / 1000).toFixed(2).replace(/\.?0+$/, "")} 秒`;

export function AnimationTab({ status, run }: { status: Status; run: (c: Command) => void }) {
  const list = status.animations ?? [];
  return (
    <>
      <div className="rb-group">
        <div className="rb-items">
          <button className="rb-btn large" disabled={list.length === 0} onMouseDown={(e) => e.preventDefault()} onClick={() => run({ kind: "previewAnimations" })} title="在編輯畫面播放這張投影片的動畫">
            <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
              <rect x="3" y="3" width="18" height="18" rx="2" strokeOpacity="0.35" />
              <path d="M10 8.5v7l5.5-3.5z" fill="currentColor" />
            </svg>
            <span>預覽</span>
          </button>
        </div>
        <div className="rb-label">預覽</div>
      </div>
      <div className="rb-group anim-list-group">
        <div className="anim-list" role="list" aria-label="動畫清單">
          {list.length === 0 && <div className="anim-empty">這張投影片沒有動畫</div>}
          {list.map((a, i) => (
            <div key={i} className="anim-row" role="listitem">
              <span className="anim-click">{a.trigger === "clickEffect" || (i === 0 && a.click === 0) ? a.click || "▶" : ""}</span>
              <span className={`anim-class ${a.class}`}>{CLASS_LABEL[a.class] ?? a.class}</span>
              <span className="anim-name">{effectName(a)}</span>
              <span className="anim-shape">{a.shapes.join("、")}</span>
              <span className="anim-meta">
                {TRIGGER_LABEL[a.trigger] ?? a.trigger}
                {a.startMs > 0 ? ` · 延遲 ${seconds(a.startMs)}` : ""} · {seconds(a.durMs)}
              </span>
            </div>
          ))}
        </div>
        <div className="rb-label">動畫清單（唯讀）</div>
      </div>
    </>
  );
}
