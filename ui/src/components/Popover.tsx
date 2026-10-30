import { useEffect, useRef, useState, type ReactNode } from "react";
import { focusables, moveFocus, ownsArrows } from "../lib/keyboard";

/**
 * A button that opens a floating panel; clicking outside or pressing Esc closes it.
 * Opened from the keyboard, the panel takes focus: Tab cycles inside it, ↑/↓ move
 * between its items, and closing returns focus to the button.
 */
export function Popover(props: { button: (open: boolean, toggle: () => void) => ReactNode; children: (close: () => void) => ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  // The focused trigger when opened from the keyboard (mouse clicks keep focus on the canvas).
  const opener = useRef<HTMLElement | null>(null);
  const close = () => {
    setOpen(false);
    const back = opener.current;
    opener.current = null;
    back?.focus();
  };
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) {
        opener.current = null;
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
  useEffect(() => {
    if (open && opener.current) focusables(panel.current)[0]?.focus();
  }, [open]);
  const toggle = () => {
    const active = document.activeElement as HTMLElement | null;
    opener.current = !open && active && ref.current?.contains(active) ? active : null;
    if (open) close();
    else setOpen(true);
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    const items = focusables(panel.current);
    if (e.key === "Tab" || ((e.key === "ArrowDown" || e.key === "ArrowUp") && !ownsArrows(e.target as Element))) {
      moveFocus(items, e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey) ? -1 : 1);
      e.preventDefault();
    }
  };
  return (
    <div className="popover-anchor" ref={ref}>
      {props.button(open, toggle)}
      {open && (
        <div className="popover" ref={panel} onKeyDown={onKeyDown}>
          {props.children(close)}
        </div>
      )}
    </div>
  );
}
