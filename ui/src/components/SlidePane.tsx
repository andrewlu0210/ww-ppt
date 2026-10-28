import { useEffect, useState } from "react";
import { Thumb } from "./Thumb";
import type { Command } from "../lib/commands";

interface Props {
  count: number;
  current: number;
  thumbs: Map<number, ImageData>;
  onSelect: (index: number) => void;
  run: (c: Command) => void;
}

interface Menu {
  x: number;
  y: number;
  index: number;
}

/** Slide thumbnails: click to open, drag to reorder, right-click for slide commands. */
export function SlidePane({ count, current, thumbs, onSelect, run }: Props) {
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<number | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [menu]);

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

  return (
    <nav className="slide-pane" aria-label="投影片" onDragOver={(e) => e.preventDefault()} onDrop={() => finishDrop(dropAt ?? count)}>
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
            aria-label={`投影片 ${i + 1}`}
          >
            <span className="slide-no">{i + 1}</span>
            <span className="slide-card">
              <Thumb image={thumbs.get(i)} className="slide-thumb" />
            </span>
          </button>
        </div>
      ))}
      {dropAt === count && dragFrom !== null && <div className="drop-line" />}
      {menu && (
        <div className="context-menu" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
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
              onClick={() => {
                run(cmd);
                setMenu(null);
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
