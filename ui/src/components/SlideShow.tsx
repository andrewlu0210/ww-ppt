import { useEffect, useRef, useState } from "react";
import { showKeyCommand, type ShowCommand, type ShowState } from "../lib/show";

interface Props {
  state: ShowState | null;
  command: (c: ShowCommand) => void;
  /** ⌥↩: switch to the presenter view (single display only). */
  onToggleView?: () => void;
}

/** The pointer hides after this long without moving. */
const CURSOR_IDLE_MS = 2500;
/** Wheel distance that counts as one step. */
const WHEEL_STEP = 40;

/**
 * Input layer over a canvas showing the slides full screen. The engine draws
 * the slides; this maps keys, clicks and the wheel to show commands.
 */
export function SlideShow({ state, command, onToggleView }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [cursor, setCursor] = useState(false);
  const digits = useRef("");
  const wheel = useRef(0);
  const idle = useRef(0);

  useEffect(() => {
    ref.current?.focus({ preventScroll: true });
    return () => window.clearTimeout(idle.current);
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    const c = showKeyCommand(e, digits);
    if (c === "toggleView") onToggleView?.();
    else if (c) command(c);
  };

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    ref.current?.focus({ preventScroll: true });
    if (e.button === 0 && state?.onClick !== false) command({ name: "next" });
    else if (e.button === 2) command({ name: "prev" });
  };

  const onWheel = (e: React.WheelEvent) => {
    wheel.current += e.deltaY;
    if (Math.abs(wheel.current) < WHEEL_STEP) return;
    command({ name: wheel.current > 0 ? "next" : "prev" });
    wheel.current = 0;
  };

  const onPointerMove = () => {
    setCursor(true);
    window.clearTimeout(idle.current);
    idle.current = window.setTimeout(() => setCursor(false), CURSOR_IDLE_MS);
  };

  return (
    <div
      ref={ref}
      className="slide-show"
      tabIndex={0}
      style={{ cursor: cursor ? "default" : "none" }}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onWheel={onWheel}
      onContextMenu={(e) => e.preventDefault()}
      onBlur={() => window.setTimeout(() => ref.current?.focus({ preventScroll: true }), 0)}
      aria-label="投影片放映"
    >
      {state?.slide === -1 && !state.blank && <div className="show-end">投影片放映結束，按一下以結束。</div>}
    </div>
  );
}
