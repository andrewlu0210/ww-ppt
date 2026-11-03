import type { Status } from "../lib/status";
import type { Command } from "../lib/commands";
import { t } from "../i18n";
import { icons } from "./icons";

interface Props {
  status: Status;
  fontsLoading: boolean;
  notesOpen: boolean;
  viewMode: "normal" | "sorter" | "reading";
  /** Thumbnail scale in the slide sorter (1 = 100%). */
  sorterZoom: number;
  /** The current slide has recorded narration. */
  hasNarration: boolean;
  /** How many accessibility problems the checker found. */
  a11yIssues: number;
  run: (c: Command) => void;
  /** Opens the zoom dialog (normal view). */
  onZoomDialog: () => void;
}

export function StatusBar({ status, fontsLoading, notesOpen, viewMode, hasNarration, sorterZoom, a11yIssues, run, onZoomDialog }: Props) {
  const view = (mode: "normal" | "sorter", label: string, icon?: React.ReactNode) => (
    <button
      className={[icon ? "icon-btn" : "", "toggle", viewMode === mode ? "on" : ""].filter(Boolean).join(" ")}
      onClick={() => run({ kind: "viewMode", mode })}
      onMouseDown={(e) => e.preventDefault()}
      aria-pressed={viewMode === mode}
      title={icon ? label : undefined}
      aria-label={icon ? label : undefined}
    >
      {icon ?? label}
    </button>
  );
  const reading = (
    <button className={viewMode === "reading" ? "icon-btn toggle on" : "icon-btn toggle"} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false, reading: true })} onMouseDown={(e) => e.preventDefault()} aria-pressed={viewMode === "reading"} title={t("status.reading")} aria-label={t("status.reading")}>
      {icons.readingView}
    </button>
  );
  return (
    <footer className="status-bar">
      <span>{t("status.slide", { current: status.slideCount ? status.slideIndex + 1 : 0, total: status.slideCount })}</span>
      {status.editingText && <span>{t("status.editingText")}</span>}
      {!status.editingText && status.selectionName && <span>{t("status.selected", { name: status.selectionName })}</span>}
      {fontsLoading && <span>{t("status.loadingFonts")}</span>}
      {hasNarration && <span title={t("record.narrationTip")}>{t("record.narration")}</span>}
      <button className="a11y-status" onClick={() => run({ kind: "checker", open: true })} onMouseDown={(e) => e.preventDefault()} title={t("status.checkAccessibility")}>
        {a11yIssues === 0 ? t("status.a11yGood") : t("status.a11yIssues", { count: a11yIssues })}
      </button>
      <span className="spacer" />
      <span className="muted">WebGPU · vello · {status.frameMs.toFixed(1)} ms</span>
      <button className={notesOpen ? "toggle on" : "toggle"} onClick={() => run({ kind: "toggleNotes" })} onMouseDown={(e) => e.preventDefault()} aria-pressed={notesOpen} title={t("status.toggleNotes")}>
        {t("status.notes")}
      </button>
      {view("normal", t("status.normal"), icons.normalView)}
      {view("sorter", t("status.sorter"), icons.sorterView)}
      {reading}
      <button className="icon-btn" disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false })} onMouseDown={(e) => e.preventDefault()} title={t("ribbon.fromCurrent")} aria-label={t("ribbon.fromCurrent")}>
        {icons.playCurrent}
      </button>
      <button onClick={() => run({ kind: "zoom", factor: 0.8 })} onMouseDown={(e) => e.preventDefault()} aria-label={t("status.zoomOut")}>
        −
      </button>
      <button className="zoom-value" onClick={() => (viewMode === "sorter" ? run({ kind: "zoom", factor: "fit" }) : onZoomDialog())} onMouseDown={(e) => e.preventDefault()} title={viewMode === "sorter" ? t("status.fit") : t("zoom.title")}>
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
