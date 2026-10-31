// Spoken feedback for screen readers (VoiceOver) through a polite live region.

import { t } from "../i18n";

let region: HTMLElement | null = null;
let timer = 0;
/** Messages waiting for the next update, spoken together. */
let pending: string[] = [];
/** The last message, for tests and the automation channel. */
export let lastAnnouncement = "";

function live(): HTMLElement {
  if (!region || !region.isConnected) {
    region = document.createElement("div");
    region.className = "sr-only";
    region.setAttribute("aria-live", "polite");
    region.setAttribute("aria-atomic", "true");
    region.setAttribute("role", "status");
    document.body.appendChild(region);
  }
  return region;
}

/** Speaks `message` once. Repeating the same text speaks it again. */
export function announce(message: string) {
  if (!message) return;
  if (!pending.includes(message)) pending.push(message);
  const el = live();
  el.textContent = "";
  window.clearTimeout(timer);
  // A fresh text node after a pause makes VoiceOver read even a repeated message.
  timer = window.setTimeout(() => {
    lastAnnouncement = pending.join(t("common.sentenceSeparator"));
    pending = [];
    el.textContent = lastAnnouncement;
  }, 80);
}
