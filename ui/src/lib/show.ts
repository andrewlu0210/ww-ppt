// Slide show state and input mapping shared by the slide view, the presenter
// view and the audience window.

export interface ShowState {
  /** Slide on screen; -1 on the end screen. */
  slide: number;
  /** What comes next (the presenter's preview); -1 is the end screen. */
  next: number;
  count: number;
  blank: "" | "black" | "white";
  /** A transition is playing (the audience window plays it too). */
  transition: boolean;
  /** The current slide's transition length, 0 if it has none. */
  transitionMs: number;
  /** The current slide advances by itself after this long (-1: it doesn't). */
  afterMs: number;
  /** Mouse clicks advance (keys always do). */
  onClick: boolean;
}

/** A navigation command for `App.showCommand` (plus "end"). */
export interface ShowCommand {
  name: "next" | "prev" | "first" | "last" | "goto" | "black" | "white" | "end";
  index?: number;
}

/** Label of the window that shows the slides on the second display. */
export const AUDIENCE_LABEL = "audience";

/**
 * Maps a key press to a show command. `digits` collects a typed slide
 * number until Enter. Returns "toggleView" for ⌥↩ (slides ↔ presenter view).
 */
export function showKeyCommand(e: { key: string; altKey: boolean }, digits: { current: string }): ShowCommand | "toggleView" | null {
  const k = e.key;
  if (k === "Enter" && e.altKey) return "toggleView";
  if (/^[0-9]$/.test(k)) {
    digits.current += k;
    return null;
  }
  if (k === "Enter" && digits.current) {
    const n = parseInt(digits.current, 10);
    digits.current = "";
    return { name: "goto", index: Math.max(0, n - 1) };
  }
  digits.current = "";
  switch (k) {
    case "ArrowRight":
    case "ArrowDown":
    case "PageDown":
    case " ":
    case "Enter":
    case "n":
    case "N":
      return { name: "next" };
    case "ArrowLeft":
    case "ArrowUp":
    case "PageUp":
    case "Backspace":
    case "p":
    case "P":
      return { name: "prev" };
    case "Home":
      return { name: "first" };
    case "End":
      return { name: "last" };
    case "b":
    case "B":
    case ".":
      return { name: "black" };
    case "w":
    case "W":
    case ",":
      return { name: "white" };
    case "Escape":
    case "-":
      return { name: "end" };
  }
  return null;
}

/** Formats seconds as h:mm:ss (or m:ss under an hour). */
export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s % 60)}` : `${pad(m)}:${pad(s % 60)}`;
}
