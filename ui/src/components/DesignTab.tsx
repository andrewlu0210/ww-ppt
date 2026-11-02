import { t } from "../i18n";
import { useState, type ReactNode } from "react";
import { icons } from "./icons";
import { Popover } from "./Popover";
import type { Command } from "../lib/commands";

/** The deck's theme (`App::themeInfo`); colors in dk1 lt1 dk2 lt2 accent1-6 hlink folHlink order. */
export interface ThemeInfo {
  colorScheme: string;
  colors: string[];
  fontScheme: string;
  major: string;
  minor: string;
}

/** A built-in color variant; colors are dk2 lt2 accent1-6 hlink folHlink. */
export interface ColorSchemeInfo {
  name: string;
  label: string;
  colors: string[];
}

export interface FontSchemeInfo {
  name: string;
  major: string;
  minor: string;
}

/** The current slide's background (`App::background`). */
export interface BackgroundInfo {
  kind: "solid" | "gradient" | "picture";
  color: string;
  colors: [string, string];
  radial: boolean;
  angle: number;
  own: boolean;
  hideGraphics: boolean;
}

const EMU_PER_CM = 360_000;
/** Named slide sizes in EMU, like PowerPoint's 投影片大小 menu. */
const SIZES: [string, number, number][] = [
  [t("design.widescreen"), 12_192_000, 6_858_000],
  [t("design.standard"), 9_144_000, 6_858_000],
  [t("design.onScreen"), 9_144_000, 5_715_000],
  [t("design.a4"), 9_906_000, 6_858_000],
];

const keepFocus = (e: React.MouseEvent) => e.preventDefault();

function DropButton({ icon, label, open, toggle }: { icon: ReactNode; label: string; open: boolean; toggle: () => void }) {
  return (
    <button className={open ? "rb-btn large active" : "rb-btn large"} title={label} onMouseDown={keepFocus} onClick={toggle}>
      {icon}
      <span>{label} ▾</span>
    </button>
  );
}

function ColorsMenu({ theme, schemes, run }: { theme: ThemeInfo | null; schemes: ColorSchemeInfo[]; run: (c: Command) => void }) {
  return (
    <Popover button={(open, toggle) => <DropButton icon={icons.colors} label={t("design.colors")} open={open} toggle={toggle} />}>
      {(close) => (
        <div className="menu-panel scheme-menu">
          {schemes.map((s) => (
            <button
              key={s.name}
              className={theme?.colorScheme === s.name ? "menu-item active" : "menu-item"}
              onMouseDown={keepFocus}
              onClick={() => {
                run({ kind: "colorScheme", name: s.name });
                close();
              }}
            >
              <span className="scheme-swatches">
                {/* dk2, lt2 and the six accents, as PowerPoint previews them. */}
                {s.colors.slice(0, 8).map((c, i) => (
                  <span key={i} style={{ background: c }} />
                ))}
              </span>
              {s.label}
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}

function FontsMenu({ theme, schemes, run }: { theme: ThemeInfo | null; schemes: FontSchemeInfo[]; run: (c: Command) => void }) {
  return (
    <Popover button={(open, toggle) => <DropButton icon={icons.fonts} label={t("group.font")} open={open} toggle={toggle} />}>
      {(close) => (
        <div className="menu-panel scheme-menu">
          {schemes.map((s) => (
            <button
              key={s.name}
              className={theme?.fontScheme === s.name ? "menu-item active" : "menu-item"}
              onMouseDown={keepFocus}
              onClick={() => {
                run({ kind: "fontScheme", name: s.name });
                close();
              }}
            >
              <span className="font-sample" style={{ fontFamily: `"${s.major}"` }}>
                Aa
              </span>
              <span className="font-names">
                <span className="font-scheme-name">{s.name}</span>
                <span style={{ fontFamily: `"${s.major}"` }}>{s.major}</span>
                <span style={{ fontFamily: `"${s.minor}"` }}>{s.minor}</span>
              </span>
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}

const cm = (emu: number) => Math.round((emu / EMU_PER_CM) * 100) / 100;

function SlideSizeMenu({ size, run }: { size: [number, number]; run: (c: Command) => void }) {
  // A size waiting for the fit choice, or the custom size form.
  const [pending, setPending] = useState<[number, number] | null>(null);
  const [custom, setCustom] = useState<[string, string] | null>(null);
  const [cw, ch] = size;
  const choose = (w: number, h: number, close: () => void) => {
    w = Math.round(w);
    h = Math.round(h);
    if (w === cw && h === ch) return close();
    // Same shape: content simply scales.
    if (Math.abs(w / h - cw / ch) < 1e-3) {
      run({ kind: "slideSize", width: w, height: h, fit: "fit" });
      return close();
    }
    setPending([w, h]);
  };
  const reset = () => {
    setPending(null);
    setCustom(null);
  };
  return (
    <Popover
      button={(open, toggle) => (
        <DropButton
          icon={icons.slideSize}
          label={t("design.slideSize")}
          open={open}
          toggle={() => {
            reset();
            toggle();
          }}
        />
      )}
    >
      {(close) => {
        const done = () => {
          reset();
          close();
        };
        if (pending) {
          const fit = (f: "maximize" | "fit") => {
            run({ kind: "slideSize", width: pending[0], height: pending[1], fit: f });
            done();
          };
          return (
            <div className="hf-dialog" role="dialog" aria-label={t("design.scaleTitle")}>
              <div className="hf-title">{t("design.scaleQuestion")}</div>
              <div className="fit-choices">
                <button className="fit-choice" onClick={() => fit("maximize")}>
                  <span className="fit-preview maximize" />
                  {t("design.maximize")}
                </button>
                <button className="fit-choice" onClick={() => fit("fit")}>
                  <span className="fit-preview ensure" />
                  {t("design.ensureFit")}
                </button>
              </div>
              <div className="hf-buttons">
                <button onClick={done}>{t("save.cancel")}</button>
              </div>
            </div>
          );
        }
        if (custom) {
          const [w, h] = custom.map(Number);
          const ok = w >= 2.54 && h >= 2.54 && w <= 142.24 && h <= 142.24;
          return (
            <div className="hf-dialog" role="dialog" aria-label={t("design.slideSize")}>
              <div className="hf-title">{t("design.customSize")}</div>
              <label className="hf-row">
                {t("design.width")}
                <input className="hf-text" type="number" step="0.01" value={custom[0]} onChange={(e) => setCustom([e.target.value, custom[1]])} />
                {t("design.cmUnit")}
              </label>
              <label className="hf-row">
                {t("design.height")}
                <input className="hf-text" type="number" step="0.01" value={custom[1]} onChange={(e) => setCustom([custom[0], e.target.value])} />
                {t("design.cmUnit")}
              </label>
              <div className="hf-buttons">
                <button onClick={done}>{t("save.cancel")}</button>
                <button className="primary" disabled={!ok} onClick={() => choose(w * EMU_PER_CM, h * EMU_PER_CM, done)}>
                  {t("common.ok")}
                </button>
              </div>
            </div>
          );
        }
        return (
          <div className="menu-panel">
            {SIZES.map(([label, w, h]) => (
              <button key={label} className={Math.round(w) === cw && Math.round(h) === ch ? "menu-item active" : "menu-item"} onMouseDown={keepFocus} onClick={() => choose(w, h, done)}>
                {label}
                <span className="menu-note">
                  {t("design.sizeCm", { w: cm(w), h: cm(h) })}
                </span>
              </button>
            ))}
            <div className="spell-sep" />
            <button className="menu-item" onMouseDown={keepFocus} onClick={() => setCustom([String(cm(cw)), String(cm(ch))])}>
              {t("design.customSizeMenu")}
            </button>
          </div>
        );
      }}
    </Popover>
  );
}

/** Gradient directions offered, as PowerPoint's linear angles. */
const ANGLES: [number, string][] = [
  [90, t("design.topToBottom")],
  [270, t("design.bottomToTop")],
  [0, t("design.leftToRight")],
  [180, t("design.rightToLeft")],
  [45, t("design.diagDown")],
  [135, t("design.diagUp")],
];

/** 背景格式: solid, gradient or picture fill for the current slide, applied as it is edited. */
function BackgroundPanel({ initial, theme, run }: { initial: BackgroundInfo; theme: ThemeInfo | null; run: (c: Command) => void }) {
  const [bg, setBg] = useState(initial);
  const update = (next: BackgroundInfo, c: Command) => {
    setBg({ ...next, own: true });
    run(c);
  };
  const solid = (color: string) => update({ ...bg, kind: "solid", color }, { kind: "background", fill: "solid", color });
  const gradient = (colors: [string, string], radial: boolean, angle: number) =>
    update({ ...bg, kind: "gradient", colors, radial, angle }, { kind: "background", fill: "gradient", start: colors[0], end: colors[1], radial, angle });
  // lt1, dk1, lt2, dk2, accents: PowerPoint's palette order.
  const palette = theme ? [theme.colors[1], theme.colors[0], theme.colors[3], theme.colors[2], ...theme.colors.slice(4, 10)] : [];
  const swatches = (value: string, pick: (c: string) => void) => (
    <div className="bg-swatches">
      {palette.map((c, i) => (
        <button key={i} className={c === value ? "swatch active" : "swatch"} style={{ background: c }} title={c} onMouseDown={keepFocus} onClick={() => pick(c)} />
      ))}
      <input type="color" title={t("design.moreColors")} value={value.toLowerCase()} onChange={(e) => pick(e.target.value.toUpperCase())} />
    </div>
  );
  const kind = (k: BackgroundInfo["kind"]) => {
    if (k === "solid") solid(bg.color);
    else if (k === "gradient") gradient(bg.kind === "gradient" ? bg.colors : [bg.color, palette[4] ?? "#4472C4"], bg.radial, bg.angle);
    else run({ kind: "backgroundPicture" });
  };
  return (
    <div className="hf-dialog bg-panel" role="dialog" aria-label={t("design.formatBackground")}>
      <div className="hf-title">{t("design.fill")}</div>
      {(["solid", "gradient", "picture"] as const).map((k) => (
        <label key={k} className="hf-row">
          <input type="radio" checked={bg.kind === k} onChange={() => kind(k)} />
          {k === "solid" ? t("design.solidFill") : k === "gradient" ? t("design.gradientFill") : t("design.pictureFill")}
        </label>
      ))}
      {bg.kind === "solid" && (
        <div className="hf-indent">
          {t("design.color")}
          {swatches(bg.color, solid)}
        </div>
      )}
      {bg.kind === "gradient" && (
        <div className="hf-indent">
          {t("design.gradientStart")}
          {swatches(bg.colors[0], (c) => gradient([c, bg.colors[1]], bg.radial, bg.angle))}
          {t("design.gradientEnd")}
          {swatches(bg.colors[1], (c) => gradient([bg.colors[0], c], bg.radial, bg.angle))}
          <label className="hf-row">
            {t("design.direction")}
            <select value={bg.radial ? "radial" : String(bg.angle)} onChange={(e) => (e.target.value === "radial" ? gradient(bg.colors, true, 0) : gradient(bg.colors, false, Number(e.target.value)))}>
              {ANGLES.map(([a, label]) => (
                <option key={a} value={a}>
                  {t("design.linear", { dir: label })}
                </option>
              ))}
              <option value="radial">{t("design.radial")}</option>
            </select>
          </label>
        </div>
      )}
      {bg.kind === "picture" && (
        <div className="hf-indent">
          <button className="bg-button" onClick={() => run({ kind: "backgroundPicture" })}>
            {t("design.insertPicture")}
          </button>
        </div>
      )}
      <label className="hf-row sep">
        <input
          type="checkbox"
          checked={bg.hideGraphics}
          onChange={(e) => {
            setBg({ ...bg, hideGraphics: e.target.checked });
            run({ kind: "hideBackgroundGraphics", on: e.target.checked });
          }}
        />
        {t("design.hideBackgroundGraphics")}
      </label>
      <div className="hf-buttons">
        <button
          disabled={!bg.own}
          onClick={() => {
            run({ kind: "resetBackground" });
            setBg({ ...bg, own: false });
          }}
        >
          {t("design.resetBackground")}
        </button>
        <button className="primary" onClick={() => run({ kind: "backgroundToAll" })}>
          {t("trans.applyAll")}
        </button>
      </div>
    </div>
  );
}

interface Props {
  theme: ThemeInfo | null;
  colorSchemes: ColorSchemeInfo[];
  fontSchemes: FontSchemeInfo[];
  slideSize: [number, number];
  loadBackground: () => BackgroundInfo | null;
  run: (c: Command) => void;
}

/** 設計: theme color and font variants, slide size and background. */
export function DesignTab({ theme, colorSchemes, fontSchemes, slideSize, loadBackground, run }: Props) {
  return (
    <>
      <div className="rb-group">
        <div className="rb-items">
          <div className="theme-current" title={theme ? `${theme.colorScheme}／${theme.fontScheme}` : undefined}>
            <span className="theme-aa" style={{ fontFamily: theme ? `"${theme.major}"` : undefined, color: theme?.colors[2] }}>
              Aa
            </span>
            <span className="scheme-swatches">{theme?.colors.slice(4, 10).map((c, i) => <span key={i} style={{ background: c }} />)}</span>
          </div>
          <ColorsMenu theme={theme} schemes={colorSchemes} run={run} />
          <FontsMenu theme={theme} schemes={fontSchemes} run={run} />
        </div>
      </div>
      <div className="rb-group">
        <div className="rb-items">
          <SlideSizeMenu size={slideSize} run={run} />
          <Popover button={(open, toggle) => <DropButton icon={icons.background} label={t("design.formatBackground")} open={open} toggle={toggle} />}>
            {() => {
              const bg = loadBackground();
              return bg ? <BackgroundPanel initial={bg} theme={theme} run={run} /> : <div className="hf-dialog">{t("design.noSlides")}</div>;
            }}
          </Popover>
        </div>
      </div>
    </>
  );
}
