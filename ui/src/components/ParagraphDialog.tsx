import { useState } from "react";
import { t } from "../i18n";
import type { Command, ParagraphSpec } from "../lib/commands";
import type { Status } from "../lib/status";

interface Props {
  status: Status;
  run: (c: Command) => void;
  onClose: () => void;
}

const num = (v: string) => (v === "" ? undefined : Number(v));

/** The Paragraph dialog: alignment, indents and spacing, applied in one undo step. */
export function ParagraphDialog({ status, run, onClose }: Props) {
  const first = status.indentFirst ?? 0;
  const [spec, setSpec] = useState<ParagraphSpec>({
    align: status.align ?? "left",
    indentLeft: status.indentLeft ?? 0,
    special: first > 0 ? "first" : first < 0 ? "hanging" : "none",
    by: Math.abs(first),
    spaceBefore: status.spaceBefore ?? 0,
    spaceAfter: status.spaceAfter ?? 0,
    lineSpacing: status.lineSpacing ?? 1,
  });
  const [changed, setChanged] = useState<Set<keyof ParagraphSpec>>(new Set());
  const set = <K extends keyof ParagraphSpec>(key: K, value: ParagraphSpec[K]) => {
    setSpec((s) => ({ ...s, [key]: value }));
    setChanged((c) => new Set(c).add(key));
  };
  const apply = () => {
    const out: Record<string, unknown> = {};
    for (const k of changed) out[k] = spec[k];
    // The first-line setting is one value in the file: send the kind and its amount together.
    if (changed.has("special") || changed.has("by")) {
      out.special = spec.special;
      out.by = spec.by ?? 0;
    }
    if (changed.size > 0) run({ kind: "paragraphSpec", spec: out as ParagraphSpec });
    onClose();
  };
  const valid = [spec.indentLeft, spec.by, spec.spaceBefore, spec.spaceAfter, spec.lineSpacing].every((v) => v !== undefined && Number.isFinite(v) && v >= 0) && (spec.lineSpacing ?? 0) > 0;
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="hf-dialog para-dialog"
        role="dialog"
        aria-label={t("para.title")}
        aria-modal="true"
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement) && valid) apply();
        }}
      >
        <div className="hf-title">{t("para.title")}</div>
        <label className="hf-row">
          <span className="para-label">{t("para.align")}</span>
          <select className="para-input" value={spec.align} onChange={(e) => set("align", e.target.value as ParagraphSpec["align"])}>
            <option value="left">{t("para.alignLeft")}</option>
            <option value="center">{t("para.alignCenter")}</option>
            <option value="right">{t("para.alignRight")}</option>
            <option value="justify">{t("para.alignJustify")}</option>
          </select>
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.indentLeft")}</span>
          <input className="hf-text para-input" type="number" min={0} step={1} value={spec.indentLeft ?? ""} onChange={(e) => set("indentLeft", num(e.target.value))} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.special")}</span>
          <select className="para-input" value={spec.special} onChange={(e) => set("special", e.target.value as ParagraphSpec["special"])}>
            <option value="none">{t("para.none")}</option>
            <option value="first">{t("para.first")}</option>
            <option value="hanging">{t("para.hanging")}</option>
          </select>
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.by")}</span>
          <input className="hf-text para-input" type="number" min={0} step={1} disabled={spec.special === "none"} value={spec.by ?? ""} onChange={(e) => set("by", num(e.target.value))} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.spaceBefore")}</span>
          <input className="hf-text para-input" type="number" min={0} step={1} value={spec.spaceBefore ?? ""} onChange={(e) => set("spaceBefore", num(e.target.value))} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.spaceAfter")}</span>
          <input className="hf-text para-input" type="number" min={0} step={1} value={spec.spaceAfter ?? ""} onChange={(e) => set("spaceAfter", num(e.target.value))} />
        </label>
        <label className="hf-row">
          <span className="para-label">{t("para.lineSpacing")}</span>
          <input className="hf-text para-input" type="number" min={0.1} max={10} step={0.1} value={spec.lineSpacing ?? ""} onChange={(e) => set("lineSpacing", num(e.target.value))} />
        </label>
        <div className="hf-buttons">
          <button onClick={onClose}>{t("save.cancel")}</button>
          <button className="primary" disabled={!valid} onClick={apply}>
            {t("common.ok")}
          </button>
        </div>
      </div>
    </div>
  );
}
