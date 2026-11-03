import { t } from "../i18n";
import { useEffect, useState, type ReactNode } from "react";
import type { App } from "../wasm/ppt_wasm";
import type { Command } from "../lib/commands";
import { ColorPicker } from "./ColorPicker";
import { SHADOWS } from "./FormatGroups";
import type { Status } from "../lib/status";

// What `App::formatInfo` returns.
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
interface ShadowInfo {
  visible: boolean;
  color?: string;
  alpha?: number;
  blurPt?: number;
  distPt?: number;
  angle?: number;
}
interface TextBoxInfo {
  anchor: "top" | "middle" | "bottom" | null;
  left: number | null;
  top: number | null;
  right: number | null;
  bottom: number | null;
  wrap: boolean | null;
  autoGrow: boolean | null;
}
interface Info {
  /** Lock aspect ratio; null when the shapes differ. */
  lockAspect: boolean | null;
  /** null when no selected shape has a text body. */
  textBox: TextBoxInfo | null;
  canFill: boolean;
  canArrows: boolean;
  canShadow: boolean;
  /** null: the selected shapes differ. */
  shadow: ShadowInfo | null;
  /** null: the selected shapes differ. */
  fill: FillInfo | null;
  line: LineInfo | null;
}

export type FillSpec = { type: "none" } | { type: "solid"; color: string; alpha: number } | { type: "gradient"; kind: "linear" | "radial"; angle: number; stops: Stop[] };
export type TextBoxSpec = Partial<{ anchor: "top" | "middle" | "bottom"; left: number; top: number; right: number; bottom: number; wrap: boolean; autoGrow: boolean }>;
export type ShadowSpec = Partial<{ visible: boolean; color: string; alpha: number; blurPt: number; distPt: number; angle: number }>;
export type LineSpec = Partial<{ visible: boolean; color: string; alpha: number; widthPt: number; dash: string; head: End; tail: End }>;

const DEFAULT_COLOR = "#4472C4";
const EMU_PER_CM = 360_000;

/** PowerPoint's preset shadows as (direction °, distance pt, blur pt), to tell which one a shadow is. */
const SHADOW_SHAPES: Record<string, [number, number, number]> = { bottomRight: [45, 3, 4], bottom: [90, 3, 4], right: [0, 3, 4], bottomLeft: [135, 3, 4], center: [0, 0, 5] };

function shadowPreset(s: ShadowInfo | null): string {
  if (s === null) return "";
  if (!s.visible) return "none";
  const found = Object.entries(SHADOW_SHAPES).find(([, [dir, dist, blur]]) => Math.abs((s.angle ?? 0) - dir) < 0.5 && Math.abs((s.distPt ?? 0) - dist) < 0.05 && Math.abs((s.blurPt ?? 0) - blur) < 0.05);
  return found ? found[0] : "custom";
}

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
  status: Status;
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
export function FormatPane({ status, engine, version, run, onClose }: Props) {
  const [info, setInfo] = useState<Info | null>(null);
  useEffect(() => {
    setInfo(engine ? ((engine.formatInfo() as Info | null) ?? null) : null);
  }, [engine, version]);

  const fill = info?.fill ?? null;
  const line = info?.line ?? null;
  const setFill = (spec: FillSpec) => run({ kind: "fillSpec", spec });
  const setLine = (spec: LineSpec) => run({ kind: "lineSpec", spec });
  const setShadow = (spec: ShadowSpec) => run({ kind: "shadowSpec", spec });
  const setTextBox = (spec: TextBoxSpec) => run({ kind: "textBoxSpec", spec });
  // --- size and position (centimetres on screen, EMU in the engine)
  const f = status.frame;
  const cm = (emu: number | undefined | null) => (emu === undefined || emu === null ? undefined : emu / EMU_PER_CM);
  const setFrame = (patch: Partial<{ x: number; y: number; w: number; h: number; rot: number }>) => f && run({ kind: "setFrame", x: f.x, y: f.y, w: f.w, h: f.h, rot: f.rot, ...patch });
  const aspect = info?.lockAspect === true && f && f.w > 0 && f.h > 0 ? f.h / f.w : null;
  const setWidth = (cmValue: number) => {
    const w = Math.max(1, Math.round(cmValue * EMU_PER_CM));
    setFrame(aspect ? { w, h: Math.max(1, Math.round(w * aspect)) } : { w });
  };
  const setHeight = (cmValue: number) => {
    const h = Math.max(1, Math.round(cmValue * EMU_PER_CM));
    setFrame(aspect ? { h, w: Math.max(1, Math.round(h / aspect)) } : { h });
  };
  const tb = info?.textBox ?? null;
  // The alternative text of a single selected object (read again when the selection changes).
  const alt = engine && status.selectionCount === 1 ? (engine.altText() as { text: string; decorative: boolean } | null) : null;
  const shadow = info?.shadow ?? null;
  const preset = shadowPreset(shadow);

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
          {info.canShadow && (
            <Section title={t("format.effectsSection")}>
              <div className="fp-subhead">{t("format.shadow")}</div>
              <label className="fp-field wide">
                <span>{t("format.shadowPreset")}</span>
                <select value={preset} onChange={(e) => run({ kind: "shadow", preset: e.target.value })}>
                  {preset === "" && (
                    <option value="" disabled>
                      —
                    </option>
                  )}
                  {SHADOWS.map(([v, l]) => (
                    <option key={v} value={v}>
                      {l}
                    </option>
                  ))}
                  {preset === "custom" && <option value="custom">{t("format.shadowCustom")}</option>}
                </select>
              </label>
              {shadow === null && <div className="pane-empty">{t("format.mixed")}</div>}
              {shadow?.visible && (
                <>
                  <div className="fp-row">
                    <span className="fp-label">{t("format.color")}</span>
                    <ColorPicker icon={<Swatch color={shadow.color} alpha={shadow.alpha} />} label="" value={shadow.color} onPick={(color) => color && setShadow({ color, alpha: shadow.alpha })} />
                  </div>
                  <Percent label={t("format.transparency")} value={Math.round((1 - shadow.alpha!) * 100)} onCommit={(p) => setShadow({ alpha: 1 - p / 100 })} />
                  <Num label={t("format.blur")} unit="pt" value={shadow.blurPt} min={0} max={100} step={0.5} onCommit={(blurPt) => setShadow({ blurPt })} />
                  <Num label={t("format.angle")} unit="°" value={shadow.angle} min={0} max={359} step={5} onCommit={(angle) => setShadow({ angle })} />
                  <Num label={t("format.distance")} unit="pt" value={shadow.distPt} min={0} max={200} step={0.5} onCommit={(distPt) => setShadow({ distPt })} />
                </>
              )}
            </Section>
          )}
          <Section title={t("format.sizeSection")}>
            <div className="fp-subhead">{t("format.sizeGroup")}</div>
            <Num label={t("design.height")} unit={t("design.cmUnit")} value={cm(f?.h)} min={0.01} step={0.1} disabled={!f} onCommit={setHeight} />
            <Num label={t("design.width")} unit={t("design.cmUnit")} value={cm(f?.w)} min={0.01} step={0.1} disabled={!f} onCommit={setWidth} />
            <Num label={t("format.rotation")} unit="°" value={f?.single ? f.rot : undefined} step={1} disabled={!f?.single} onCommit={(rot) => setFrame({ rot })} />
            <label className="fp-check">
              <input type="checkbox" checked={info.lockAspect === true} ref={(el) => {
                  if (el) el.indeterminate = info.lockAspect === null;
                }} onChange={(e) => run({ kind: "lockAspect", on: e.target.checked })} />
              {t("format.lockAspect")}
            </label>
            <div className="fp-subhead">{t("format.positionGroup")}</div>
            <Num label={t("format.horizontal")} unit={t("design.cmUnit")} value={cm(f?.x)} step={0.1} disabled={!f} onCommit={(v) => setFrame({ x: Math.round(v * EMU_PER_CM) })} />
            <Num label={t("format.vertical")} unit={t("design.cmUnit")} value={cm(f?.y)} step={0.1} disabled={!f} onCommit={(v) => setFrame({ y: Math.round(v * EMU_PER_CM) })} />
            {tb && (
              <>
                <div className="fp-subhead">{t("format.textBoxGroup")}</div>
                <label className="fp-field wide">
                  <span>{t("format.verticalAlign")}</span>
                  <select value={tb.anchor ?? ""} onChange={(e) => setTextBox({ anchor: e.target.value as "top" | "middle" | "bottom" })}>
                    {tb.anchor === null && (
                      <option value="" disabled>
                        —
                      </option>
                    )}
                    <option value="top">{t("format.anchorTop")}</option>
                    <option value="middle">{t("format.anchorMiddle")}</option>
                    <option value="bottom">{t("format.anchorBottom")}</option>
                  </select>
                </label>
                <div className="fp-subhead">{t("format.margins")}</div>
                <div className="fp-margins">
                  {(["left", "right", "top", "bottom"] as const).map((side) => (
                    <Num key={side} label={t(`format.${side}`)} unit={t("design.cmUnit")} value={cm(tb[side])} min={0} step={0.1} onCommit={(v) => setTextBox({ [side]: Math.round(v * EMU_PER_CM) })} />
                  ))}
                </div>
                <label className="fp-check">
                  <input type="checkbox" checked={tb.wrap === true} ref={(el) => {
                    if (el) el.indeterminate = tb.wrap === null;
                  }} onChange={(e) => setTextBox({ wrap: e.target.checked })} />
                  {t("format.wrapText")}
                </label>
                <label className="fp-field wide">
                  <span>{t("format.autofit")}</span>
                  <select value={tb.autoGrow === null ? "" : tb.autoGrow ? "shape" : "none"} onChange={(e) => setTextBox({ autoGrow: e.target.value === "shape" })}>
                    {tb.autoGrow === null && (
                      <option value="" disabled>
                        —
                      </option>
                    )}
                    <option value="none">{t("format.autofitNone")}</option>
                    <option value="shape">{t("format.autofitShape")}</option>
                  </select>
                </label>
              </>
            )}
            {alt && <AltTextBox key={version} initial={alt} run={run} />}
            <div className="fp-subhead">{t("format.propertiesGroup")}</div>
            <label className="fp-check">
              <input type="checkbox" checked={status.locked === true} onChange={(e) => run({ kind: "lock", on: e.target.checked })} />
              {t("format.lockObject")}
            </label>
          </Section>
        </div>
      )}
    </aside>
  );
}

/** Alternative text of the selected object: the description (committed when the field is left) and the decorative mark. */
function AltTextBox({ initial, run }: { initial: { text: string; decorative: boolean }; run: (c: Command) => void }) {
  const [text, setText] = useState(initial.text);
  return (
    <>
      <div className="fp-subhead">{t("ribbon.altText")}</div>
      <textarea
        className="fp-alt"
        rows={3}
        value={text}
        disabled={initial.decorative}
        placeholder={t("alt.example")}
        aria-label={t("ribbon.altText")}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => text.trim() !== initial.text && run({ kind: "altText", text, decorative: false })}
      />
      <label className="fp-check">
        <input type="checkbox" checked={initial.decorative} onChange={(e) => run({ kind: "altText", text: e.target.checked ? "" : text, decorative: e.target.checked })} />
        {t("checker.markDecorative")}
      </label>
    </>
  );
}
