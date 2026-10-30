import { useEffect, useRef, useState } from "react";
import type { App } from "../wasm/ppt_wasm";
import { focusCanvas } from "./CanvasHost";

/** A row of `App::paneItems`. */
interface PaneItem {
  id: number;
  name: string;
  kind: string;
  depth: number;
  hidden: boolean;
  selected: boolean;
  altText: string;
  decorative: boolean;
}

const KIND_LABELS: Record<string, string> = {
  group: "群組",
  picture: "圖片",
  table: "表格",
  chart: "圖表",
  diagram: "SmartArt",
  text: "文字方塊",
  line: "線條",
  shape: "圖案",
  other: "物件",
};

const KIND_GLYPHS: Record<string, string> = { group: "▣", picture: "▨", table: "▦", chart: "▥", diagram: "◈", text: "T", line: "╱", shape: "◆", other: "□" };

const Eye = ({ open }: { open: boolean }) => (
  <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
    <path d="M2 10s3-5.5 8-5.5S18 10 18 10s-3 5.5-8 5.5S2 10 2 10z" />
    {open ? <circle cx="10" cy="10" r="2.5" /> : <path d="M4 16L16 4" />}
  </svg>
);

interface Props {
  engine: App | null;
  /** Re-read the rows when this changes (document revision, selection). */
  version: string;
  onChange: () => void;
  onClose: () => void;
}

/**
 * 選取窗格: the slide's objects, top-most first. Click to select (⌘ to add),
 * the eye hides or shows, double-click or F2 renames, ▲▼ change the order.
 * Keys: ↑/↓ select, Space hides/shows, ⌘↑/⌘↓ move, Esc goes back to the slide.
 */
export function SelectionPane({ engine, version, onChange, onClose }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const [renaming, setRenaming] = useState<number | null>(null);
  const [items, setItems] = useState<PaneItem[]>([]);
  useEffect(() => {
    setItems(engine ? (engine.paneItems() as PaneItem[]) : []);
  }, [engine, version]);
  const current = items.find((i) => i.selected) ?? null;
  const act = (f: (e: App) => unknown) => {
    if (!engine) return;
    f(engine);
    setItems(engine.paneItems() as PaneItem[]);
    onChange();
  };
  const select = (i: PaneItem, additive = false) => act((e) => e.selectInPane(i.id, additive));

  useEffect(() => {
    listRef.current?.querySelector(".pane-row.selected")?.scrollIntoView({ block: "nearest" });
  }, [items]);

  const onKey = (e: React.KeyboardEvent) => {
    if (renaming !== null) return;
    const visible = items.filter((i) => !i.hidden);
    const at = current ? visible.findIndex((i) => i.id === current.id) : -1;
    switch (e.key) {
      case "ArrowUp":
      case "ArrowDown": {
        const up = e.key === "ArrowUp";
        if (e.metaKey && current) act((en) => en.moveInPane(current.id, up));
        else {
          const next = visible[at < 0 ? 0 : Math.max(0, Math.min(visible.length - 1, at + (up ? -1 : 1)))];
          if (next) select(next);
        }
        break;
      }
      case " ":
        if (current) act((en) => en.setShapeHidden(current.id, !current.hidden));
        break;
      case "F2":
      case "Enter":
        if (current && current.kind !== "group") setRenaming(current.id);
        break;
      case "Escape":
        focusCanvas();
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <aside className="selection-pane" aria-label="選取窗格">
      <div className="pane-head">
        <span className="pane-title">選取窗格</span>
        <button className="pane-close" onClick={onClose} aria-label="關閉選取窗格" title="關閉">
          ×
        </button>
      </div>
      <div className="pane-tools">
        <button onClick={() => act((e) => e.setAllHidden(false))}>全部顯示</button>
        <button onClick={() => act((e) => e.setAllHidden(true))}>全部隱藏</button>
        <span className="spacer" />
        <button disabled={!current} onClick={() => current && act((e) => e.moveInPane(current.id, true))} aria-label="上移一層" title="上移一層">
          ▲
        </button>
        <button disabled={!current} onClick={() => current && act((e) => e.moveInPane(current.id, false))} aria-label="下移一層" title="下移一層">
          ▼
        </button>
      </div>
      <div className="pane-list" ref={listRef} role="listbox" aria-label="投影片上的物件" tabIndex={0} onKeyDown={onKey}>
        {items.length === 0 && <div className="pane-empty">這張投影片沒有物件</div>}
        {items.map((i) => (
          <div
            key={i.id}
            role="option"
            aria-selected={i.selected}
            className={["pane-row", i.selected && "selected", i.hidden && "hidden"].filter(Boolean).join(" ")}
            style={{ paddingLeft: 8 + i.depth * 16 }}
            title={i.altText ? `替代文字：${i.altText}` : i.decorative ? "裝飾用" : KIND_LABELS[i.kind]}
            onMouseDown={(e) => e.preventDefault()}
            onClick={(e) => {
              listRef.current?.focus();
              select(i, e.metaKey || e.shiftKey);
            }}
            onDoubleClick={() => i.kind !== "group" && setRenaming(i.id)}
          >
            <span className="pane-kind" aria-label={KIND_LABELS[i.kind]}>
              {KIND_GLYPHS[i.kind]}
            </span>
            {renaming === i.id ? (
              <input
                className="pane-rename"
                autoFocus
                defaultValue={i.name}
                onClick={(e) => e.stopPropagation()}
                onMouseDown={(e) => e.stopPropagation()}
                onKeyDown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") e.currentTarget.blur();
                  if (e.key === "Escape") {
                    setRenaming(null);
                    listRef.current?.focus();
                  }
                }}
                onBlur={(e) => {
                  const name = e.currentTarget.value;
                  setRenaming(null);
                  act((en) => en.renameShape(i.id, name));
                  listRef.current?.focus();
                }}
              />
            ) : (
              <span className="pane-name">{i.name || KIND_LABELS[i.kind]}</span>
            )}
            <button
              className="pane-eye"
              aria-label={i.hidden ? `顯示 ${i.name}` : `隱藏 ${i.name}`}
              aria-pressed={!i.hidden}
              title={i.hidden ? "顯示" : "隱藏"}
              onMouseDown={(e) => e.preventDefault()}
              onClick={(e) => {
                e.stopPropagation();
                act((en) => en.setShapeHidden(i.id, !i.hidden));
              }}
            >
              <Eye open={!i.hidden} />
            </button>
          </div>
        ))}
      </div>
    </aside>
  );
}
