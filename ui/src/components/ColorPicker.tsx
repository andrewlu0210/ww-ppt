import type { ReactNode } from "react";
import { Popover } from "./Popover";

// The deck's theme colors (Office's until a deck says otherwise) with PowerPoint's lighter/darker variations.
let THEME = ["#FFFFFF", "#000000", "#E7E6E6", "#44546A", "#4472C4", "#ED7D31", "#A5A5A5", "#FFC000", "#5B9BD5", "#70AD47"];

/** Uses a theme's colors (dk1 lt1 dk2 lt2 accent1-6 ...) in the palette, in PowerPoint's lt1 dk1 lt2 dk2 order. */
export function setThemePalette(colors: string[]) {
  if (colors.length >= 10) THEME = [colors[1], colors[0], colors[3], colors[2], ...colors.slice(4, 10)];
}
const STANDARD = ["#C00000", "#FF0000", "#FFC000", "#FFFF00", "#92D050", "#00B050", "#00B0F0", "#0070C0", "#002060", "#7030A0"];

function mix(hex: string, target: number, amount: number): string {
  const v = parseInt(hex.slice(1), 16);
  const ch = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((c) => Math.round(c + (target - c) * amount));
  return "#" + ch.map((c) => c.toString(16).padStart(2, "0")).join("").toUpperCase();
}

const VARIANTS = [
  (c: string) => mix(c, 255, 0.8),
  (c: string) => mix(c, 255, 0.6),
  (c: string) => mix(c, 255, 0.4),
  (c: string) => mix(c, 0, 0.25),
  (c: string) => mix(c, 0, 0.5),
];

interface Props {
  icon: ReactNode;
  label: string;
  value?: string;
  disabled?: boolean;
  /** Label of the "none" entry (no fill / no outline); omitted when not applicable. */
  noneLabel?: string;
  onPick: (color: string) => void;
}

export function ColorPicker({ icon, label, value, disabled, noneLabel, onPick }: Props) {
  return (
    <Popover
      button={(open, toggle) => (
        <button className={open ? "rb-btn active" : "rb-btn"} disabled={disabled} title={label} onClick={toggle} onMouseDown={(e) => e.preventDefault()}>
          <span className="color-icon">
            {icon}
            <span className="color-bar" style={{ background: value || "transparent", borderColor: value ? value : "#bbb" }} />
          </span>
          <span>{label}</span>
        </button>
      )}
    >
      {(close) => {
        const pick = (c: string) => {
          onPick(c);
          close();
        };
        return (
          <div className="color-panel" onMouseDown={(e) => e.preventDefault()}>
            <div className="color-heading">佈景主題色彩</div>
            <div className="color-grid">
              {THEME.map((c, i) => (
                <button key={i} className="swatch" style={{ background: c }} title={c} onClick={() => pick(c)} />
              ))}
              {VARIANTS.flatMap((v, row) =>
                THEME.map((c, i) => {
                  const shade = v(c);
                  return <button key={`${row}-${i}`} className="swatch" style={{ background: shade }} title={shade} onClick={() => pick(shade)} />;
                }),
              )}
            </div>
            <div className="color-heading">標準色彩</div>
            <div className="color-grid">
              {STANDARD.map((c) => (
                <button key={c} className="swatch" style={{ background: c }} title={c} onClick={() => pick(c)} />
              ))}
            </div>
            {noneLabel && (
              <button className="color-none" onClick={() => pick("")}>
                {noneLabel}
              </button>
            )}
          </div>
        );
      }}
    </Popover>
  );
}
