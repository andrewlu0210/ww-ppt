// Interface language: Traditional Chinese, Simplified Chinese or English,
// chosen from the system's preferred languages at startup (macOS can also set
// a language per app in System Settings). Anything else falls back to English.

import { en } from "./en";
import { zhHans } from "./zh-Hans";
import { zhHant, type Messages } from "./zh-Hant";

export type Lang = "zh-Hant" | "zh-Hans" | "en";

/** Maps a BCP 47 tag to a supported language, or null. */
export function matchLang(tag: string): Lang | null {
  const t = tag.toLowerCase();
  if (!t.startsWith("zh")) return t.startsWith("en") ? "en" : null;
  // Script first (zh-Hans-TW is Simplified), then the region's usual script.
  if (t.includes("hans")) return "zh-Hans";
  if (t.includes("hant")) return "zh-Hant";
  return /-(cn|sg|my)\b/.test(t) ? "zh-Hans" : "zh-Hant";
}

function detect(): Lang {
  for (const tag of navigator.languages ?? [navigator.language]) {
    const lang = matchLang(tag);
    if (lang) return lang;
  }
  return "en";
}

export const lang: Lang = detect();

/** The locale for dates and numbers. */
export const locale = { "zh-Hant": "zh-TW", "zh-Hans": "zh-CN", en: "en-US" }[lang];

const messages: Messages = { "zh-Hant": zhHant, "zh-Hans": zhHans, en }[lang];

export type MessageKey = keyof Messages;

/** The message for `key`, with `{name}` placeholders filled in from `params`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  let text = messages[key];
  if (text.includes("|") && typeof params?.count === "number") {
    const [one, other] = text.split("|");
    text = params.count === 1 ? one : other;
  }
  return params ? text.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m)) : text;
}
