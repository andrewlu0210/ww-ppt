// Loads system font files into the WASM text engine on demand.
import type { App } from "../wasm/ppt_wasm";
import { inTauri, readFont, resolveFont } from "./backend";
import { mark } from "./perf";

/// Always available for missing glyphs: a Latin UI face, Traditional Chinese,
/// and symbol faces for arrows, shapes and dingbats used as bullets.
export const FALLBACK_FAMILIES = ["Helvetica Neue", "PingFang TC", "Apple Symbols", "Zapf Dingbats"];
/// Color emoji: in every font stack, but its file (~190MB) is only loaded for decks with emoji
/// (`fontFamilies()` lists it then). Must match `EMOJI_FAMILY` in ppt-model.
export const EMOJI_FAMILY = "Apple Color Emoji";

interface FontState {
  loadedPaths: Set<string>;
  resolved: Map<string, string>;
}

// Keyed by engine instance: a fresh engine (e.g. after hot reload) starts with no fonts.
const states = new WeakMap<App, FontState>();

/// Makes sure every family in `families` (or its substitute) is registered.
/// Calls `onProgress` after each newly registered file so the canvas can repaint.
/// Returns how many font files were newly registered.
export async function ensureFonts(app: App, families: string[], onProgress?: () => void): Promise<number> {
  if (!inTauri) return 0;
  let added = 0;
  let state = states.get(app);
  if (!state) {
    state = { loadedPaths: new Set(), resolved: new Map() };
    states.set(app, state);
  }
  const { loadedPaths, resolved } = state;
  for (const requested of families) {
    const key = requested.toLowerCase();
    if (resolved.has(key)) continue;
    const font = await resolveFont(requested);
    if (!font) continue;
    resolved.set(key, font.family);
    for (const path of font.paths) {
      if (loadedPaths.has(path)) continue;
      loadedPaths.add(path);
      const t = performance.now();
      const bytes = await readFont(path);
      const read = performance.now() - t;
      app.registerFont(bytes);
      added++;
      mark(`font ${path.split("/").pop()} (${(bytes.length / 1e6).toFixed(1)} MB): read ${read.toFixed(0)} + register`, performance.now() - t);
      onProgress?.();
    }
    if (font.family.toLowerCase() !== key) app.setFontAlias(requested, font.family);
  }
  return added;
}
