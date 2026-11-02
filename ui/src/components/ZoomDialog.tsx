import { useEffect, useRef, useState } from "react";
import { t } from "../i18n";
import type { Command } from "../lib/commands";

const PRESETS = [400, 200, 100, 66, 50, 33];
// The engine clamps its zoom to 0.1–8 times the fit size.
const MIN_REL = 0.1;
const MAX_REL = 8;

interface Props {
  /** Current absolute zoom in percent. */
  percent: number;
  /** Current zoom relative to the fit size (1 = fit). */
  relative: number;
  /** No manual zoom: the slide follows the window size. */
  fit: boolean;
  run: (c: Command) => void;
  onClose: () => void;
}

/** The status-bar zoom dialog: fit, a few presets, or any percentage. */
export function ZoomDialog({ percent, relative, fit, run, onClose }: Props) {
  const fitPercent = percent / relative;
  const current = Math.round(percent);
  const [value, setValue] = useState(String(current));
  const [choice, setChoice] = useState<"fit" | number | null>(fit ? "fit" : PRESETS.includes(current) ? current : null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.select(), []);

  const minPercent = Math.ceil(fitPercent * MIN_REL);
  const maxPercent = Math.floor(fitPercent * MAX_REL);
  const typed = Number(value);
  const valid = value.trim() !== "" && Number.isFinite(typed) && typed > 0;

  const pick = (c: "fit" | number) => {
    setChoice(c);
    setValue(String(c === "fit" ? Math.round(fitPercent) : c));
  };
  const edit = (text: string) => {
    setValue(text);
    const n = Number(text);
    setChoice(PRESETS.includes(n) ? n : null);
  };
  const apply = () => {
    if (choice === "fit") run({ kind: "zoom", factor: "fit" });
    else if (valid) {
      const target = Math.min(maxPercent, Math.max(minPercent, typed));
      run({ kind: "zoom", factor: target / percent });
    } else return;
    onClose();
  };

  const option = (c: "fit" | number, label: string) => (
    <label className="hf-row zoom-option">
      <input type="radio" name="zoom" checked={choice === c} onChange={() => pick(c)} />
      {label}
    </label>
  );
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="hf-dialog zoom-dialog"
        role="dialog"
        aria-label={t("zoom.title")}
        aria-modal="true"
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter") apply();
        }}
      >
        <div className="hf-title">{t("zoom.title")}</div>
        {option("fit", t("zoom.fit"))}
        {PRESETS.map((p) => option(p, `${p}%`))}
        <label className="hf-row">
          {t("zoom.percent")}
          <input ref={input} className="hf-text zoom-percent" type="number" min={minPercent} max={maxPercent} step={1} value={value} onChange={(e) => edit(e.target.value)} />
        </label>
        <div className="hf-buttons">
          <button onClick={onClose}>{t("save.cancel")}</button>
          <button className="primary" disabled={choice !== "fit" && !valid} onClick={apply}>
            {t("common.ok")}
          </button>
        </div>
      </div>
    </div>
  );
}
