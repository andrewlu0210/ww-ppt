import type { Status } from "../lib/status";
import type { Command } from "../lib/commands";
import { t } from "../i18n";
import { icons } from "./icons";

interface Props {
  status: Status;
  fontsLoading: boolean;
  notesOpen: boolean;
  viewMode: "normal" | "sorter";
  /** Thumbnail scale in the slide sorter (1 = 100%). */
  sorterZoom: number;
  /** How many accessibility problems the checker found. */
  a11yIssues: number;
  run: (c: Command) => void;
}

export function StatusBar({ status, fontsLoading, notesOpen, viewMode, sorterZoom, a11yIssues, run }: Props) {
  const view = (mode: "normal" | "sorter", label: string) => (
    <button className={viewMode === mode ? "toggle on" : "toggle"} onClick={() => run({ kind: "viewMode", mode })} onMouseDown={(e) => e.preventDefault()} aria-pressed={viewMode === mode}>
      {label}
    </button>
  );
  return (
    <footer className="status-bar">
      <span>{t("status.slide", { current: status.slideCount ? status.slideIndex + 1 : 0, total: status.slideCount })}</span>
      {status.editingText && <span>{t("status.editingText")}</span>}
      {!status.editingText && status.selectionName && <span>{t("status.selected", { name: status.selectionName })}</span>}
      {fontsLoading && <span>{t("status.loadingFonts")}</span>}
      <button className="a11y-status" onClick={() => run({ kind: "checker", open: true })} onMouseDown={(e) => e.preventDefault()} title={t("status.checkAccessibility")}>
        {a11yIssues === 0 ? t("status.a11yGood") : t("status.a11yIssues", { count: a11yIssues })}
      </button>
      <span className="spacer" />
      <span className="muted">WebGPU · vello · {status.frameMs.toFixed(1)} ms</span>
      <button className={notesOpen ? "toggle on" : "toggle"} onClick={() => run({ kind: "toggleNotes" })} onMouseDown={(e) => e.preventDefault()} aria-pressed={notesOpen} title={t("status.toggleNotes")}>
        {t("status.notes")}
      </button>
      {view("normal", t("status.normal"))}
      {view("sorter", t("status.sorter"))}
      <button className="icon-btn" disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false })} onMouseDown={(e) => e.preventDefault()} title={t("ribbon.fromCurrent")} aria-label={t("ribbon.fromCurrent")}>
        {icons.playCurrent}
      </button>
      <button onClick={() => run({ kind: "zoom", factor: 0.8 })} onMouseDown={(e) => e.preventDefault()} aria-label={t("status.zoomOut")}>
        −
      </button>
      <button className="zoom-value" onClick={() => run({ kind: "zoom", factor: "fit" })} onMouseDown={(e) => e.preventDefault()} title={t("status.fit")}>
        {Math.round(viewMode === "sorter" ? sorterZoom * 100 : status.zoomPercent)}%
      </button>
      <button onClick={() => run({ kind: "zoom", factor: 1.25 })} onMouseDown={(e) => e.preventDefault()} aria-label={t("status.zoomIn")}>
        +
      </button>
      <button className="icon-btn" onClick={() => run({ kind: "zoom", factor: "fit" })} onMouseDown={(e) => e.preventDefault()} title={t("status.fit")} aria-label={t("status.fit")}>
        {icons.fit}
      </button>
    </footer>
  );
}
