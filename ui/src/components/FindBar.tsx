import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";
import { focusCanvas } from "./CanvasHost";

/** A match as `App::findNext` reports it. */
interface FoundMatch {
  index: number;
  total: number;
  slide: number;
  notes: boolean;
  /** UTF-16 range in the notes, for notes matches. */
  start?: number;
  end?: number;
}

export interface FindBarHandle {
  open: (replace: boolean) => void;
  /** ⌘G / ⇧⌘G: the next or previous match, opening the bar if needed. */
  step: (backwards: boolean) => void;
}

interface Props {
  engine: Engine | null;
  /** After the engine changed selection, slide or text. `all`: many slides may have changed. */
  onChange: (all: boolean) => void;
  /** Selects a notes match in the notes pane (opening it). */
  showNotesMatch: (start: number, end: number) => void;
}

const NOTES_SELECTOR = ".notes-input";

/**
 * Find and replace over the whole deck, floating over the canvas. Matches in
 * slides are selected in a text edit; matches in speaker notes in the notes pane.
 */
export const FindBar = forwardRef<FindBarHandle, Props>(function FindBar({ engine, onChange, showNotesMatch }, ref) {
  const [open, setOpen] = useState(false);
  const [replacing, setReplacing] = useState(false);
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [matchCase, setMatchCase] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [result, setResult] = useState<string>("");
  const input = useRef<HTMLInputElement>(null);
  /** The notes match last shown, to continue from it. */
  const notesMatch = useRef<FoundMatch | null>(null);

  // The notes range to continue from: the last notes match, if it is still what is selected there.
  const notesRange = (): [number, number] => {
    const m = notesMatch.current;
    const ta = document.querySelector<HTMLTextAreaElement>(NOTES_SELECTOR);
    const slide = (engine?.status() as { slideIndex: number } | undefined)?.slideIndex;
    if (!m || !ta || slide !== m.slide || ta.selectionStart !== m.start || ta.selectionEnd !== m.end) return [-1, -1];
    return [m.start ?? -1, m.end ?? -1];
  };

  const show = (m: FoundMatch | null, all = false) => {
    notesMatch.current = m?.notes ? m : null;
    setResult(m ? `${m.index + 1} / ${m.total}` : query ? "找不到" : "");
    onChange(all);
    if (m?.notes && m.start !== undefined && m.end !== undefined) showNotesMatch(m.start, m.end);
  };

  const step = (backwards: boolean) => {
    if (!engine || !query) return;
    const [s, e] = notesRange();
    show(engine.findNext(query, matchCase, wholeWord, backwards, s, e) as FoundMatch | null);
  };

  const replaceOne = () => {
    if (!engine || !query) return;
    const [s, e] = notesRange();
    show(engine.replaceNext(query, replacement, matchCase, wholeWord, s, e) as FoundMatch | null);
  };

  const replaceAll = () => {
    if (!engine || !query) return;
    const n = engine.replaceAll(query, replacement, matchCase, wholeWord);
    notesMatch.current = null;
    setResult(n > 0 ? `已取代 ${n} 處` : "找不到");
    onChange(true);
  };

  useImperativeHandle(ref, () => ({
    open: (replace) => {
      setOpen(true);
      if (replace) setReplacing(true);
      // Start from the selected text, as macOS find panels do.
      const picked = engine?.selectedText();
      if (picked && !picked.includes("\n")) setQuery(picked);
      requestAnimationFrame(() => {
        input.current?.focus();
        input.current?.select();
      });
    },
    step: (backwards) => {
      if (!open) setOpen(true);
      step(backwards);
    },
  }));

  // Live count while typing.
  useEffect(() => {
    if (!open || !engine) return;
    notesMatch.current = null;
    const n = query ? engine.findCount(query, matchCase, wholeWord) : 0;
    setResult(query ? (n ? `${n} 個結果` : "找不到") : "");
  }, [open, engine, query, matchCase, wholeWord]);

  if (!open) return null;

  const close = () => {
    setOpen(false);
    focusCanvas();
  };
  const onKey = (e: React.KeyboardEvent, enter: () => void) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Enter" && !e.nativeEvent.isComposing) {
      e.preventDefault();
      if (e.shiftKey) step(true);
      else enter();
    }
  };

  return (
    <div className="find-bar" role="search" onMouseDown={(e) => e.target === e.currentTarget && e.preventDefault()}>
      <div className="find-row">
        <button className={replacing ? "find-toggle-mode open" : "find-toggle-mode"} onClick={() => setReplacing((r) => !r)} title={replacing ? "隱藏取代" : "顯示取代"} aria-label="切換取代">
          ›
        </button>
        <input ref={input} className="find-input" placeholder="尋找" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => onKey(e, () => step(false))} aria-label="尋找" />
        <span className="find-result" aria-live="polite">
          {result}
        </span>
        <button className="find-btn" disabled={!query} onClick={() => step(true)} title="上一個（⇧⌘G）" aria-label="上一個">
          ▲
        </button>
        <button className="find-btn" disabled={!query} onClick={() => step(false)} title="下一個（⌘G）" aria-label="下一個">
          ▼
        </button>
        <button className={matchCase ? "find-btn active" : "find-btn"} aria-pressed={matchCase} onClick={() => setMatchCase((v) => !v)} title="大小寫須相符">
          Aa
        </button>
        <button className={wholeWord ? "find-btn active" : "find-btn"} aria-pressed={wholeWord} onClick={() => setWholeWord((v) => !v)} title="全字拼寫須相符">
          全字
        </button>
        <button className="find-btn" onClick={close} title="關閉（Esc）" aria-label="關閉">
          ✕
        </button>
      </div>
      {replacing && (
        <div className="find-row">
          <span className="find-toggle-mode spacer" />
          <input className="find-input replace-input" placeholder="取代為" value={replacement} onChange={(e) => setReplacement(e.target.value)} onKeyDown={(e) => onKey(e, replaceOne)} aria-label="取代為" />
          <button className="find-btn text replace-one" disabled={!query} onClick={replaceOne} title="取代選取的符合項目，並找下一個">
            取代
          </button>
          <button className="find-btn text replace-all" disabled={!query} onClick={replaceAll} title="取代所有投影片與備忘稿中的符合項目">
            全部取代
          </button>
        </div>
      )}
    </div>
  );
});
