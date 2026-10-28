import { useEffect, useRef, useState, type ReactNode } from "react";

/** A button that opens a floating panel; clicking outside or pressing Esc closes it. */
export function Popover(props: { button: (open: boolean, toggle: () => void) => ReactNode; children: (close: () => void) => ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);
  return (
    <div className="popover-anchor" ref={ref}>
      {props.button(open, () => setOpen((o) => !o))}
      {open && <div className="popover">{props.children(() => setOpen(false))}</div>}
    </div>
  );
}
