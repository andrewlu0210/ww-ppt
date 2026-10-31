// Light / dark interface appearance. The choice is stored per machine; "system"
// follows macOS. Only the editor chrome changes: slides keep their own colors.

import { setTheme } from "@tauri-apps/api/app";
import { inTauri } from "./backend";

export type Appearance = "light" | "dark" | "system";

const KEY = "ww-ppt.appearance";
const media = window.matchMedia("(prefers-color-scheme: dark)");

export function loadAppearance(): Appearance {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // Storage unavailable: use the default.
  }
  return "system";
}

/** The appearance actually shown right now. */
export function resolvedAppearance(): "light" | "dark" {
  return document.documentElement.dataset.appearance === "dark" ? "dark" : "light";
}

/** A color token from the stylesheet (e.g. `--workspace`) for canvas drawing. */
export function cssColor(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/**
 * Applies `pref` to the page and to the native windows, menus and dialogs, and
 * calls `onChange` whenever the shown appearance changes (including macOS
 * switching while following the system). Returns a function that stops listening.
 */
export function applyAppearance(pref: Appearance, onChange: (shown: "light" | "dark") => void): () => void {
  try {
    localStorage.setItem(KEY, pref);
  } catch {
    // Not remembered; the default applies next time.
  }
  let first = true;
  const update = () => {
    const shown = pref === "system" ? (media.matches ? "dark" : "light") : pref;
    if (first || document.documentElement.dataset.appearance !== shown) {
      first = false;
      document.documentElement.dataset.appearance = shown;
      onChange(shown);
    }
  };
  // The webview follows the app's native appearance, so with a forced theme
  // `prefers-color-scheme` agrees with it; with "system" it tracks macOS.
  if (inTauri) void setTheme(pref === "system" ? null : pref).catch(() => {});
  update();
  media.addEventListener("change", update);
  return () => media.removeEventListener("change", update);
}

/** Sets the stored appearance before the first render, so the page never flashes light. */
export function initAppearance() {
  const pref = loadAppearance();
  document.documentElement.dataset.appearance = pref === "system" ? (media.matches ? "dark" : "light") : pref;
}
