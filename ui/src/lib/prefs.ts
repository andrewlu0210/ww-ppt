// Preferences kept per machine: the interface language and the autosave interval.
// (The appearance has its own module, `appearance.ts`.) They live in localStorage,
// which every window of the app shares; the language is also handed to the backend so
// the native menus follow it.

import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { inTauri } from "./backend";
import { storedLang, type Lang } from "../i18n";

export type LanguagePref = Lang | "system";

const AUTOSAVE_KEY = "ww-ppt.autosaveSeconds";

/** Autosave intervals offered in Preferences, in seconds; 0 turns it off. */
export const AUTOSAVE_CHOICES = [0, 30, 60, 120, 300, 600];
const AUTOSAVE_DEFAULT = 30;

/** Seconds between recovery autosaves while a document has unsaved changes; 0 means off. */
export function autosaveSeconds(): number {
  try {
    const v = Number(localStorage.getItem(AUTOSAVE_KEY));
    if (localStorage.getItem(AUTOSAVE_KEY) !== null && AUTOSAVE_CHOICES.includes(v)) return v;
  } catch {
    // Storage unavailable: use the default.
  }
  return AUTOSAVE_DEFAULT;
}

export function setAutosaveSeconds(seconds: number) {
  try {
    localStorage.setItem(AUTOSAVE_KEY, String(seconds));
  } catch {
    // Not remembered; the default applies next time.
  }
}

export function languagePref(): LanguagePref {
  return storedLang() ?? "system";
}

/** Stores the language choice for the next launch, here and in the backend (for the menus). */
export async function setLanguagePref(pref: LanguagePref) {
  try {
    if (pref === "system") localStorage.removeItem("ww-ppt.language");
    else localStorage.setItem("ww-ppt.language", pref);
  } catch {
    // Not remembered; the system language applies next time.
  }
  if (inTauri) await invoke("set_language_pref", { value: pref }).catch(() => {});
}

/** Tells the main window the appearance was changed in Preferences. */
export function announceAppearance(value: string) {
  if (inTauri) void emit("appearance-changed", value);
}
