import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { App } from "../wasm/ppt_wasm";
import { focusCanvas } from "./CanvasHost";

/** `App::diagramInfo`. */
export interface DiagramInfo {
  rows: { depth: number; text: string }[];
  /** Nodes can be added, removed and reordered. */
  restructure: boolean;
  /** Nodes can be moved between levels. */
  nest: boolean;
}

export type DiagramCommand = "addAfter" | "addBefore" | "addChild" | "delete" | "promote" | "demote" | "moveUp" | "moveDown";

interface Props {
  engine: App | null;
  /** Re-read the rows when this changes (document revision, selection). */
  version: string;
  /** The row being edited, shared with the ribbon's SmartArt commands. */
  row: number;
  onRow: (row: number) => void;
  /** Bumped to move keyboard focus to `row` (a double-click on the SmartArt). */
  focusRequest: number;
  onChange: () => void;
  onClose: () => void;
}

const BULLETS = ["●", "○", "■", "□", "◆"];

/**
 * 文字窗格: the selected SmartArt's nodes as an outline, one field per node.
 * Enter adds a node, Tab / Shift+Tab demote and promote, Backspace in an empty
 * field removes it, ⌥⇧↑ / ⌥⇧↓ move it, Shift+Enter starts a new paragraph.
 */
export function DiagramPane({ engine, version, row, onRow, focusRequest, onChange, onClose }: Props) {
  const [info, setInfo] = useState<DiagramInfo | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  // Focus this row once the rows are re-read.
  const pendingFocus = useRef<{ row: number; caret: "end" | "start" } | null>(null);

  useEffect(() => {
    setInfo(engine ? (engine.diagramInfo() as DiagramInfo | null) : null);
  }, [engine, version]);

  // Before the focusing effect below, so the request applies in this commit.
  useLayoutEffect(() => {
    if (focusRequest > 0) pendingFocus.current = { row, caret: "end" };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusRequest]);

  useLayoutEffect(() => {
    const want = pendingFocus.current;
    const fields = listRef.current?.querySelectorAll<HTMLTextAreaElement>("textarea");
    if (!want || !fields || fields.length === 0) return;
    const field = fields[Math.min(want.row, fields.length - 1)];
    pendingFocus.current = null;
    field.focus();
    const at = want.caret === "end" ? field.value.length : 0;
    field.setSelectionRange(at, at);
  });

  const command = (c: DiagramCommand, at = row) => {
    if (!engine) return;
    const next = engine.diagramCommand(c, at);
    if (next < 0) return;
    onRow(next);
    pendingFocus.current = { row: next, caret: c === "delete" ? "end" : c === "addAfter" || c === "addBefore" || c === "addChild" ? "start" : "end" };
    setInfo(engine.diagramInfo() as DiagramInfo | null);
    onChange();
  };

  const setText = (i: number, text: string) => {
    if (!engine) return;
    setInfo((cur) => (cur ? { ...cur, rows: cur.rows.map((r, k) => (k === i ? { ...r, text } : r)) } : cur));
    engine.diagramSetText(i, text);
    onChange();
  };

  const focusRow = (i: number, caret: "end" | "start") => {
    if (!info || i < 0 || i >= info.rows.length) return;
    onRow(i);
    pendingFocus.current = { row: i, caret };
    setInfo({ ...info });
  };

  const onKey = (e: React.KeyboardEvent<HTMLTextAreaElement>, i: number) => {
    if (e.nativeEvent.isComposing || !info) return;
    const field = e.currentTarget;
    if (e.key === "Enter" && !e.shiftKey) {
      command("addAfter", i);
    } else if (e.key === "Tab") {
      command(e.shiftKey ? "promote" : "demote", i);
    } else if (e.key === "Backspace" && field.value === "" && info.rows.length > 1) {
      command("delete", i);
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && e.altKey && e.shiftKey) {
      command(e.key === "ArrowUp" ? "moveUp" : "moveDown", i);
    } else if (e.key === "ArrowUp" && !field.value.slice(0, field.selectionStart).includes("\n")) {
      if (i === 0) return;
      focusRow(i - 1, "end");
    } else if (e.key === "ArrowDown" && !field.value.slice(field.selectionEnd).includes("\n")) {
      if (i === info.rows.length - 1) return;
      focusRow(i + 1, "end");
    } else if (e.key === "Escape") {
      focusCanvas();
    } else {
      // Typing stays in the field (no canvas shortcuts).
      e.stopPropagation();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  };

  const rows = info?.rows ?? [];
  const canMove = (up: boolean) => {
    if (!info?.restructure || !rows[row]) return false;
    const depth = rows[row].depth;
    if (depth > 0 && !info.nest) return false;
    const step = up ? -1 : 1;
    for (let k = row + step; k >= 0 && k < rows.length; k += step) {
      if (rows[k].depth < depth) return false;
      if (rows[k].depth === depth) return true;
    }
    return false;
  };
  const current = rows[row];
  const topCount = rows.filter((r) => r.depth === 0).length;

  return (
    <aside className="diagram-pane" aria-label="文字窗格">
      <div className="pane-head">
        <span className="pane-title">文字窗格</span>
        <button className="pane-close" onClick={onClose} aria-label="關閉文字窗格" title="關閉">
          ×
        </button>
      </div>
      {!info ? (
        <div className="pane-empty">選取 SmartArt 圖形以編輯它的文字</div>
      ) : (
        <>
          <div className="pane-tools">
            <button disabled={!info.restructure || !current || (current.depth > 0 && !info.nest)} onClick={() => command("addAfter")} title="在後面新增圖案 (Enter)">
              ＋ 新增
            </button>
            <button disabled={!info.nest || !current || current.depth === 0} onClick={() => command("promote")} aria-label="升階" title="升階 (Shift+Tab)">
              ←
            </button>
            <button disabled={!info.nest || !current || row === 0 || rows[row - 1].depth < current.depth} onClick={() => command("demote")} aria-label="降階" title="降階 (Tab)">
              →
            </button>
            <span className="spacer" />
            <button disabled={!canMove(true)} onClick={() => command("moveUp")} aria-label="上移" title="上移 (⌥⇧↑)">
              ▲
            </button>
            <button disabled={!canMove(false)} onClick={() => command("moveDown")} aria-label="下移" title="下移 (⌥⇧↓)">
              ▼
            </button>
            <button
              disabled={!info.restructure || !current || (current.depth > 0 ? !info.nest : topCount <= 1)}
              onClick={() => command("delete")}
              aria-label="刪除圖案"
              title="刪除圖案"
            >
              ✕
            </button>
          </div>
          <div className="diagram-rows" ref={listRef} role="list" aria-label="SmartArt 文字">
            {rows.map((r, i) => (
              <div key={i} role="listitem" className={["diagram-row", i === row && "current"].filter(Boolean).join(" ")} style={{ paddingLeft: 8 + r.depth * 18 }}>
                <span className="diagram-bullet" aria-hidden="true">
                  {BULLETS[Math.min(r.depth, BULLETS.length - 1)]}
                </span>
                <textarea
                  className="diagram-input"
                  rows={Math.max(1, r.text.split("\n").length)}
                  value={r.text}
                  placeholder="[文字]"
                  aria-label={`第 ${r.depth + 1} 層：${r.text || "空白"}`}
                  spellCheck={false}
                  onFocus={() => onRow(i)}
                  onChange={(e) => setText(i, e.currentTarget.value)}
                  onKeyDown={(e) => onKey(e, i)}
                />
              </div>
            ))}
          </div>
          {!info.restructure && <div className="pane-note">這個版面只能修改文字。</div>}
          {info.restructure && !info.nest && <div className="pane-note">這個版面沒有下層項目。</div>}
        </>
      )}
    </aside>
  );
}
