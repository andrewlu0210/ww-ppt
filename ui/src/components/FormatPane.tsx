import { t } from "../i18n";
import { useEffect, useState, type ReactNode } from "react";
import type { App } from "../wasm/ppt_wasm";
import type { Command } from "../lib/commands";
import { ColorPicker } from "./ColorPicker";

// What `App::fillLineInfo` returns.
interface Stop {
  pos: number;
  color: string;
  alpha: number;
}
interface FillInfo {
  type: "none" | "solid" | "gradient";
  color?: string;
  alpha?: number;
  kind?: "linear" | "radial";
  angle?: number;
  stops?: Stop[];
}
interface End {
  type: string;
  w: number;
  l: number;
}
interface LineInfo {
  visible: boolean;
  color?: string;
  alpha?: number;
  widthPt?: number;
  dash?: string;
  head?: End;
  tail?: End;
}
interface Info {
  canFill: boolean;
  canArrows: boolean;
  /** null: the selected shapes differ. */
  fill: FillInfo | null;
  line: LineInfo | null;
}

export type FillSpec = { type: "none" } | { type: "solid"; color: string; alpha: number } | { type: "gradient"; kind: "linear" | "radial"; angle: number; stops: Stop[] };
export type LineSpec = Partial<{ visible: boolean; color: string; alpha: number; widthPt: number; dash: string; head: End; tail: End }>;

const DEFAULT_COLOR = "#4472C4";

const DASHES: [string, string][] = [
  ["solid", t("format.dash.solid")],
  ["sysDot", t("format.dash.sysDot")],
  ["sysDash", t("format.dash.sysDash")],
  ["dash", t("format.dash.dash")],
  ["dashDot", t("format.dash.dashDot")],
  ["lgDash", t("format.dash.lgDash")],
  ["lgDashDot", t("format.dash.lgDashDot")],
  ["lgDashDotDot", t("format.dash.lgDashDotDot")],
];

const ARROWS: [string, string][] = [
  ["none", t("format.arrow.none")],
  ["triangle", t("format.arrow.triangle")],
  ["stealth", t("format.arrow.stealth")],
  ["diamond", t("format.arrow.diamond")],
  ["oval", t("format.arrow.oval")],
  ["arrow", t("format.arrow.arrow")],
];

const SIZES: [number, string][] = [
  [0, t("format.sizeSmall")],
  [1, t("format.sizeMedium")],
  [2, t("format.sizeLarge")],
];

/** `hex` moved towards white by `amount` (0–1). */
function lighten(hex: string, amount: number): string {
  const v = parseInt(hex.slice(1), 16);
  const ch = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((c) => Math.round(c + (255 - c) * amount));
  return "#" + ch.map((c) => c.toString(16).padStart(2, "0")).join("").toUpperCase();
}

const css = (hex: string, alpha: number) => {
  const v = parseInt(hex.slice(1), 16);
  return `rgba(${(v >> 16) & 255}, ${(v >> 8) & 255}, ${v & 255}, ${alpha})`;
};

/** A number committed on Enter or when the field is left (not on every keystroke). */
function Num({ label, value, unit, min, max, step, disabled, onCommit }: { label: string; value: number | undefined; unit?: string; min?: number; max?: number; step?: number; disabled?: boolean; onCommit: (v: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const shown = text ?? (value === undefined ? "" : String(Math.round(value * 100) / 100));
  const commit = () => {
    if (text === null) return;
    const v = Number(text);
    setText(null);
    if (Number.isFinite(v) && text.trim() !== "") onCommit(Math.min(max ?? Infinity, Math.max(min ?? -Infinity, v)));
  };
  return (
    <label className="fp-field">
      <span>{label}</span>
      <input
        type="number"
        value={shown}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          else if (e.key === "Escape") setText(null);
        }}
      />
      {unit && <span className="fp-unit">{unit}</span>}
    </label>
  );
}

/** A slider with its number, in percent. The slider commits when released, so one drag is one undo step. */
function Percent({ label, value, disabled, onCommit }: { label: string; value: number; disabled?: boolean; onCommit: (percent: number) => void }) {
  const [drag, setDrag] = useState<number | null>(null);
  const shown = drag ?? value;
  const release = () => {
    if (drag !== null) onCommit(drag);
    setDrag(null);
  };
  return (
    <div className="fp-row">
      <span className="fp-label">{label}</span>
      <input type="range" min={0} max={100} step={1} value={shown} disabled={disabled} aria-label={label} onChange={(e) => setDrag(Number(e.target.value))} onPointerUp={release} onKeyUp={release} onBlur={release} />
      <Num label={label} value={shown} unit="%" min={0} max={100} step={1} disabled={disabled} onCommit={(v) => onCommit(v)} />
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  const [open, setOpen] = useState(true);
  return (
    <section className="fp-section">
      <button className="fp-section-head" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span aria-hidden="true">{open ? "▾" : "▸"}</span> {title}
      </button>
      {open && <div className="fp-section-body">{children}</div>}
    </section>
  );
}

function Swatch({ color, alpha }: { color?: string; alpha?: number }) {
  return <span className="fp-swatch" style={{ background: color ? css(color, alpha ?? 1) : "transparent" }} />;
}

function Radios({ name, value, options, onPick, disabled }: { name: string; value: string | null; options: [string, string][]; onPick: (v: string) => void; disabled?: boolean }) {
  return (
    <div className="fp-radios" role="radiogroup">
      {options.map(([v, label]) => (
        <label key={v}>
          <input type="radio" name={name} checked={value === v} disabled={disabled} onChange={() => onPick(v)} />
          {label}
        </label>
      ))}
    </div>
  );
}

interface Props {
  engine: App | null;
  /** Re-read the selection when this changes (document revision, selection). */
  version: string;
  run: (c: Command) => void;
  onClose: () => void;
}

/**
 * 設定圖形格式: the selected shapes' fill and outline. Every change is one command and one undo step;
 * with different values in the selection the controls show nothing chosen until one is picked.
 */
export function FormatPane({ engine, version, run, onClose }: Props) {
  const [info, setInfo] = useState<Info | null>(null);
  useEffect(() => {
    setInfo(engine ? ((engine.fillLineInfo() as Info | null) ?? null) : null);
  }, [engine, version]);

  const fill = info?.fill ?? null;
  const line = info?.line ?? null;
  const setFill = (spec: FillSpec) => run({ kind: "fillSpec", spec });
  const setLine = (spec: LineSpec) => run({ kind: "lineSpec", spec });

  // --- fill
  const baseColor = fill?.type === "solid" ? fill.color! : fill?.type === "gradient" ? fill.stops![0].color : DEFAULT_COLOR;
  const baseAlpha = fill?.type === "solid" ? fill.alpha! : 1;
  const pickFillType = (type: string) => {
    if (type === "none") setFill({ type: "none" });
    else if (type === "solid") setFill({ type: "solid", color: baseColor, alpha: baseAlpha });
    else setFill({ type: "gradient", kind: "linear", angle: 90, stops: [{ pos: 0, color: baseColor, alpha: baseAlpha }, { pos: 1, color: lighten(baseColor, 0.6), alpha: baseAlpha }] });
  };
  const gradient = fill?.type === "gradient" ? fill : null;
  const setGradient = (patch: Partial<{ kind: "linear" | "radial"; angle: number; stops: Stop[] }>) => {
    if (!gradient) return;
    setFill({ type: "gradient", kind: patch.kind ?? gradient.kind ?? "linear", angle: patch.angle ?? gradient.angle ?? 90, stops: patch.stops ?? gradient.stops! });
  };
  const editStop = (i: number, patch: Partial<Stop>) => setGradient({ stops: gradient!.stops!.map((s, k) => (k === i ? { ...s, ...patch } : s)) });
  const addStop = () => {
    const stops = [...gradient!.stops!].sort((a, b) => a.pos - b.pos);
    // In the middle of the widest gap, in the color of the stop before it.
    let at = 0;
    for (let i = 1; i < stops.length; i++) if (stops[i].pos - stops[i - 1].pos > stops[at + 1].pos - stops[at].pos) at = i - 1;
    const mid = (stops[at].pos + stops[at + 1].pos) / 2;
    setGradient({ stops: [...stops.slice(0, at + 1), { ...stops[at], pos: mid }, ...stops.slice(at + 1)] });
  };
  const preview = gradient
    ? `linear-gradient(90deg, ${[...gradient.stops!].sort((a, b) => a.pos - b.pos).map((s) => `${css(s.color, s.alpha)} ${Math.round(s.pos * 100)}%`).join(", ")})`
    : undefined;

  // --- line
  const lineVisible = line === null ? null : line.visible ? "solid" : "none";
  const dashOptions = line?.dash && !DASHES.some(([v]) => v === line.dash) ? [...DASHES, [line.dash, line.dash] as [string, string]] : DASHES;
  const endControls = (key: "head" | "tail", label: string) => {
    const e = line?.[key];
    return (
      <div className="fp-group">
        <label className="fp-field wide">
          <span>{label}</span>
          <select value={e?.type ?? "none"} onChange={(ev) => setLine({ [key]: { type: ev.target.value, w: e?.w ?? 1, l: e?.l ?? 1 } })}>
            {ARROWS.map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </label>
        {e && e.type !== "none" && (
          <div className="fp-row">
            <label className="fp-field">
              <span>{t("format.arrowWidth")}</span>
              <select value={e.w} onChange={(ev) => setLine({ [key]: { ...e, w: Number(ev.target.value) } })}>
                {SIZES.map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
            </label>
            <label className="fp-field">
              <span>{t("format.arrowLength")}</span>
              <select value={e.l} onChange={(ev) => setLine({ [key]: { ...e, l: Number(ev.target.value) } })}>
                {SIZES.map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
            </label>
          </div>
        )}
      </div>
    );
  };

  return (
    <aside className="format-pane" aria-label={t("format.paneTitle")}>
      <div className="pane-head">
        <span className="pane-title">{t("format.paneTitle")}</span>
        <button className="pane-close" onClick={onClose} aria-label={t("format.paneClose")} title={t("common.close")}>
          ×
        </button>
      </div>
      {!info ? (
        <div className="pane-empty">{t("format.nothingSelected")}</div>
      ) : (
        <div className="fp-scroll">
          <Section title={t("format.fillSection")}>
            {!info.canFill ? (
              <div className="pane-empty">{t("format.noFill")}</div>
            ) : (
              <>
                <Radios
                  name="fp-fill"
                  value={fill?.type ?? null}
                  options={[
                    ["none", t("format.fillNone")],
                    ["solid", t("format.fillSolid")],
                    ["gradient", t("format.fillGradient")],
                  ]}
                  onPick={pickFillType}
                />
                {fill === null && <div className="pane-empty">{t("format.mixed")}</div>}
                {fill?.type === "solid" && (
                  <>
                    <div className="fp-row">
                      <span className="fp-label">{t("format.color")}</span>
                      <ColorPicker icon={<Swatch color={fill.color} alpha={fill.alpha} />} label="" value={fill.color} onPick={(color) => color && setFill({ type: "solid", color, alpha: fill.alpha! })} />
                    </div>
                    <Percent label={t("format.transparency")} value={Math.round((1 - fill.alpha!) * 100)} onCommit={(p) => setFill({ type: "solid", color: fill.color!, alpha: 1 - p / 100 })} />
                  </>
                )}
                {gradient && (
                  <>
                    <div className="fp-gradient-bar" style={{ background: preview }} aria-hidden="true" />
                    <div className="fp-row">
                      <label className="fp-field wide">
                        <span>{t("format.gradientType")}</span>
                        <select value={gradient.kind} onChange={(e) => setGradient({ kind: e.target.value as "linear" | "radial" })}>
                          <option value="linear">{t("format.gradLinear")}</option>
                          <option value="radial">{t("format.gradRadial")}</option>
                        </select>
                      </label>
                    </div>
                    {gradient.kind === "linear" && <Num label={t("format.angle")} unit="°" value={gradient.angle} min={0} max={359} step={5} onCommit={(angle) => setGradient({ angle })} />}
                    <div className="fp-subhead">{t("format.stops")}</div>
                    {gradient.stops!.map((s, i) => (
                      <div className="fp-stop" key={i} role="group" aria-label={t("format.stopN", { n: i + 1 })}>
                        <ColorPicker icon={<Swatch color={s.color} alpha={s.alpha} />} label="" value={s.color} onPick={(color) => color && editStop(i, { color })} />
                        <Num label={t("format.stopPosition")} unit="%" value={Math.round(s.pos * 100)} min={0} max={100} step={5} onCommit={(p) => editStop(i, { pos: p / 100 })} />
                        <Num label={t("format.transparency")} unit="%" value={Math.round((1 - s.alpha) * 100)} min={0} max={100} step={5} onCommit={(p) => editStop(i, { alpha: 1 - p / 100 })} />
                        <button className="fp-icon" disabled={gradient.stops!.length <= 2} onClick={() => setGradient({ stops: gradient.stops!.filter((_, k) => k !== i) })} aria-label={t("format.removeStop")} title={t("format.removeStop")}>
                          −
                        </button>
                      </div>
                    ))}
                    <button className="fp-add" onClick={addStop}>
                      + {t("format.addStop")}
                    </button>
                  </>
                )}
              </>
            )}
          </Section>
          <Section title={t("format.lineSection")}>
            <Radios
              name="fp-line"
              value={lineVisible}
              options={[
                ["none", t("format.lineNone")],
                ["solid", t("format.lineSolid")],
              ]}
              onPick={(v) => setLine({ visible: v === "solid" })}
            />
            {line === null && <div className="pane-empty">{t("format.mixed")}</div>}
            {line?.visible && (
              <>
                <div className="fp-row">
                  <span className="fp-label">{t("format.color")}</span>
                  <ColorPicker icon={<Swatch color={line.color} alpha={line.alpha} />} label="" value={line.color} onPick={(color) => color && setLine({ color, alpha: line.alpha })} />
                </div>
                <Percent label={t("format.transparency")} value={Math.round((1 - line.alpha!) * 100)} onCommit={(p) => setLine({ alpha: 1 - p / 100 })} />
                <Num label={t("format.lineWidth")} unit="pt" value={line.widthPt} min={0} max={1584} step={0.25} onCommit={(widthPt) => setLine({ widthPt })} />
                <label className="fp-field wide">
                  <span>{t("format.dashType")}</span>
                  <select value={line.dash} onChange={(e) => setLine({ dash: e.target.value })}>
                    {dashOptions.map(([v, l]) => (
                      <option key={v} value={v}>
                        {l}
                      </option>
                    ))}
                  </select>
                </label>
                {info.canArrows && (
                  <>
                    {endControls("head", t("format.beginArrow"))}
                    {endControls("tail", t("format.endArrow"))}
                  </>
                )}
              </>
            )}
          </Section>
        </div>
      )}
    </aside>
  );
}
