import { t } from "../i18n";
import { memo, useEffect, useState } from "react";
import { Thumb } from "./Thumb";
import type { Command } from "../lib/commands";
import { focusCanvas } from "./CanvasHost";

interface Props {
  count: number;
  current: number;
  thumbs: Map<number, ImageData>;
  /** Slides with a transition get a mark under their number. */
  transitions?: boolean[];
  /** Hidden slides: number struck through, thumbnail dimmed. */
  hidden?: boolean[];
  onSelect: (index: number) => void;
  run: (c: Command) => void;
  /** Accessible names ("投影片 2：標題"). */
  labels?: string[];
  /** Right click (or Shift+F10) on a slide, or on the empty part of the pane (`index` null). */
  onContextMenu: (index: number | null, x: number, y: number, byKey?: boolean) => void;
}

/** Slide thumbnails: click to open, drag to reorder, right-click for slide commands. */
function SlidePaneImpl({ count, current, thumbs, transitions, hidden, onSelect, run, labels, onContextMenu }: Props) {
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);

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
        if (r) onContextMenu(current, r.left + 24, r.top + 24, true);
        break;
      }
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <nav
      className="slide-pane"
      aria-label={t("group.slides")}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(null, e.clientX, e.clientY);
      }}
      onCopy={(e) => {
        e.preventDefault();
        run({ kind: "slideClip", op: "copy", index: current });
      }}
      onCut={(e) => {
        e.preventDefault();
        if (count > 1) run({ kind: "slideClip", op: "cut", index: current });
      }}
      onPaste={(e) => {
        e.preventDefault();
        run({ kind: "slideClip", op: "paste", after: current });
      }}
      onDragOver={(e) => e.preventDefault()} onDrop={() => finishDrop(dropAt ?? count)} onKeyDown={onKey}>
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className="slide-slot">
          {dropAt === i && dragFrom !== null && <div className="drop-line" />}
          <button
            className={["slide-item", i === current && "current", hidden?.[i] && "hidden-slide"].filter(Boolean).join(" ")}
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
              e.stopPropagation();
              onSelect(i);
              onContextMenu(i, e.clientX, e.clientY);
            }}
            onMouseDown={(e) => e.button === 0 && e.preventDefault()}
            aria-current={i === current}
            aria-label={(labels?.[i] ?? t("slide.label", { n: i + 1 })) + (hidden?.[i] ? `（${t("slide.hidden")}）` : "")}
          >
            <span className="slide-no">
              {i + 1}
              {transitions?.[i] && (
                <span className="slide-mark" title={t("slide.hasTransition")}>
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
    </nav>
  );
}

export const SlidePane = memo(SlidePaneImpl);
