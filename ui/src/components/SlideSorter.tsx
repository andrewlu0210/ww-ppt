import { t } from "../i18n";
import { useEffect, useRef, useState } from "react";
import { Thumb } from "./Thumb";
import type { Command } from "../lib/commands";

interface Props {
  count: number;
  current: number;
  thumbs: Map<number, ImageData>;
  transitions?: boolean[];
  /** Hidden slides: number struck through, thumbnail dimmed. */
  hidden?: boolean[];
  /** Thumbnail width in CSS pixels. */
  size: number;
  onSelect: (index: number) => void;
  /** Double click or Enter: edit this slide in the normal view. */
  onOpen: (index: number) => void;
  run: (c: Command) => void;
  /** Accessible names ("投影片 2：標題"). */
  labels?: string[];
  /** Right click on a slide, or on the empty part of the grid (`index` null). */
  onContextMenu: (index: number | null, x: number, y: number) => void;
}

/** 投影片瀏覽: every slide in a grid; drag to reorder, Delete to remove. */
export function SlideSorter({ count, current, thumbs, transitions, hidden, size, onSelect, onOpen, run, labels, onContextMenu }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);

  useEffect(() => {
    ref.current?.focus({ preventScroll: true });
  }, []);
  useEffect(() => {
    ref.current?.querySelector(".sorter-item.current")?.scrollIntoView({ block: "nearest" });
  }, [current]);

  const finishDrop = (to: number) => {
    if (dragFrom !== null) {
      const target = to > dragFrom ? to - 1 : to;
      if (target !== dragFrom) run({ kind: "moveSlide", from: dragFrom, to: target });
    }
    setDragFrom(null);
    setDropAt(null);
  };

  // Slides per row, for ↑/↓.
  const perRow = () => {
    const items = ref.current?.querySelectorAll<HTMLElement>(".sorter-item");
    if (!items || items.length < 2) return 1;
    const y = items[0].offsetTop;
    let n = 0;
    while (n < items.length && items[n].offsetTop === y) n++;
    return Math.max(1, n);
  };

  const onKey = (e: React.KeyboardEvent) => {
    const go = (i: number) => onSelect(Math.max(0, Math.min(count - 1, i)));
    switch (e.key) {
      case "ArrowLeft":
        go(current - 1);
        break;
      case "ArrowRight":
        go(current + 1);
        break;
      case "ArrowUp":
        go(current - perRow());
        break;
      case "ArrowDown":
        go(current + perRow());
        break;
      case "Home":
        go(0);
        break;
      case "End":
        go(count - 1);
        break;
      case "Enter":
        onOpen(current);
        break;
      case "Delete":
      case "Backspace":
        if (count > 1) run({ kind: "deleteSlide", index: current });
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  return (
    <div
      ref={ref}
      className="slide-sorter"
      tabIndex={0}
      role="listbox"
      aria-label={t("ribbon.sorter")}
      onKeyDown={onKey}
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
      onDragOver={(e) => e.preventDefault()}
      onDrop={() => finishDrop(dropAt ?? count)}
      style={{ "--sorter-size": `${size}px` } as React.CSSProperties}
    >
      {Array.from({ length: count }, (_, i) => (
        <div
          key={i}
          role="option"
          aria-selected={i === current}
          aria-label={(labels?.[i] ?? t("slide.label", { n: i + 1 })) + (hidden?.[i] ? `（${t("slide.hidden")}）` : "")}
          className={["sorter-item", i === current && "current", hidden?.[i] && "hidden-slide", dragFrom !== null && dropAt === i && "drop-before", dragFrom !== null && dropAt === i + 1 && i === count - 1 && "drop-after"].filter(Boolean).join(" ")}
          draggable
          onDragStart={(e) => {
            setDragFrom(i);
            onSelect(i);
            e.dataTransfer.effectAllowed = "move";
          }}
          onDragOver={(e) => {
            e.preventDefault();
            const r = e.currentTarget.getBoundingClientRect();
            setDropAt(e.clientX < r.left + r.width / 2 ? i : i + 1);
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
          onDoubleClick={() => onOpen(i)}
        >
          <span className="sorter-card">
            <Thumb image={thumbs.get(i)} className="sorter-thumb" />
          </span>
          <span className="sorter-no">
            {i + 1}
            {transitions?.[i] && <span className="slide-mark">★</span>}
          </span>
        </div>
      ))}
    </div>
  );
}
