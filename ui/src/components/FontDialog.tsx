import { useState } from "react";
import { t } from "../i18n";
import type { Command, FontSpec } from "../lib/commands";
import type { Status } from "../lib/status";
import { Combobox } from "./Combobox";

interface Props {
  status: Status;
  fonts: string[];
  run: (c: Command) => void;
  onClose: () => void;
}

/** The Font dialog: character formatting of the selected text, applied in one undo step. */
export function FontDialog({ status, fonts, run, onClose }: Props) {
  const [spec, setSpec] = useState<FontSpec>({
    family: status.fontFamily ?? "",
    eaFamily: status.eaFont ?? "",
    size: status.fontSize ?? 18,
    bold: !!status.bold,
    italic: !!status.italic,
    underline: !!status.underline,
    strike: !!status.strike,
    baseline: (status.baseline ?? 0) > 0 ? "super" : (status.baseline ?? 0) < 0 ? "sub" : "normal",
    spacing: status.charSpacing ?? 0,
    color: status.textColor ?? "#000000",
  });
  // Only what the user changed is applied: a selection with mixed formatting keeps the rest.
  const [changed, setChanged] = useState<Set<keyof FontSpec>>(new Set());
  const set = <K extends keyof FontSpec>(key: K, value: FontSpec[K]) => {
    setSpec((s) => ({ ...s, [key]: value }));
    setChanged((c) => new Set(c).add(key));
  };
  const style = spec.bold ? (spec.italic ? "boldItalic" : "bold") : spec.italic ? "italic" : "regular";
  const setStyle = (v: string) => {
    set("bold", v === "bold" || v === "boldItalic");
    set("italic", v === "italic" || v === "boldItalic");
  };
  const apply = () => {
    const out: Record<string, unknown> = {};
    for (const k of changed) out[k] = spec[k];
    if (changed.size > 0) run({ kind: "fontSpec", spec: out as FontSpec });
    onClose();
  };
  const sizeOk = spec.size !== undefined && Number.isFinite(spec.size) && spec.size > 0;
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="hf-dialog para-dialog"
        role="dialog"
        aria-label={t("font.title")}
        aria-modal="true"
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) apply();
        }}
      >
        <div className="hf-title">{t("font.title")}</div>
        <div className="hf-row">
          <span className="para-label">{t("font.latin")}</span>
          <Combobox className="hf-text wide" value={spec.family ?? ""} options={fonts} previewFonts ariaLabel={t("font.latin")} onCommit={(v) => set("family", v)} />
        </div>
        <div className="hf-row">
          <span className="para-label">{t("font.eastAsian")}</span>
          <Combobox className="hf-text wide" value={spec.eaFamily ?? ""} options={fonts} previewFonts ariaLabel={t("font.eastAsian")} onCommit={(v) => set("eaFamily", v)} />
        </div>
        <label className="hf-row">
          <span className="para-label">{t("font.style")}</span>
          <select className="para-input" value={style} onChange={(e) => setStyle(e.target.value)}>
            <option value="regular">{t("font.regular")}</option>
            <option value="italic">{t("font.italic")}</option>
            <option value="bold">{t("font.bold")}</option>
            <option value="boldItalic">{t("font.boldItalic")}</option>
          </select>
        </label>
        <label className="hf-row">
          <span className="para-label">{t("font.size")}</span>
          <input className="hf-text para-input" type="number" min={1} max={1000} step={0.5} value={spec.size ?? ""} onChange={(e) => set("size", e.target.value === "" ? undefined : Number(e.target.value))} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("font.color")}</span>
          <input className="para-color" type="color" value={spec.color} onChange={(e) => set("color", e.target.value)} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("font.offset")}</span>
          <select className="para-input" value={spec.baseline} onChange={(e) => set("baseline", e.target.value as FontSpec["baseline"])}>
            <option value="normal">{t("font.normal")}</option>
            <option value="super">{t("font.superscript")}</option>
            <option value="sub">{t("font.subscript")}</option>
          </select>
        </label>
        <label className="hf-row">
          <span className="para-label">{t("font.spacing")}</span>
          <input className="hf-text para-input" type="number" min={-20} max={100} step={0.5} value={spec.spacing ?? ""} onChange={(e) => set("spacing", e.target.value === "" ? undefined : Number(e.target.value))} />
        </label>
        <div className="hf-row para-checks">
          <label className="hf-row">
            <input type="checkbox" checked={!!spec.underline} onChange={(e) => set("underline", e.target.checked)} />
            {t("font.underline")}
          </label>
          <label className="hf-row">
            <input type="checkbox" checked={!!spec.strike} onChange={(e) => set("strike", e.target.checked)} />
            {t("font.strike")}
          </label>
        </div>
        <div className="hf-buttons">
          <button onClick={onClose}>{t("save.cancel")}</button>
          <button className="primary" disabled={!sizeOk} onClick={apply}>
            {t("common.ok")}
          </button>
        </div>
      </div>
    </div>
  );
}
