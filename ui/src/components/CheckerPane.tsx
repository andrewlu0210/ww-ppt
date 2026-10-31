import { t } from "../i18n";
import { useState } from "react";
import type { App } from "../wasm/ppt_wasm";
import type { Command } from "../lib/commands";
import { focusCanvas } from "./CanvasHost";
import { focusables, moveFocus, ownsArrows } from "../lib/keyboard";

/** A result of `App::checkAccessibility`. */
export interface A11yIssue {
  slide: number;
  shape: number | null;
  severity: "error" | "warning" | "tip";
  kind: "altText" | "title" | "duplicateTitle" | "contrast" | "headerRow" | "readingOrder";
  message: string;
}

const SEVERITIES: [A11yIssue["severity"], string][] = [
  ["error", t("checker.error")],
  ["warning", t("checker.warning")],
  ["tip", t("checker.tip")],
];

const KIND_TITLES: Record<A11yIssue["kind"], string> = {
  altText: t("checker.altText"),
  title: t("checker.title"),
  duplicateTitle: t("checker.duplicateTitle"),
  contrast: t("checker.contrast"),
  headerRow: t("checker.headerRow"),
  readingOrder: t("checker.readingOrder"),
};

/** Why each issue matters and how to fix it. */
const KIND_HELP: Record<A11yIssue["kind"], string> = {
  altText: t("checker.altTextWhy"),
  title: t("checker.titleWhy"),
  duplicateTitle: t("checker.duplicateTitleWhy"),
  contrast: t("checker.contrastWhy"),
  headerRow: t("checker.headerRowWhy"),
  readingOrder: t("checker.readingOrderWhy"),
};

interface Props {
  engine: App | null;
  issues: A11yIssue[];
  run: (c: Command) => void;
  onChange: () => void;
  onClose: () => void;
}

/** Inline fix for a missing description: write it, or mark the object decorative. */
function AltTextFix({ engine, issue, onDone }: { engine: App; issue: A11yIssue; onDone: () => void }) {
  const [text, setText] = useState("");
  const apply = (decorative: boolean) => {
    if (issue.shape === null) return;
    engine.setSlide(issue.slide);
    engine.selectInPane(issue.shape, false);
    engine.setAltText(decorative ? "" : text, decorative);
    onDone();
  };
  return (
    <div className="check-fix">
      <textarea rows={2} value={text} placeholder={t("checker.describe")} aria-label={t("ribbon.altText")} onChange={(e) => setText(e.target.value)} />
      <div className="check-fix-buttons">
        <button disabled={!text.trim()} onClick={() => apply(false)}>
          {t("checker.addAltText")}
        </button>
        <button onClick={() => apply(true)}>{t("checker.markDecorative")}</button>
      </div>
    </div>
  );
}

/**
 * 協助工具: the checker's results grouped by severity. Clicking an issue goes to
 * its slide and object; some issues can be fixed right here.
 */
export function CheckerPane({ engine, issues, run, onChange, onClose }: Props) {
  const [open, setOpen] = useState<string | null>(null);
  const go = (i: A11yIssue) => {
    if (!engine) return;
    engine.setSlide(i.slide);
    if (i.shape !== null) engine.selectInPane(i.shape, false);
    onChange();
  };
  const key = (i: A11yIssue) => `${i.slide}:${i.shape}:${i.kind}`;
  return (
    <aside className="checker-pane" aria-label={t("group.accessibility")}>
      <div className="pane-head">
        <span className="pane-title">{t("group.accessibility")}</span>
        <button className="pane-close" onClick={onClose} aria-label={t("checker.close")} title={t("common.close")}>
          ×
        </button>
      </div>
      <div
        className="check-list"
        onKeyDown={(e) => {
          const el = e.target as Element;
          if ((e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Tab") && !ownsArrows(el)) {
            moveFocus(focusables(e.currentTarget), e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey) ? -1 : 1);
            e.preventDefault();
          } else if (e.key === "Escape") {
            focusCanvas();
          }
        }}
      >
        {issues.length === 0 && <div className="check-ok">{t("checker.allGood")}</div>}
        {SEVERITIES.map(([sev, label]) => {
          const list = issues.filter((i) => i.severity === sev);
          if (list.length === 0) return null;
          return (
            <section key={sev} className={`check-group ${sev}`} aria-label={t("checker.groupAria", { label, count: list.length })}>
              <h3>
                {t("checker.groupCount", { label, count: list.length })}
              </h3>
              <ul>
                {list.map((i) => {
                  const k = key(i);
                  const expanded = open === k;
                  return (
                    <li key={k} className={expanded ? "check-item open" : "check-item"}>
                      <button
                        className="check-summary"
                        aria-expanded={expanded}
                        onClick={() => {
                          setOpen(expanded ? null : k);
                          go(i);
                        }}
                      >
                        <span className="check-kind">{KIND_TITLES[i.kind]}</span>
                        <span className="check-where">{i.message}</span>
                      </button>
                      {expanded && (
                        <div className="check-detail">
                          <p>{KIND_HELP[i.kind]}</p>
                          {i.kind === "altText" && engine && <AltTextFix engine={engine} issue={i} onDone={onChange} />}
                          {i.kind === "headerRow" && (
                            <button
                              onClick={() => {
                                go(i);
                                run({ kind: "tableFlag", flag: "firstRow", on: true });
                              }}
                            >
                              {t("checker.turnOnHeaderRow")}
                            </button>
                          )}
                          {i.kind === "readingOrder" && <button onClick={() => run({ kind: "selectionPane", open: true })}>{t("checker.openSelectionPane")}</button>}
                          {(i.kind === "title" || i.kind === "duplicateTitle" || i.kind === "contrast") && (
                            <button
                              onClick={() => {
                                go(i);
                                focusCanvas();
                              }}
                            >
                              {t("checker.goToSlide")}
                            </button>
                          )}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })}
      </div>
    </aside>
  );
}
