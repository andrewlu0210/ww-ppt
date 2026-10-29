import { useEffect, useRef, useState } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";

interface ShowState {
  /** -1 on the end screen. */
  slide: number;
  count: number;
  blank: "" | "black" | "white";
}

interface Props {
  engine: Engine;
  onEnd: () => void;
}

/** The pointer hides after this long without moving. */
const CURSOR_IDLE_MS = 2500;
/** Wheel distance that counts as one step. */
const WHEEL_STEP = 40;

/**
 * Input layer over the canvas while a slide show runs. The engine draws the
 * slides; this maps keys, clicks and the wheel to show commands.
 */
export function SlideShow({ engine, onEnd }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<ShowState | null>(() => engine.showState() as ShowState | null);
  const [cursor, setCursor] = useState(false);
  const digits = useRef("");
  const wheel = useRef(0);
  const idle = useRef(0);

  useEffect(() => {
    ref.current?.focus({ preventScroll: true });
    return () => window.clearTimeout(idle.current);
  }, []);

  const command = (name: string, index = 0) => {
    digits.current = "";
    if (!engine.showCommand(name, index)) {
      onEnd();
      return;
    }
    setState(engine.showState() as ShowState | null);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey) return;
    e.preventDefault();
    const k = e.key;
    if (/^[0-9]$/.test(k)) {
      digits.current += k;
      return;
    }
    if (k === "Enter" && digits.current) {
      const n = parseInt(digits.current, 10);
      command("goto", Math.max(0, n - 1));
      return;
    }
    switch (k) {
      case "ArrowRight":
      case "ArrowDown":
      case "PageDown":
      case " ":
      case "Enter":
      case "n":
      case "N":
        command("next");
        break;
      case "ArrowLeft":
      case "ArrowUp":
      case "PageUp":
      case "Backspace":
      case "p":
      case "P":
        command("prev");
        break;
      case "Home":
        command("first");
        break;
      case "End":
        command("last");
        break;
      case "b":
      case "B":
      case ".":
        command("black");
        break;
      case "w":
      case "W":
      case ",":
        command("white");
        break;
      case "Escape":
      case "-":
        onEnd();
        break;
    }
  };

  const onPointerDown = (e: React.PointerEvent) => {
    e.preventDefault();
    ref.current?.focus({ preventScroll: true });
    if (e.button === 0) command("next");
    else if (e.button === 2) command("prev");
  };

  const onWheel = (e: React.WheelEvent) => {
    wheel.current += e.deltaY;
    if (Math.abs(wheel.current) < WHEEL_STEP) return;
    command(wheel.current > 0 ? "next" : "prev");
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
      {state?.slide === -1 && <div className="show-end">投影片放映結束，按一下以結束。</div>}
    </div>
  );
}
