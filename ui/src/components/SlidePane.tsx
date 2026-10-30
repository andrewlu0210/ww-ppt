import { useEffect, useState } from "react";
import { Thumb } from "./Thumb";
import type { Command } from "../lib/commands";
import { focusCanvas } from "./CanvasHost";
import { focusables, moveFocus } from "../lib/keyboard";

interface Props {
  count: number;
  current: number;
  thumbs: Map<number, ImageData>;
  /** Slides with a transition get a mark under their number. */
  transitions?: boolean[];
  onSelect: (index: number) => void;
  run: (c: Command) => void;
  /** Accessible names ("投影片 2：標題"). */
  labels?: string[];
}

interface Menu {
  x: number;
  y: number;
  index: number;
  /** Opened from the keyboard: the menu takes focus. */
  byKey?: boolean;
}

/** Slide thumbnails: click to open, drag to reorder, right-click for slide commands. */
export function SlidePane({ count, current, thumbs, transitions, onSelect, run, labels }: Props) {
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [menu]);

  useEffect(() => {
    if (menu?.byKey) document.querySelector<HTMLElement>(".slide-pane .context-menu button:not(:disabled)")?.focus();
  }, [menu]);
  const closeMenu = () => {
    const byKey = menu?.byKey;
    setMenu(null);
    if (byKey) requestAnimationFrame(() => document.querySelector<HTMLElement>(".slide-item.current")?.focus());
  };

  // Keep the current slide visible.
  useEffect(() => {
    document.querySelector(".slide-item.current")?.scrollIntoView({ block: "nearest" });
  }, [current]);

  const finishDrop = (to: number) => {
    if (dragFrom !== null) {
      // `to` is an insertion gap; convert to the final index.
      const target = to > dragFrom ? to - 1 : to;
      if (target !== dragFrom) run({ kind: "moveSlide", from: dragFrom, to: target });
    }
    setDragFrom(null);
    setDropAt(null);
  };

  // Keyboard: ↑/↓ Home/End pick a slide, ⌘↑/⌘↓ move it, Delete removes it,
  // Enter goes to the slide, Shift+F10 opens the slide menu.
  const onKey = (e: React.KeyboardEvent) => {
    if (menu) return;
    const go = (i: number) => {
      const to = Math.max(0, Math.min(count - 1, i));
      onSelect(to);
      requestAnimationFrame(() => document.querySelectorAll<HTMLElement>(".slide-item")[to]?.focus());
    };
    switch (e.key) {
      case "ArrowUp":
      case "ArrowDown": {
        const to = current + (e.key === "ArrowUp" ? -1 : 1);
        if (e.metaKey) {
          if (to >= 0 && to < count) {
            run({ kind: "moveSlide", from: current, to });
            requestAnimationFrame(() => document.querySelectorAll<HTMLElement>(".slide-item")[to]?.focus());
          }
        } else go(to);
        break;
      }
      case "Home":
        go(0);
        break;
      case "End":
        go(count - 1);
        break;
      case "Delete":
      case "Backspace":
        if (count > 1) {
          run({ kind: "deleteSlide", index: current });
          go(Math.min(current, count - 2));
        }
        break;
      case "Enter":
        focusCanvas();
        break;
      case "F10":
      case "ContextMenu": {
        if (e.key === "F10" && !e.shiftKey) return;
        const r = document.querySelectorAll<HTMLElement>(".slide-item")[current]?.getBoundingClientRect();
        if (r) setMenu({ x: r.left + 24, y: r.top + 24, index: current, byKey: true });
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <nav className="slide-pane" aria-label="投影片" onDragOver={(e) => e.preventDefault()} onDrop={() => finishDrop(dropAt ?? count)} onKeyDown={onKey}>
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="slide-slot">
          {dropAt === i && dragFrom !== null && <div className="drop-line" />}
          <button
            className={i === current ? "slide-item current" : "slide-item"}
            draggable
            onDragStart={(e) => {
              setDragFrom(i);
              e.dataTransfer.effectAllowed = "move";
            }}
            onDragOver={(e) => {
              e.preventDefault();
              const r = e.currentTarget.getBoundingClientRect();
              setDropAt(e.clientY < r.top + r.height / 2 ? i : i + 1);
            }}
            onDragEnd={() => {
              setDragFrom(null);
              setDropAt(null);
            }}
            onClick={() => onSelect(i)}
            onContextMenu={(e) => {
              e.preventDefault();
              onSelect(i);
              setMenu({ x: e.clientX, y: e.clientY, index: i });
            }}
            onMouseDown={(e) => e.button === 0 && e.preventDefault()}
            aria-current={i === current}
            aria-label={labels?.[i] ?? `投影片 ${i + 1}`}
          >
            <span className="slide-no">
              {i + 1}
              {transitions?.[i] && (
                <span className="slide-mark" title="有轉場">
                  ★
                </span>
              )}
            </span>
            <span className="slide-card">
              <Thumb image={thumbs.get(i)} className="slide-thumb" />
            </span>
          </button>
        </div>
      ))}
      {dropAt === count && dragFrom !== null && <div className="drop-line" />}
      {menu && (
        <div
          className="context-menu"
          role="menu"
          style={{ left: menu.x, top: menu.y }}
          onMouseDown={(e) => e.stopPropagation()}
          onKeyDown={(e) => {
            if (e.key === "Escape") closeMenu();
            else if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Tab") moveFocus(focusables(e.currentTarget), e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey) ? -1 : 1);
            else return;
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          {(
            [
              ["新增投影片", { kind: "newSlide" }],
              ["複製投影片", { kind: "duplicateSlide", index: menu.index }],
              ["刪除投影片", { kind: "deleteSlide", index: menu.index }],
            ] as [string, Command][]
          ).map(([label, cmd]) => (
            <button
              key={label}
              disabled={label === "刪除投影片" && count <= 1}
              role="menuitem"
              onClick={() => {
                run(cmd);
                closeMenu();
              }}
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </nav>
  );
}
