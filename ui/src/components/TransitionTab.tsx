import { t } from "../i18n";
import type { ReactNode } from "react";
import type { Command } from "../lib/commands";
import type { Status, TransitionInfo } from "../lib/status";

/** Transition types in gallery order, with PowerPoint's default durations (ms). */
const TYPES: { id: string; label: string; ms: number; icon: ReactNode }[] = [
  { id: "none", label: t("trans.none"), ms: 0, icon: <path d="M6 6l12 12" /> },
  { id: "cut", label: t("trans.cut"), ms: 100, icon: <path d="M12 4v16" /> },
  { id: "fade", label: t("trans.fade"), ms: 700, icon: <rect x="6" y="7" width="12" height="10" strokeDasharray="2 2" /> },
  { id: "push", label: t("trans.push"), ms: 1000, icon: <path d="M12 18V7M8.5 10.5 12 7l3.5 3.5" /> },
  { id: "wipe", label: t("trans.wipe"), ms: 1000, icon: <path d="M17 6v12M7 12h8M12 9l3 3-3 3" /> },
  { id: "cover", label: t("trans.cover"), ms: 1000, icon: <path d="M10 8h9v9h-9zM5 6h9" /> },
  { id: "pull", label: t("trans.uncover"), ms: 1000, icon: <path d="M5 8h9v9H5zM10 6h9" /> },
  { id: "split", label: t("trans.split"), ms: 1500, icon: <path d="M12 5v14M9 12H5M15 12h4M7 10l-2 2 2 2M17 10l2 2-2 2" /> },
  { id: "dissolve", label: t("trans.dissolve"), ms: 700, icon: <path d="M7 7h2v2H7zM12 9h2v2h-2zM9 13h2v2H9zM15 14h2v2h-2zM16 7h1v1h-1z" /> },
  { id: "randomBar", label: t("trans.randomBars"), ms: 1000, icon: <path d="M7 6v12M9.5 6v12M14 6v12M17 6v12" /> },
  { id: "zoom", label: t("trans.zoom"), ms: 500, icon: <path d="M9 9h6v6H9zM5 5l3 3M19 5l-3 3M5 19l3-3M19 19l-3-3" /> },
  { id: "circle", label: t("trans.circle"), ms: 1000, icon: <circle cx="12" cy="12" r="6" /> },
];

/** Effect options per type: [option id, label]. Directions are where the slide moves. */
const OPTIONS: Record<string, [string, string][]> = {
  cut: [
    ["", t("trans.smoothly")],
    ["thruBlk", t("trans.throughBlack")],
  ],
  fade: [
    ["", t("trans.smoothly")],
    ["thruBlk", t("trans.throughBlack")],
  ],
  push: [
    ["u", t("trans.fromBottom")],
    ["l", t("trans.fromRight")],
    ["d", t("trans.fromTop")],
    ["r", t("trans.fromLeft")],
  ],
  wipe: [
    ["l", t("trans.fromRight")],
    ["u", t("trans.fromBottom")],
    ["r", t("trans.fromLeft")],
    ["d", t("trans.fromTop")],
  ],
  cover: [
    ["l", t("trans.fromRight")],
    ["u", t("trans.fromBottom")],
    ["r", t("trans.fromLeft")],
    ["d", t("trans.fromTop")],
    ["ld", t("trans.fromTopRight")],
    ["lu", t("trans.fromBottomRight")],
    ["rd", t("trans.fromTopLeft")],
    ["ru", t("trans.fromBottomLeft")],
  ],
  split: [
    ["vertOut", t("trans.verticalOut")],
    ["vertIn", t("trans.verticalIn")],
    ["horzOut", t("trans.horizontalOut")],
    ["horzIn", t("trans.horizontalIn")],
  ],
  randomBar: [
    ["vert", t("trans.vertical")],
    ["horz", t("trans.horizontal")],
  ],
  zoom: [
    ["in", t("trans.in")],
    ["out", t("trans.out")],
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
  const tr: TransitionInfo | undefined = status.transition;
  const id = tr?.id ?? "none";
  const set = (patch: Partial<TransitionInfo>, all = false) => {
    const next: TransitionInfo = { id, option: tr?.option ?? "", durationMs: tr?.durationMs ?? 700, onClick: tr?.onClick ?? true, afterMs: tr?.afterMs ?? -1, ...patch };
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
          <button className="rb-btn large" disabled={id === "none"} onMouseDown={keepFocus} onClick={() => run({ kind: "previewTransition" })} title={t("anim.preview")}>
            <Svg>
              <path d="M10 8.5v7l5.5-3.5z" fill="currentColor" />
            </Svg>
            <span>{t("anim.preview")}</span>
          </button>
        </div>
        <div className="rb-label">{t("anim.preview")}</div>
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
            <span className="transition-other" title={t("trans.otherTip")}>
              {t("trans.other", { id })}
            </span>
          )}
        </div>
        <div className="rb-label">{t("trans.toThisSlide")}</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-field">
            {t("anim.effectOptions")}
            <select disabled={!options} value={tr?.option ?? ""} onChange={(e) => set({ option: e.target.value })}>
              {(options ?? [["", "—"]]).map(([value, label]) => (
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
              disabled={id === "none"}
              value={((tr?.durationMs ?? 0) / 1000).toFixed(2)}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s > 0) set({ durationMs: Math.round(s * 1000) });
              }}
            />
          </label>
          <button className="rb-btn" onMouseDown={keepFocus} onClick={() => set({}, true)} title={t("trans.applyAllTip")}>
            {t("trans.applyAll")}
          </button>
        </div>
        <div className="rb-label">{t("anim.timing")}</div>
      </div>
      <div className="rb-group">
        <div className="rb-col">
          <label className="rb-check">
            <input type="checkbox" checked={tr?.onClick ?? true} onChange={(e) => set({ id: id === "none" ? "cut" : id, durationMs: id === "none" ? 100 : (tr?.durationMs ?? 0), onClick: e.target.checked })} />
            {t("trans.onClick")}
          </label>
          <label className="rb-check">
            <input
              type="checkbox"
              checked={(tr?.afterMs ?? -1) >= 0}
              onChange={(e) => set({ id: id === "none" ? "cut" : id, durationMs: id === "none" ? 100 : (tr?.durationMs ?? 0), afterMs: e.target.checked ? 5000 : -1 })}
            />
            {t("trans.after")}
            <input
              type="number"
              min={0}
              step={0.5}
              className="narrow"
              disabled={(tr?.afterMs ?? -1) < 0}
              value={((tr?.afterMs ?? -1) >= 0 ? (tr!.afterMs / 1000).toFixed(1) : "")}
              onChange={(e) => {
                const s = Number(e.target.value);
                if (s >= 0) set({ afterMs: Math.round(s * 1000) });
              }}
            />
            {t("trans.secondsUnit")}
          </label>
        </div>
        <div className="rb-label">{t("trans.advance")}</div>
      </div>
    </>
  );
}
