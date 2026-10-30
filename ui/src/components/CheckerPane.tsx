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
  ["error", "錯誤"],
  ["warning", "警告"],
  ["tip", "提示"],
];

const KIND_TITLES: Record<A11yIssue["kind"], string> = {
  altText: "缺少替代文字",
  title: "缺少投影片標題",
  duplicateTitle: "投影片標題重複",
  contrast: "文字對比不足",
  headerRow: "表格沒有標題列",
  readingOrder: "請檢查閱讀順序",
};

/** Why each issue matters and how to fix it. */
const KIND_HELP: Record<A11yIssue["kind"], string> = {
  altText: "螢幕閱讀器無法描述這個物件。請寫一兩句描述，或標示為裝飾用。",
  title: "沒有標題時，使用螢幕閱讀器的人難以找到和分辨投影片。請在標題版面配置區輸入標題。",
  duplicateTitle: "相同的標題讓人分不出投影片。請讓每張投影片的標題不同。",
  contrast: "文字和背景的顏色太接近，視力不佳的人不易閱讀（一般文字需要 4.5:1，大字需要 3:1）。",
  headerRow: "標題列讓螢幕閱讀器能說明每一欄的意思。",
  readingOrder: "螢幕閱讀器依選取窗格由下往上的順序朗讀。請在選取窗格確認順序。",
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
      <textarea rows={2} value={text} placeholder="描述這個物件" aria-label="替代文字" onChange={(e) => setText(e.target.value)} />
      <div className="check-fix-buttons">
        <button disabled={!text.trim()} onClick={() => apply(false)}>
          加入替代文字
        </button>
        <button onClick={() => apply(true)}>標示為裝飾用</button>
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
    <aside className="checker-pane" aria-label="協助工具">
      <div className="pane-head">
        <span className="pane-title">協助工具</span>
        <button className="pane-close" onClick={onClose} aria-label="關閉協助工具" title="關閉">
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
        {issues.length === 0 && <div className="check-ok">沒有發現協助工具問題，使用身心障礙的人應該不難閱讀這份簡報。</div>}
        {SEVERITIES.map(([sev, label]) => {
          const list = issues.filter((i) => i.severity === sev);
          if (list.length === 0) return null;
          return (
            <section key={sev} className={`check-group ${sev}`} aria-label={`${label}，${list.length} 項`}>
              <h3>
                {label}（{list.length}）
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
                              開啟標題列
                            </button>
                          )}
                          {i.kind === "readingOrder" && <button onClick={() => run({ kind: "selectionPane", open: true })}>開啟選取窗格</button>}
                          {(i.kind === "title" || i.kind === "duplicateTitle" || i.kind === "contrast") && (
                            <button
                              onClick={() => {
                                go(i);
                                focusCanvas();
                              }}
                            >
                              前往投影片
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
