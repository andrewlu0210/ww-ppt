import { t } from "../i18n";
import { useState, type ReactNode } from "react";
import { Popover } from "./Popover";
import type { Command } from "../lib/commands";
import type { AnimationInfo, AnimationPreset, Status } from "../lib/status";

/** PowerPoint preset names by class and `presetID` (the common ones). */
const PRESETS: Record<string, Record<number, string>> = {
  entr: {
    1: t("anim.appear"),
    2: t("anim.flyIn"),
    3: t("anim.blinds"),
    4: t("anim.box"),
    5: t("anim.checkerboard"),
    6: t("anim.circle"),
    7: t("anim.crawlIn"),
    8: t("anim.diamond"),
    9: t("anim.dissolve"),
    10: t("anim.fade"),
    12: t("anim.peekIn"),
    13: t("anim.plus"),
    14: t("anim.randomBars"),
    16: t("anim.split"),
    17: t("anim.stretch"),
    18: t("anim.strips"),
    19: t("anim.swivel"),
    20: t("anim.wedge"),
    21: t("anim.wheel"),
    22: t("anim.wipe"),
    23: t("anim.zoom"),
    26: t("anim.bounce"),
    53: t("anim.zoom"),
    55: t("anim.growTurn"),
  },
  exit: {
    1: t("anim.disappear"),
    2: t("anim.flyOut"),
    3: t("anim.blinds"),
    4: t("anim.box"),
    5: t("anim.checkerboard"),
    6: t("anim.circle"),
    8: t("anim.diamond"),
    9: t("anim.dissolve"),
    10: t("anim.fade"),
    14: t("anim.randomBars"),
    16: t("anim.split"),
    21: t("anim.wheel"),
    22: t("anim.wipe"),
    23: t("anim.zoom"),
    53: t("anim.zoom"),
  },
  emph: {
    1: t("anim.fillColor"),
    3: t("anim.fontColor"),
    6: t("anim.growShrink"),
    7: t("anim.lineColor"),
    8: t("anim.spin"),
    9: t("anim.transparency"),
    14: t("anim.blink"),
    26: t("anim.pulse"),
    32: t("anim.teeter"),
  },
  path: {},
};

const CLASS_LABEL: Record<string, string> = { entr: t("anim.entrance"), exit: t("anim.exit"), emph: t("anim.emphasis"), path: t("anim.motionPath") };
const TRIGGERS: [string, string][] = [
  ["clickEffect", t("anim.onClick")],
  ["withEffect", t("anim.withPrevious")],
  ["afterEffect", t("anim.afterPrevious")],
];
const TRIGGER_LABEL: Record<string, string> = Object.fromEntries(TRIGGERS);
/** Direction options (`presetSubtype`): where the shape comes from, or goes to on exits. */
const DIRECTIONS: [number, string, string][] = [
  [4, t("anim.fromBottom"), t("anim.toBottom")],
  [8, t("anim.fromLeft"), t("anim.toLeft")],
  [2, t("anim.fromRight"), t("anim.toRight")],
  [1, t("anim.fromTop"), t("anim.toTop")],
];

function presetName(cls: string, id: number | null): string {
  const named = id !== null ? PRESETS[cls]?.[id] : undefined;
  return named ?? (cls === "path" ? t("anim.customPath") : t("anim.custom"));
}

const seconds = (ms: number) => t("anim.seconds", { n: (ms / 1000).toFixed(2).replace(/\.?0+$/, "") });
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
          <button className="rb-btn large" disabled={list.length === 0} onMouseDown={keepFocus} onClick={() => run({ kind: "previewAnimations" })} title={t("anim.previewTip")}>
            <Svg>
              <rect x="3" y="3" width="18" height="18" rx="2" strokeOpacity="0.35" />
              <path d="M10 8.5v7l5.5-3.5z" fill="currentColor" />
            </Svg>
            <span>{t("anim.preview")}</span>
          </button>
        </div>
        <div className="rb-label">{t("anim.preview")}</div>
      </div>
      <div className="rb-group">
        <div className="rb-items">
          <EffectGallery presets={presets} large label={t("anim.addAnimation")} title={t("anim.addAnimationTip")} disabled={!status.canAnimate} onPick={(p) => {
              // New effects go to the end of the list; show the first of them.
              setPicked(list.length);
              run({ kind: "addAnimation", class: p.class, id: p.id });
            }}
          />
          <div className="rb-col">
            <EffectGallery
              presets={presets}
              label={t("anim.changeEffect")}
              title={t("anim.changeEffectTip")}
              disabled={!cur}
              current={cur}
              onPick={(p) => run({ kind: "changeAnimation", index, class: p.class, id: p.id })}
            />
            <label className="rb-field">
              {t("anim.effectOptions")}
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
        <div className="rb-label">{t("anim.animation")}</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-field">
            {t("anim.start")}
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
            {t("anim.duration")}
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
            {t("anim.delay")}
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
        <div className="rb-label">{t("anim.timing")}</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <button className="rb-btn" disabled={!cur || index === 0} onMouseDown={keepFocus} onClick={() => move(index - 1)} title={t("anim.earlierTip")}>
            {t("anim.moveEarlier")}
          </button>
          <button className="rb-btn" disabled={!cur || index === list.length - 1} onMouseDown={keepFocus} onClick={() => move(index + 1)} title={t("anim.laterTip")}>
            {t("anim.moveLater")}
          </button>
          <button className="rb-btn" disabled={!cur} onMouseDown={keepFocus} onClick={() => run({ kind: "removeAnimation", index })} title={t("anim.removeTip")}>
            {t("anim.remove")}
          </button>
        </div>
        <div className="rb-label">{t("anim.reorder")}</div>
      </div>
      <div className="rb-group anim-list-group">
        <div className="anim-list" role="listbox" aria-label={t("anim.list")}>
          {list.length === 0 && <div className="anim-empty">{t("anim.empty")}</div>}
          {list.map((a, i) => (
            <div key={i} className={i === index ? "anim-row active" : "anim-row"} role="option" aria-selected={i === index} onMouseDown={keepFocus} onClick={() => pick(i)}>
              <span className="anim-click">{a.trigger === "clickEffect" || (i === 0 && a.click === 0) ? a.click || "▶" : ""}</span>
              <span className={`anim-class ${a.class}`}>{CLASS_LABEL[a.class] ?? a.class}</span>
              <span className="anim-name">{presetName(a.class, a.presetId)}</span>
              <span className="anim-shape">{a.shapes.join(t("common.listSeparator"))}</span>
              <span className="anim-meta">
                {TRIGGER_LABEL[a.trigger] ?? a.trigger}
                {a.startMs > 0 ? ` · ${t("anim.startsAt", { time: seconds(a.startMs) })}` : ""}
                {a.durMs > 1 ? ` · ${seconds(a.durMs)}` : ""}
              </span>
            </div>
          ))}
        </div>
        <div className="rb-label">{t("anim.list")}</div>
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
