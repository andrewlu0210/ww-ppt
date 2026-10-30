// Keyboard navigation outside the canvas. WebKit only moves Tab focus between
// text fields by default, so buttons, checkboxes and menus are reached through
// these handlers instead: F6 cycles the panes (like PowerPoint), arrow keys
// move within the ribbon and menus, and Tab is kept inside open popovers.
import { focusCanvas } from "../components/CanvasHost";

const FOCUSABLE = "button, input, select, textarea, [tabindex]:not([tabindex='-1'])";

/** Enabled, visible controls inside `root`, in document order. */
export function focusables(root: Element | null): HTMLElement[] {
  if (!root) return [];
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((el) => !(el as HTMLButtonElement).disabled && !el.hidden && el.getClientRects().length > 0 && !el.closest("[hidden]"));
}

/** Moves focus `step` places from the focused element among `items` (wrapping). Returns false if focus is not among them. */
export function moveFocus(items: HTMLElement[], step: number): boolean {
  if (items.length === 0) return false;
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = at < 0 ? (step > 0 ? 0 : items.length - 1) : (at + step + items.length) % items.length;
  items[next].focus();
  return true;
}

/** Whether arrow keys belong to the element itself (text entry, number boxes, drop-downs). */
export function ownsArrows(el: Element | null): boolean {
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
  return el instanceof HTMLInputElement && !["checkbox", "radio", "button", "color", "range"].includes(el.type);
}

/** Briefly outlines a pane that just got keyboard focus, so it is clear where focus went. */
function flash(el: Element | null) {
  if (!el) return;
  el.classList.remove("pane-focus");
  void (el as HTMLElement).offsetWidth;
  el.classList.add("pane-focus");
  window.setTimeout(() => el.classList.remove("pane-focus"), 900);
}

type Pane = { root: () => Element | null; focus: () => boolean };

const PANES: Pane[] = [
  {
    root: () => document.querySelector(".canvas-area"),
    focus: () => {
      if (!document.querySelector(".canvas-area")) return false;
      focusCanvas();
      return true;
    },
  },
  {
    root: () => document.querySelector(".selection-pane"),
    focus: () => {
      const list = document.querySelector<HTMLElement>(".selection-pane .pane-list");
      list?.focus();
      return !!list;
    },
  },
  {
    root: () => document.querySelector(".notes-pane"),
    focus: () => {
      const notes = document.querySelector<HTMLTextAreaElement>(".notes-input");
      notes?.focus();
      return !!notes;
    },
  },
  {
    root: () => document.querySelector(".slide-pane"),
    focus: () => {
      const item = document.querySelector<HTMLElement>(".slide-item.current") ?? document.querySelector<HTMLElement>(".slide-item");
      item?.focus();
      return !!item;
    },
  },
  {
    root: () => document.querySelector(".ribbon"),
    focus: () => {
      const tab = document.querySelector<HTMLElement>(".rb-tabs [aria-selected='true']");
      tab?.focus();
      return !!tab;
    },
  },
];

/** The slide sorter replaces the canvas, notes and slide panes. */
const SORTER_PANES: Pane[] = [
  {
    root: () => document.querySelector(".slide-sorter"),
    focus: () => {
      const s = document.querySelector<HTMLElement>(".slide-sorter");
      s?.focus();
      return !!s;
    },
  },
  PANES[4],
];

/** F6 / Shift+F6: focus the next or previous pane. */
export function cyclePanes(back: boolean) {
  const panes = document.querySelector(".slide-sorter") ? SORTER_PANES : PANES;
  const active = document.activeElement;
  let at = panes.findIndex((p) => p.root()?.contains(active));
  for (let i = 0; i < panes.length; i++) {
    at = (at + (back ? -1 : 1) + panes.length) % panes.length;
    if (panes[at].focus()) {
      flash(panes[at].root());
      return;
    }
  }
}
