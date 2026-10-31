import { t } from "../i18n";
import type { ReactNode } from "react";
import { Popover } from "./Popover";

/** Symbols by group, as in PowerPoint's 符號 dialog (common subsets). */
const SYMBOLS: [string, string][] = [
  [t("symbol.common"), "©®™°±×÷≠≈≤≥∞√∑∏∫∂µπΩ€£¥¢§¶†‡•…‰′″"],
  [t("symbol.arrows"), "←↑→↓↔↕↖↗↘↙⇐⇑⇒⇓⇔➔➜➤"],
  [t("symbol.shapes"), "■□▲△▼▽◆◇○●◎★☆♠♣♥♦✓✔✕✖✗✘☐☑☒"],
  [t("symbol.punctuation"), "，、。．；：？！「」『』（）【】《》〈〉—～…‥※〃々〒"],
  [t("symbol.numbers"), "①②③④⑤⑥⑦⑧⑨⑩ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩ½⅓¼¾¹²³"],
  [t("symbol.greek"), "αβγδεζηθικλμνξοπρστυφχψωΑΒΓΔΘΛΞΠΣΦΨΩ"],
];

/** 插入 > 符號: a symbol for the caret (or a new text box). */
export function SymbolPicker({ onPick, button }: { onPick: (symbol: string) => void; button: (open: boolean, toggle: () => void) => ReactNode }) {
  return (
    <Popover button={button}>
      {() => (
        <div className="symbol-panel" onMouseDown={(e) => e.preventDefault()}>
          {SYMBOLS.map(([title, chars]) => (
            <div key={title} className="symbol-section">
              <div className="shape-section-title">{title}</div>
              <div className="symbol-grid">
                {Array.from(chars).map((c) => (
                  <button key={c} className="symbol-item" title={`U+${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}`} onClick={() => onPick(c)}>
                    {c}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </Popover>
  );
}
