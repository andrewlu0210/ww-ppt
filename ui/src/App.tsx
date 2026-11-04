import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import type { App as Engine } from "./wasm/ppt_wasm";
import { CanvasHost, focusCanvas } from "./components/CanvasHost";
import { Ribbon } from "./components/Ribbon";
import { SlidePane } from "./components/SlidePane";
import { SlideShow } from "./components/SlideShow";
import { ReadingBar } from "./components/ReadingBar";
import { NarrationRecorder, type Take } from "./lib/narration";
import { domSlideBox, ShowMedia } from "./lib/showMedia";
import { PresenterView, type ShowTimer } from "./components/PresenterView";
import { AUDIENCE_LABEL, type ShowCommand, type ShowState } from "./lib/show";
import { MIN_NOTES_HEIGHT, NotesPane, notesFocused } from "./components/NotesPane";
import { StatusBar } from "./components/StatusBar";
import { Rulers } from "./components/Ruler";
import { SlideSorter } from "./components/SlideSorter";
import {
  askSaveChanges,
  inTauri,
  listFonts,
  onMenu,
  onOpenFiles,
  pickImage,
  pickPptx,
  pickSavePath,
  readFile,
  showError,
  startupFiles,
  askRecover,
  recoveryClear,
  recoveryList,
  recoveryRead,
  recoveryWrite,
  writeFile,
  APP_NAME,
} from "./lib/backend";
import { startAutomation } from "./lib/automation";
import { cyclePanes } from "./lib/keyboard";
import { mark, nextFrame, timed } from "./lib/perf";
import { SHAPES_MARKER, type Command, type TableStyleInfo } from "./lib/commands";
import { EMOJI_FAMILY, ensureFonts, FALLBACK_FAMILIES } from "./lib/fonts";
import { EMPTY_STATUS, type AnimationPreset, type Status } from "./lib/status";
import { AnimationBadges } from "./components/AnimationTab";
import { FindBar, type FindBarHandle } from "./components/FindBar";
import { SpellMenu, type SpellHit } from "./components/SpellMenu";
import { ContextMenu } from "./components/ContextMenu";
import { FontDialog } from "./components/FontDialog";
import { ParagraphDialog } from "./components/ParagraphDialog";
import { AltTextForm } from "./components/AltTextDialog";
import { buildContextMenu, type ContextKind } from "./lib/contextMenu";
import { SHAPE_CATEGORIES, type PresetIcon } from "./components/ShapeGallery";
import type { HeaderFooterInfo } from "./components/HeaderFooterDialog";
import type { AltTextInfo } from "./components/AltTextDialog";
import { SelectionPane } from "./components/SelectionPane";
import { FormatPane } from "./components/FormatPane";
import { DiagramPane } from "./components/DiagramPane";
import { VideoDialog } from "./components/VideoDialog";
import { ZoomDialog } from "./components/ZoomDialog";
import { CheckerPane, type A11yIssue } from "./components/CheckerPane";
import { A11yMirror } from "./components/A11yMirror";
import { announce } from "./lib/announce";
import { openAbout } from "./components/AboutWindow";
import { openPreferences } from "./components/PreferencesWindow";
import { autosaveSeconds } from "./lib/prefs";
import type { BackgroundInfo, ColorSchemeInfo, FontSchemeInfo, ThemeInfo } from "./components/DesignTab";
import { setThemePalette } from "./components/ColorPicker";
import { applyAppearance, cssColor, loadAppearance, type Appearance } from "./lib/appearance";
import { t } from "./i18n";

const THUMB_WIDTH = 320;
const NOTES_KEY = "ww-ppt.notes";
const PRESENTER_KEY = "ww-ppt.presenter";
const SPELL_KEY = "ww-ppt.spell";
/** Quiet time after a change before the slide's text is spell checked again. */
const SPELL_DELAY_MS = 400;

function loadPresenterPref(): boolean {
  try {
    return localStorage.getItem(PRESENTER_KEY) !== "0";
  } catch {
    return true;
  }
}

function loadSpellPref(): boolean {
  try {
    return localStorage.getItem(SPELL_KEY) !== "0";
  } catch {
    return true;
  }
}

/** Notes pane visibility and height, remembered per machine. */
function loadNotesPrefs(): { open: boolean; height: number } {
  try {
    const v = JSON.parse(localStorage.getItem(NOTES_KEY) ?? "null") as { open?: unknown; height?: unknown } | null;
    return { open: v?.open !== false, height: typeof v?.height === "number" ? Math.max(MIN_NOTES_HEIGHT, v.height) : 110 };
  } catch {
    return { open: true, height: 110 };
  }
}

const VIEW_KEY = "ww-ppt.view";
/** Sorter thumbnail width (CSS px) at 100%. */
const SORTER_BASE = 200;

interface ViewPrefs {
  ruler: boolean;
  /** The selection pane is open. */
  pane: boolean;
  /** The accessibility checker is open. */
  checker: boolean;
  /** The Format Shape pane is open. */
  format: boolean;
  grid: boolean;
  /** Slide sorter thumbnail width in CSS px. */
  sorterSize: number;
}

/** Ruler, gridlines and sorter size, remembered per machine. */
/** Autosave a few seconds after editing stops, at most this often. */
const AUTOSAVE_IDLE_MS = 3000;

const newSessionId = () => crypto.randomUUID();

function loadViewPrefs(): ViewPrefs {
  try {
    const v = JSON.parse(localStorage.getItem(VIEW_KEY) ?? "null") as Partial<ViewPrefs> | null;
    return { ruler: v?.ruler === true, pane: v?.pane === true, checker: v?.checker === true, format: v?.format === true, grid: v?.grid === true, sorterSize: typeof v?.sorterSize === "number" ? v.sorterSize : SORTER_BASE };
  } catch {
    return { ruler: false, pane: false, checker: false, format: false, grid: false, sorterSize: SORTER_BASE };
  }
}

interface Doc {
  name: string;
  path: string | null;
  savedRevision: number;
}

export function App() {
  const engineRef = useRef<Engine | null>(null);
  const [status, setStatus] = useState<Status>(EMPTY_STATUS);
  // Slide names for the slide pane, the sorter and slide show announcements.
  const slideLabelsRef = useRef<string[]>([]);
  const slideLabels = useMemo(
    () => (engineRef.current ? (engineRef.current.slideLabels() as string[]) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [status.revision, status.slideCount],
  );
  slideLabelsRef.current = slideLabels;
  // Accessibility problems, rechecked as the deck changes.
  const a11yIssues = useMemo(
    () => (engineRef.current ? (engineRef.current.checkAccessibility() as A11yIssue[]) : []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [status.revision, status.slideCount],
  );
  // What VoiceOver hears: slide changes, the selection, keyboard moves.
  const spoken = useRef({ slide: -1, selection: "", frame: "" });
  useEffect(() => {
    const engine = engineRef.current;
    if (!engine || showingRef.current) return;
    const prev = spoken.current;
    const selection = engine.describeSelection();
    const f = status.frame;
    const frame = f ? `${f.x},${f.y},${f.w},${f.h},${f.rot}` : "";
    const input = document.querySelector(".ime-input");
    input?.setAttribute("aria-label", t("app.editArea", { detail: selection || engine.describeSlide() }));
    if (status.slideCount > 0 && status.slideIndex !== prev.slide) {
      announce(engine.describeSlide());
    } else if (selection !== prev.selection) {
      announce(selection || (prev.selection ? t("app.deselected") : ""));
    } else if (f && frame !== prev.frame && prev.frame && performance.now() - lastKeyAt.current < 500) {
      const cm = (emu: number) => (emu / 360_000).toFixed(2);
      const [px, py, pw, ph, prot] = prev.frame.split(",").map(Number);
      if (f.rot !== prot) announce(t("app.rotated", { deg: Math.round(f.rot) }));
      else if (f.w !== pw || f.h !== ph) announce(t("app.resized", { w: cm(f.w), h: cm(f.h) }));
      else if (f.x !== px || f.y !== py) announce(t("app.moved", { x: cm(f.x), y: cm(f.y) }));
    }
    spoken.current = { slide: status.slideIndex, selection, frame };
  }, [status]);
  const [doc, setDoc] = useState<Doc>({ name: t("doc.defaultName"), path: null, savedRevision: 0 });
  const [error, setError] = useState<string | null>(null);
  const [fontsLoading, setFontsLoading] = useState(false);
  const [fonts, setFonts] = useState<string[]>([]);
  const [layoutNames, setLayoutNames] = useState<string[]>([]);
  const [layoutThumbs, setLayoutThumbs] = useState<(ImageData | undefined)[]>([]);
  const [thumbs, setThumbs] = useState<Map<number, ImageData>>(new Map());
  const [linkRequest, setLinkRequest] = useState(0);
  const [tabRequest, setTabRequest] = useState<{ tab: string; n: number }>({ tab: "", n: 0 });
  /** The right-click menu (objects, text, blank area), and the alt text form it can open. */
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; kind: ContextKind } | null>(null);
  const [altPopup, setAltPopup] = useState<{ x: number; y: number } | null>(null);
  const [tableStyles, setTableStyles] = useState<TableStyleInfo[]>([]);
  const [animPresets, setAnimPresets] = useState<AnimationPreset[]>([]);
  const [shapeIcons, setShapeIcons] = useState<Map<string, PresetIcon>>(new Map());
  const [theme, setTheme] = useState<ThemeInfo | null>(null);
  const [schemes, setSchemes] = useState<{ colors: ColorSchemeInfo[]; fonts: FontSchemeInfo[] }>({ colors: [], fonts: [] });
  const [slideSize, setSlideSize] = useState<[number, number]>([12_192_000, 6_858_000]);
  const deckStyleKey = useRef("");
  const [ribbonTab, setRibbonTab] = useState("");
  const findRef = useRef<FindBarHandle>(null);
  /** Slide show in progress: the slides full screen, or the presenter view. */
  const [showMode, setShowMode] = useState<"slides" | "presenter" | null>(null);
  const showing = showMode !== null;
  /** Reading view: the show plays inside the window (title bar and status bar stay). */
  const [reading, setReading] = useState(false);
  const readingRef = useRef(false);
  readingRef.current = reading;
  /// Narration being recorded during the show (the microphone is open).
  const recorderRef = useRef<NarrationRecorder | null>(null);
  const [recording, setRecording] = useState(false);
  /// Sounds and videos of the show in progress (none while recording narration).
  const showMediaRef = useRef<ShowMedia | null>(null);
  // Whether the slide on screen has recorded narration (the engine knows; the status revision says when to ask).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const hasNarration = useMemo(() => engineRef.current?.hasNarration(status.slideIndex) ?? false, [status.revision, status.slideIndex]);
  const [showState, setShowState] = useState<ShowState | null>(null);
  // In a show, each new slide is spoken (the end screen too).
  const shownSlide = showState?.slide;
  useEffect(() => {
    if (shownSlide === undefined) return;
    announce(shownSlide < 0 ? t("app.showEnded") : (slideLabelsRef.current[shownSlide] ?? t("slide.label", { n: shownSlide + 1 })));
  }, [shownSlide]);
  /** Whether the slides are on another display (the audience window is open). */
  const [audienceOpen, setAudienceOpen] = useState(false);
  const showTimer = useRef<ShowTimer>({ elapsed: 0, since: null });
  const [usePresenter, setUsePresenter] = useState(loadPresenterPref);
  const usePresenterRef = useRef(usePresenter);
  usePresenterRef.current = usePresenter;
  useEffect(() => {
    try {
      localStorage.setItem(PRESENTER_KEY, usePresenter ? "1" : "0");
    } catch {
      // Not remembered; fine.
    }
  }, [usePresenter]);
  const [notesPrefs, setNotesPrefs] = useState(loadNotesPrefs);
  const [viewPrefs, setViewPrefs] = useState(loadViewPrefs);
  const refreshRef = useRef<() => void>(() => {});
  // SmartArt text pane: open, the row being edited (shared with the ribbon), and focus requests.
  const [videoOpen, setVideoOpen] = useState(false);
  const [zoomOpen, setZoomOpen] = useState(false);
  /** The Font or Paragraph dialog from the right-click menu. */
  const [textDialog, setTextDialog] = useState<"font" | "paragraph" | null>(null);
  // Format painter: copied a format and waiting for the target to apply it to.
  const [painter, setPainter] = useState<"once" | "sticky" | null>(null);
  const [diagramPane, setDiagramPane] = useState(false);
  const [diagramRow, setDiagramRow] = useState(0);
  const diagramRowRef = useRef(0);
  diagramRowRef.current = diagramRow;
  const [diagramFocus, setDiagramFocus] = useState(0);
  const [diagramRange, setDiagramRange] = useState<[number, number] | null>(null);
  // Another selection starts at the first node (unless the request below names one: it runs after).
  useEffect(() => setDiagramRow(0), [status.selectionName, status.slideIndex]);
  // A double-click on a SmartArt (or a find match in it) opens its text pane at that node.
  useEffect(() => {
    if (!status.diagramRequests) return;
    setDiagramRow(Math.max(0, status.diagramFocus ?? 0));
    setDiagramRange(status.diagramRange ?? null);
    setDiagramPane(true);
    setDiagramFocus((n) => n + 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status.diagramRequests]);
  const diagramShown = diagramPane && !!status.smartArt;
  const [viewMode, setViewMode] = useState<"normal" | "sorter">("normal");
  const viewModeRef = useRef(viewMode);
  viewModeRef.current = viewMode;
  // The WebView's own right-click menu (Reload, ...) has no place in an editor, and Reload would
  // throw away unsaved work. Text fields keep their native cut/copy/paste menu.
  useEffect(() => {
    const onMenu = (e: MouseEvent) => {
      if (!(e.target as Element | null)?.closest?.("input, textarea, [contenteditable='true']")) e.preventDefault();
    };
    document.addEventListener("contextmenu", onMenu);
    return () => document.removeEventListener("contextmenu", onMenu);
  }, []);
  // Last key press, so keyboard moves of the selection can be spoken.
  const lastKeyAt = useRef(0);
  useEffect(() => {
    const onKey = () => (lastKeyAt.current = performance.now());
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
  useEffect(() => {
    try {
      localStorage.setItem(VIEW_KEY, JSON.stringify(viewPrefs));
    } catch {
      // Not remembered; the defaults apply next time.
    }
    engineRef.current?.setShowGrid(viewPrefs.grid);
    // The "gridlines" checkbox reads its state from the status, which must follow.
    refreshRef.current();
  }, [viewPrefs]);
  const [appearance, setAppearance] = useState(loadAppearance);
  useEffect(() => applyAppearance(appearance, () => engineRef.current?.setWorkspaceColor(cssColor("--workspace"))), [appearance]);
  // The Preferences window changes the appearance too.
  useEffect(() => {
    if (!inTauri) return;
    const unlisten = listen<Appearance>("appearance-changed", (e) => setAppearance(e.payload));
    return () => void unlisten.then((f) => f());
  }, []);
  const [spellOn, setSpellOn] = useState(loadSpellPref);
  const [spellMenu, setSpellMenu] = useState<{ x: number; y: number; hit: SpellHit; guesses: string[] | null } | null>(null);
  /** Misspellings by checked text, so unchanged texts aren't sent again. */
  const spellCache = useRef(new Map<string, number[]>());
  const [spellEpoch, setSpellEpoch] = useState(0);
  useEffect(() => {
    try {
      localStorage.setItem(SPELL_KEY, spellOn ? "1" : "0");
    } catch {
      // Not remembered; fine.
    }
  }, [spellOn]);
  useEffect(() => {
    try {
      localStorage.setItem(NOTES_KEY, JSON.stringify(notesPrefs));
    } catch {
      // Storage may be unavailable; the defaults are fine.
    }
  }, [notesPrefs]);
  const showingRef = useRef(false);
  showingRef.current = showing;
  /** Whether the window was already full screen when the show started (so ending it leaves it so). */
  const wasFullscreen = useRef(false);

  const dirty = status.revision !== doc.savedRevision;
  const docRef = useRef(doc);
  docRef.current = doc;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  // --- Crash recovery: the open document is autosaved while it has unsaved changes.
  const sessionId = useRef<string>(newSessionId());
  /** Revision of the last autosave, or null when no copy exists for this session. */
  const autosaved = useRef<number | null>(null);
  const autosave = useRef({ timer: 0, since: 0, last: 0 });
  useEffect(() => {
    const a = autosave.current;
    window.clearTimeout(a.timer);
    if (!dirty) {
      a.since = 0;
      if (autosaved.current !== null) {
        autosaved.current = null;
        void recoveryClear(sessionId.current);
      }
      return;
    }
    // Preferences can turn the autosave off or change its interval.
    const everyMs = autosaveSeconds() * 1000;
    if (everyMs === 0) return;
    const now = Date.now();
    a.since ||= now;
    // A few seconds after editing stops, at most every everyMs, and not
    // postponed for longer than that by continuous editing.
    const idle = now - a.since >= everyMs ? 0 : AUTOSAVE_IDLE_MS;
    const wait = Math.max(idle, a.last + everyMs - now);
    a.timer = window.setTimeout(() => {
      const engine = engineRef.current;
      if (!engine || !dirtyRef.current || autosaved.current === engine.revision()) return;
      a.since = 0;
      a.last = Date.now();
      const revision = engine.revision();
      try {
        const t0 = performance.now();
        const bytes = engine.savePptx();
        mark("autosave: serialize", performance.now() - t0);
        void recoveryWrite(sessionId.current, docRef.current.name, docRef.current.path, bytes).then(
          () => void (autosaved.current = revision),
          (e) => console.warn("autosave failed", e),
        );
      } catch (e) {
        console.warn("autosave failed", e);
      }
    }, wait);
    return () => window.clearTimeout(a.timer);
  }, [dirty, status.revision]);

  // --- Thumbnails -------------------------------------------------------------
  // Slide thumbnails are rendered one at a time from a queue of stale slides,
  // nearest the user's attention first, in between interaction.
  const thumbState = useRef({
    revision: -1,
    count: -1,
    timer: 0,
    /** A refresh of every thumbnail is waiting for the timer. */
    pendingAll: false,
    /** Slides whose thumbnail is out of date. */
    stale: new Set<number>(),
    running: false,
    /** While a document opens its fonts load: thumbnails wait for them. */
    held: false,
    /** Thumbnails rendered and the slowest, since the queue last ran dry. */
    done: 0,
    slowest: 0,
    started: 0,
  });
  /** When the user last pressed a pointer or key: thumbnails stay out of the way for a moment. */
  const lastInput = useRef(0);
  const pointerDown = useRef(false);
  useEffect(() => {
    const busy = () => (lastInput.current = performance.now());
    const down = () => {
      pointerDown.current = true;
      busy();
    };
    const up = () => {
      pointerDown.current = false;
      busy();
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("pointerup", up, true);
    window.addEventListener("keydown", busy, true);
    window.addEventListener("wheel", busy, { capture: true, passive: true });
    return () => {
      window.removeEventListener("pointerdown", down, true);
      window.removeEventListener("pointerup", up, true);
      window.removeEventListener("keydown", busy, true);
      window.removeEventListener("wheel", busy, true);
    };
  }, []);

  /** The next stale slide: the current one, then those on screen in the slide pane or sorter, then outwards. */
  const nextThumb = useCallback((stale: Set<number>, current: number): number | undefined => {
    if (stale.has(current)) return current;
    const list = document.querySelector(".slide-sorter") ?? document.querySelector(".slide-pane");
    const items = list ? Array.from(list.querySelectorAll<HTMLElement>(".sorter-item, .slide-item")) : [];
    const view = list?.getBoundingClientRect();
    if (view) {
      const shown = items.findIndex((el, i) => {
        if (!stale.has(i)) return false;
        const r = el.getBoundingClientRect();
        return r.bottom > view.top && r.top < view.bottom;
      });
      if (shown >= 0) return shown;
    }
    let best: number | undefined;
    for (const i of stale) if (best === undefined || Math.abs(i - current) < Math.abs(best - current)) best = i;
    return best;
  }, []);

  const pumpThumbs = useCallback(async () => {
    const st = thumbState.current;
    if (st.running) return;
    st.running = true;
    try {
      while (st.stale.size > 0 && !st.held) {
        // Give way to dragging, typing and scrolling.
        while (pointerDown.current || performance.now() - lastInput.current < 300) await new Promise((r) => setTimeout(r, 100));
        const engine = engineRef.current;
        if (!engine || st.held) break;
        const s = engine.status() as Status;
        for (const i of st.stale) if (i >= s.slideCount) st.stale.delete(i);
        const i = nextThumb(st.stale, s.slideIndex);
        if (i === undefined) break;
        st.stale.delete(i);
        if (st.done === 0) st.started = performance.now();
        const ts = performance.now();
        try {
          const t = (await engine.renderThumbnail(i, THUMB_WIDTH, false)) as { width: number; height: number; pixels: Uint8Array };
          const image = new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height);
          setThumbs((prev) => new Map(prev).set(i, image));
        } catch {
          // The slide may have been deleted while rendering.
        }
        st.done++;
        st.slowest = Math.max(st.slowest, performance.now() - ts);
        // Let input and painting run before the next one.
        await new Promise((r) => setTimeout(r, 0));
      }
    } finally {
      st.running = false;
    }
    if (st.stale.size === 0 && st.done > 1) {
      mark(`thumbnails: ${st.done} slides`, performance.now() - st.started);
      mark("thumbnails: slowest one", st.slowest);
    }
    if (st.stale.size === 0) {
      st.done = 0;
      st.slowest = 0;
    }
  }, [nextThumb]);

  // When idle, the slides next to the current one are prepared (their images
  // decoded at screen size), so moving to them does not stall.
  const prefetched = useRef({ revision: -1, slides: new Set<number>() });
  useEffect(() => {
    const engine = engineRef.current;
    if (!engine || status.slideCount === 0) return;
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      const p = prefetched.current;
      if (p.revision !== status.revision) {
        p.revision = status.revision;
        p.slides.clear();
      }
      for (const i of [status.slideIndex + 1, status.slideIndex - 1]) {
        // Wait for quiet: no input and no thumbnails being drawn.
        while (!cancelled && (pointerDown.current || performance.now() - lastInput.current < 400 || thumbState.current.running)) await new Promise((r) => setTimeout(r, 150));
        if (cancelled) return;
        if (i < 0 || i >= status.slideCount || p.slides.has(i)) continue;
        p.slides.add(i);
        await timed(`prefetch slide ${i + 1}`, () => engine.prefetchSlide(i));
        await new Promise((r) => setTimeout(r, 0));
      }
    }, 300);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [status.slideIndex, status.slideCount, status.revision]);

  /** Marks the current slide's thumbnail (or all of them) stale, shortly after edits settle. */
  const scheduleThumbs = useCallback(
    (all: boolean) => {
      const st = thumbState.current;
      st.pendingAll ||= all;
      window.clearTimeout(st.timer);
      st.timer = window.setTimeout(() => {
        const engine = engineRef.current;
        if (!engine) return;
        const s = engine.status() as Status;
        // A different slide count shifts every index.
        const everything = st.pendingAll || s.slideCount !== st.count;
        st.pendingAll = false;
        if (!everything && s.revision === st.revision) return;
        st.count = s.slideCount;
        st.revision = s.revision;
        if (everything) {
          setThumbs((prev) => new Map([...prev].filter(([i]) => i < s.slideCount)));
          for (let i = 0; i < s.slideCount; i++) st.stale.add(i);
        } else {
          st.stale.add(s.slideIndex);
        }
        void pumpThumbs();
      }, st.pendingAll ? 30 : 250);
    },
    [pumpThumbs],
  );

  /** Holds thumbnails back (while a document's fonts load) and lets them go again. */
  const holdThumbs = useCallback(
    (held: boolean) => {
      thumbState.current.held = held;
      if (!held) void pumpThumbs();
    },
    [pumpThumbs],
  );

  // Coalesce status reads to one per animation frame.
  const pending = useRef(false);
  const refresh = useCallback(() => {
    if (pending.current) return;
    pending.current = true;
    requestAnimationFrame(() => {
      pending.current = false;
      const engine = engineRef.current;
      if (!engine) return;
      const s = engine.status() as Status;
      setStatus(s);
      if (s.revision !== thumbState.current.revision) scheduleThumbs(false);
    });
  }, [scheduleThumbs]);

  refreshRef.current = refresh;

  const findChanged = useCallback(
    (all: boolean) => {
      if (all) scheduleThumbs(true);
      refresh();
    },
    [refresh, scheduleThumbs],
  );

  /** Selects a find match in the notes pane, opening it; focus stays in the find bar. */
  const showNotesMatch = useCallback((start: number, end: number) => {
    setNotesPrefs((p) => (p.open ? p : { ...p, open: true }));
    // After the pane has (re)loaded the slide's notes.
    window.setTimeout(() => {
      const ta = document.querySelector<HTMLTextAreaElement>(".notes-input");
      if (!ta) return;
      const back = document.activeElement as HTMLElement | null;
      ta.focus({ preventScroll: true });
      ta.setSelectionRange(start, end);
      back?.focus({ preventScroll: true });
    }, 60);
  }, []);

  // Spell check the current slide's text a moment after it changes (results are drawn by the engine).
  useEffect(() => {
    const engine = engineRef.current;
    if (!engine || showing) return;
    if (!spellOn || !inTauri) {
      engine.clearSpelling();
      return;
    }
    const timer = window.setTimeout(async () => {
      const texts = engine.spellTexts() as { key: string; text: string }[];
      const cache = spellCache.current;
      const todo = texts.filter((t) => !cache.has(t.text));
      if (todo.length > 0) {
        const found = await invoke<[number, number][][]>("spell_check", { texts: todo.map((t) => t.text) }).catch(() => null);
        if (!found) return;
        todo.forEach((t, i) => cache.set(t.text, found[i].flat()));
      }
      for (const t of texts) engine.setSpelling(t.key, t.text, Uint32Array.from(cache.get(t.text) ?? []));
    }, SPELL_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [status, spellOn, showing, spellEpoch]);

  /** After ignoring or learning a word: check everything again. */
  const recheckSpelling = useCallback(() => {
    spellCache.current.clear();
    setSpellEpoch((n) => n + 1);
  }, []);

  /** Right click on the canvas: a misspelled word's corrections, or the menu of what was hit. */
  const onCanvasContextMenu = useCallback(
    (e: React.MouseEvent) => {
      const engine = engineRef.current;
      const canvas = document.querySelector(".canvas-host canvas");
      if (!engine || !canvas || showingRef.current || viewModeRef.current !== "normal") return;
      const r = canvas.getBoundingClientRect();
      const cx = e.clientX - r.left;
      const cy = e.clientY - r.top;
      if (cx < 0 || cy < 0 || cx > r.width || cy > r.height) return;
      e.preventDefault();
      const area = e.currentTarget.getBoundingClientRect();
      const at = { x: e.clientX - area.left, y: e.clientY - area.top };
      setCtxMenu(null);
      setAltPopup(null);
      const hit = spellOn ? (engine.spellingAt(cx, cy) as SpellHit | null) : null;
      if (hit) {
        setSpellMenu({ ...at, hit, guesses: null });
        void invoke<string[]>("spell_guesses", { text: hit.text, start: hit.start, len: hit.length })
          .catch(() => [])
          .then((guesses) => setSpellMenu((m) => (m?.hit === hit ? { ...m, guesses } : m)));
        return;
      }
      setSpellMenu(null);
      const kind = engine.contextTarget(cx, cy) as ContextKind;
      refresh();
      setCtxMenu({ ...at, kind });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [spellOn],
  );

  const loadLayoutThumbs = useCallback(async () => {
    const engine = engineRef.current;
    if (!engine) return;
    const names = engine.layoutNames() as string[];
    setLayoutNames(names);
    const out: (ImageData | undefined)[] = [];
    for (let i = 0; i < names.length; i++) {
      const t = (await engine.renderThumbnail(i, 160, true)) as { width: number; height: number; pixels: Uint8Array };
      out[i] = new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height);
    }
    setLayoutThumbs(out);
  }, []);

  const loadFonts = useCallback(
    async (families: string[]) => {
      const engine = engineRef.current;
      if (!engine) return;
      setFontsLoading(true);
      let added = 0;
      try {
        added = await ensureFonts(engine, families, refresh);
      } catch (e) {
        console.error("font loading failed", e);
      } finally {
        setFontsLoading(false);
        // New fonts change how text looks: thumbnails drawn before are out of date.
        if (added > 0) scheduleThumbs(true);
      }
    },
    [refresh, scheduleThumbs],
  );

  /** Re-reads what depends on the deck's theme and size (after loading, a restyle, or undoing one). */
  const deckStyleChanged = useCallback(async () => {
    const engine = engineRef.current;
    if (!engine) return;
    const info = engine.themeInfo() as ThemeInfo | null;
    const size = Array.from(engine.slideSize()) as [number, number];
    const key = JSON.stringify([info, size]);
    const changed = deckStyleKey.current !== key;
    deckStyleKey.current = key;
    setTheme(info);
    setSlideSize(size);
    if (info) setThemePalette(info.colors);
    setSchemes((s) => (s.colors.length > 0 ? s : { colors: engine.colorSchemes() as ColorSchemeInfo[], fonts: engine.fontSchemes() as FontSchemeInfo[] }));
    if (changed) {
      setLayoutThumbs([]);
      setTableStyles(engine.tableStyles() as TableStyleInfo[]);
      await loadFonts(engine.fontFamilies() as string[]);
    }
  }, [loadFonts]);

  /** Called after a document is loaded or created. `recovered`: it is the autosaved copy with this id. */
  const documentReplaced = useCallback(
    async (name: string, path: string | null, recovered?: string) => {
      const engine = engineRef.current;
      if (!engine) return;
      // The previous document was saved or discarded: its autosave goes, and the new one gets its own.
      if (autosaved.current !== null) void recoveryClear(sessionId.current);
      sessionId.current = recovered ?? newSessionId();
      autosaved.current = recovered ? -1 : null;
      // A recovered document has unsaved changes.
      setDoc({ name, path, savedRevision: recovered ? -1 : engine.revision() });
      setLayoutNames(engine.layoutNames() as string[]);
      setTableStyles(engine.tableStyles() as TableStyleInfo[]);
      setAnimPresets(engine.animationPresets() as AnimationPreset[]);
      setShapeIcons((icons) => (icons.size > 0 ? icons : new Map((engine.presetIcons(SHAPE_CATEGORIES.flatMap(([, names]) => names)) as PresetIcon[]).map((i) => [i.name, i]))));
      setLayoutThumbs([]);
      setThumbs(new Map());
      refresh();
      // Thumbnails drawn before the deck's fonts are in would be redone: wait for them.
      holdThumbs(true);
      scheduleThumbs(true);
      try {
        await timed("open: theme info", () => deckStyleChanged());
        await timed("open: fonts", () => loadFonts(engine.fontFamilies() as string[]));
      } finally {
        holdThumbs(false);
      }
    },
    [deckStyleChanged, holdThumbs, loadFonts, refresh, scheduleThumbs],
  );

  // --- Documents ----------------------------------------------------------------
  const saveTo = useCallback(async (path: string) => {
    const engine = engineRef.current;
    if (!engine) return false;
    try {
      const bytes = engine.savePptx();
      await writeFile(path, bytes);
      const name = path.split("/").pop()?.replace(/\.pptx$/i, "") ?? path;
      setDoc({ name, path, savedRevision: engine.revision() });
      return true;
    } catch (e) {
      await showError(t("app.saveFailed", { error: String(e) }));
      return false;
    }
  }, []);

  const saveAs = useCallback(async () => {
    const path = await pickSavePath(docRef.current.name);
    return path ? saveTo(path) : false;
  }, [saveTo]);

  const save = useCallback(async () => (docRef.current.path ? saveTo(docRef.current.path) : saveAs()), [saveAs, saveTo]);

  const exportPdf = useCallback(async () => {
    const engine = engineRef.current;
    if (!engine) return;
    const path = await pickSavePath(docRef.current.name, "pdf");
    if (!path) return;
    try {
      await writeFile(path, engine.exportPdf());
    } catch (e) {
      await showError(t("app.pdfFailed", { error: String(e) }));
    }
  }, []);

  /** Asks about unsaved changes; resolves true when it is fine to discard the document. */
  const confirmDiscard = useCallback(async () => {
    if (!dirtyRef.current) return true;
    const answer = await askSaveChanges(docRef.current.name);
    if (answer === "save") return save();
    return answer === "discard";
  }, [save]);

  const openPath = useCallback(
    async (path: string) => {
      const engine = engineRef.current;
      if (!engine) return;
      try {
        const t0 = performance.now();
        const bytes = await timed("open: read file", () => readFile(path));
        await timed("open: parse (loadPptx)", () => engine.loadPptx(bytes));
        setError(null);
        await timed("open: fonts + UI", () => documentReplaced(path.split("/").pop()?.replace(/\.pptx$/i, "") ?? path, path));
        await nextFrame();
        mark("open: first slide on screen", performance.now() - t0);
      } catch (e) {
        setError(t("app.openFailed", { error: String(e) }));
      }
    },
    [documentReplaced],
  );

  // --- Slide show ----------------------------------------------------------------
  /** Opens the audience window full screen on a display other than this window's. */
  const openAudience = useCallback(async (forceWindowed = false) => {
    const engine = engineRef.current;
    if (!engine) return false;
    const monitors = await availableMonitors();
    const here = await currentMonitor();
    const target = monitors.find((m) => m.name !== here?.name || m.position.x !== here.position.x || m.position.y !== here.position.y);
    if (!target && !forceWindowed) return false;
    // The audience window loads its own copy of the deck, unsaved edits included.
    await invoke("set_show_deck", engine.savePptx());
    const at = target ?? here;
    const scale = at?.scaleFactor ?? 1;
    const win = new WebviewWindow(AUDIENCE_LABEL, {
      url: "index.html?audience",
      title: t("tab.slideShow"),
      x: (at?.position.x ?? 0) / scale + (target ? 0 : 60),
      y: (at?.position.y ?? 0) / scale + (target ? 0 : 60),
      width: target ? target.size.width / scale : 800,
      height: target ? target.size.height / scale : 450,
      focus: false,
    });
    await new Promise<void>((resolve, reject) => {
      void win.once("tauri://created", () => resolve());
      void win.once("tauri://error", (e) => reject(new Error(String(e.payload))));
    });
    if (target) await win.setFullscreen(true);
    void win.onCloseRequested(() => setAudienceOpen(false));
    setAudienceOpen(true);
    return true;
  }, []);

  /** Sends the engine's show state to the views and the audience window. */
  const syncShow = useCallback(() => {
    const st = engineRef.current?.showState() as ShowState | null;
    setShowState(st);
    if (st) void emitTo(AUDIENCE_LABEL, "show-sync", st).catch(() => {});
  }, []);

  const startShow = useCallback(
    async (fromBeginning: boolean, presenter?: boolean, forceAudience = false, inWindow = false) => {
      const engine = engineRef.current;
      if (!engine || showingRef.current) return false;
      if (inWindow) setViewMode("normal");
      if (!engine.startShow(fromBeginning)) return false;
      // Recording narration plays nothing (the microphone would hear it).
      if (!recorderRef.current) {
        const [w, h] = Array.from(engine.slideSize());
        const media = new ShowMedia(engine, domSlideBox(w / Math.max(1, h)));
        showMediaRef.current = media.any ? media : null;
      }
      setReading(inWindow);
      showTimer.current = { elapsed: 0, since: Date.now() };
      syncShow();
      if (inWindow) {
        setShowMode("slides");
        return true;
      }
      const wantPresenter = presenter ?? usePresenterRef.current;
      const external = inTauri && (wantPresenter || forceAudience) ? await openAudience(forceAudience).catch(() => false) : false;
      // Presenter view: always with a second display (when wanted), or when asked for explicitly (⌥↩).
      setShowMode(external || presenter ? "presenter" : "slides");
      if (inTauri) {
        const win = getCurrentWindow();
        wasFullscreen.current = await win.isFullscreen();
        if (!wasFullscreen.current) await win.setFullscreen(true);
        if (external) await win.setFocus();
      }
      return true;
    },
    [openAudience, syncShow],
  );

  /** Starts a show with the microphone recording each slide. */
  const startRecording = useCallback(
    async (fromBeginning: boolean) => {
      if (showingRef.current) return;
      let recorder: NarrationRecorder;
      try {
        recorder = await NarrationRecorder.open();
      } catch {
        setError(t("record.micFailed"));
        return;
      }
      recorderRef.current = recorder;
      setRecording(true);
      // The slide on screen is picked up by the effect on `shownSlide`.
      if (!(await startShow(fromBeginning))) {
        recorderRef.current = null;
        setRecording(false);
        await recorder.finish();
      }
    },
    [startShow],
  );

  // Reading view: the slide is drawn above the navigation bar, not under it.
  useEffect(() => {
    const engine = engineRef.current;
    if (!engine || !reading || showMode !== "slides") return;
    const host = document.querySelector<HTMLElement>(".slide-show");
    const canvas = document.querySelector<HTMLCanvasElement>(".canvas-host canvas");
    if (!host || !canvas) return;
    const place = () => {
      const origin = canvas.getBoundingClientRect();
      const r = host.getBoundingClientRect();
      engine.setShowViewport(r.left - origin.left, r.top - origin.top, r.width, r.height);
    };
    place();
    const ro = new ResizeObserver(place);
    ro.observe(host);
    window.addEventListener("resize", place);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", place);
      engine.setShowViewport(0, 0, 0, 0);
    };
  }, [reading, showMode]);

  // Sounds and videos follow the show: slides, clicks and the black or white screen.
  useEffect(() => {
    const media = showMediaRef.current;
    if (!media || !showState) return;
    media.update(showState.slide, showState.step, showState.steps);
    media.setBlank(showState.blank !== "");
  }, [showMode, showState?.slide, showState?.step, showState?.steps, showState?.blank]);

  // One recording per slide: moving to another slide ends the last one and starts the next.
  useEffect(() => {
    const recorder = recorderRef.current;
    if (recorder && shownSlide !== undefined) void recorder.switchTo(shownSlide >= 0 ? shownSlide : null);
  }, [shownSlide]);

  /** Writes the recorded takes into the deck as narration, with each slide's time as its advance time. */
  const applyTakes = useCallback(async (engine: Engine, takes: Take[]) => {
    if (takes.length === 0) {
      announce(t("record.none"));
      setError(t("record.none"));
      return;
    }
    engine.beginRecordings();
    for (const take of takes) engine.addNarration(take.slide, new Uint8Array(await take.blob.arrayBuffer()), "m4a", take.stayMs);
    engine.endRecordings();
    announce(t("record.done", { count: takes.length }));
  }, []);

  const endShow = useCallback(async () => {
    const engine = engineRef.current;
    if (!engine || !showingRef.current) return;
    engine.endShow();
    showMediaRef.current?.stop();
    showMediaRef.current = null;
    setShowMode(null);
    setShowState(null);
    const wasReading = readingRef.current;
    setReading(false);
    setAudienceOpen(false);
    // Narration: the takes go into the deck now that the show has ended (one undo step).
    const recorder = recorderRef.current;
    recorderRef.current = null;
    if (recorder) {
      setRecording(false);
      const takes = await recorder.finish();
      await applyTakes(engine, takes);
    }
    if (inTauri) await (await WebviewWindow.getByLabel(AUDIENCE_LABEL))?.close();
    refresh();
    focusCanvas();
    if (inTauri && !wasReading && !wasFullscreen.current) await getCurrentWindow().setFullscreen(false);
  }, [refresh]);

  const showCommand = useCallback(
    (c: ShowCommand) => {
      const engine = engineRef.current;
      if (!engine || !showingRef.current) return;
      if (c.name === "end" || !engine.showCommand(c.name, c.index ?? 0)) {
        void endShow();
        return;
      }
      syncShow();
    },
    [endShow, syncShow],
  );

  // Slides set to advance by themselves (after their transition has played).
  useEffect(() => {
    // Advancing by time waits until the slide's animation clicks have played.
    // Recording is by hand: the slides advance when the speaker does.
    if (recording || !showState || showState.slide < 0 || showState.blank || showState.afterMs < 0 || showState.step < showState.steps) return;
    const delay = showState.afterMs + (showState.transition ? showState.transitionMs : 0);
    const id = window.setTimeout(() => showCommand({ name: "next" }), delay);
    return () => window.clearTimeout(id);
  }, [showState, showCommand, recording]);

  // Input forwarded from the audience window; it asks for the state once it has loaded.
  useEffect(() => {
    if (!inTauri) return;
    const a = listen<ShowCommand>("show-input", (e) => showCommand(e.payload));
    const b = listen("audience-ready", () => syncShow());
    return () => {
      void a.then((f) => f());
      void b.then((f) => f());
    };
  }, [showCommand, syncShow]);

  const run = useCallback(
    async (c: Command) => {
      const engine = engineRef.current;
      if (!engine) return;
      if (c.kind === "slideShow") {
        if (c.record) await startRecording(c.fromBeginning);
        else await startShow(c.fromBeginning, c.presenter, c.forceAudience, c.reading);
        return;
      }
      if (c.kind === "clearNarration") {
        if (engine.clearNarration((engine.status() as { slideIndex: number }).slideIndex)) refresh();
        return;
      }
      if (c.kind === "usePresenter") {
        setUsePresenter(c.on);
        return;
      }
      if (c.kind === "find") {
        findRef.current?.open(c.replace);
        return;
      }
      if (c.kind === "spellCheck") {
        setSpellOn(c.on);
        return;
      }
      if (c.kind === "toggleNotes") {
        setNotesPrefs((p) => ({ ...p, open: !p.open }));
        return;
      }
      if (c.kind === "viewMode") {
        if (showingRef.current && readingRef.current) await endShow();
        setViewMode(c.mode);
        if (c.mode === "normal") focusCanvas();
        return;
      }
      if (c.kind === "toggleRuler") {
        setViewPrefs((p) => ({ ...p, ruler: !p.ruler }));
        return;
      }
      if (c.kind === "exportVideo") {
        setVideoOpen(true);
        return;
      }
      if (c.kind === "textPane") {
        setDiagramPane((open) => c.open ?? !open);
        return;
      }
      if (c.kind === "smartArt") {
        const engine = engineRef.current;
        if (!engine) return;
        const next = engine.diagramCommand(c.command, diagramRowRef.current);
        if (next >= 0) {
          setDiagramRow(next);
          setDiagramRange(null);
          setDiagramPane(true);
          setDiagramFocus((n) => n + 1);
          refresh();
        }
        return;
      }
      if (c.kind === "formatPane") {
        setViewPrefs((p) => ({ ...p, format: c.open ?? !p.format }));
        return;
      }
      if (c.kind === "selectionPane") {
        setViewPrefs((p) => ({ ...p, pane: c.open ?? !p.pane }));
        return;
      }
      if (c.kind === "checker") {
        setViewPrefs((p) => ({ ...p, checker: c.open ?? !p.checker }));
        return;
      }
      if (c.kind === "formatPainter") {
        if (c.mode === "off") setPainter(null);
        else setPainter(engineRef.current?.copyFormat() ? c.mode : null);
        return;
      }
      if (c.kind === "showGrid") {
        setViewPrefs((p) => ({ ...p, grid: c.on }));
        return;
      }
      if (c.kind === "zoom" && viewModeRef.current === "sorter") {
        setViewPrefs((p) => ({ ...p, sorterSize: c.factor === "fit" ? SORTER_BASE : Math.max(100, Math.min(480, Math.round(p.sorterSize * c.factor))) }));
        return;
      }
      let structural = false;
      switch (c.kind) {
        case "open": {
          if (!(await confirmDiscard())) break;
          const path = await pickPptx();
          if (path) await openPath(path);
          break;
        }
        case "new":
          if (!(await confirmDiscard())) break;
          engine.newDeck();
          await documentReplaced(t("doc.defaultName"), null);
          break;
        case "save":
          await save();
          break;
        case "saveAs":
          await saveAs();
          break;
        case "exportPdf":
          await exportPdf();
          break;
        case "undo":
          announce(engine.undo() ? t("ribbon.undo") : t("app.nothingToUndo"));
          structural = true;
          await deckStyleChanged();
          break;
        case "redo":
          announce(engine.redo() ? t("ribbon.redo") : t("app.nothingToRedo"));
          structural = true;
          await deckStyleChanged();
          break;
        case "cut":
        case "copy":
          if (engine.isEditingText()) {
            const text = engine.selectedText();
            if (text) await navigator.clipboard.writeText(text);
            if (c.kind === "cut") engine.deleteTextSelection();
          } else if (engine.copyShapes() > 0) {
            await navigator.clipboard.writeText(SHAPES_MARKER).catch(() => {});
            if (c.kind === "cut") engine.deleteSelected();
          }
          break;
        case "paste": {
          const text = await navigator.clipboard.readText().catch(() => "");
          if (engine.isEditingText()) {
            if (text && text !== SHAPES_MARKER) engine.insertText(text);
          } else if ((text === SHAPES_MARKER || !text) && (engine.status() as Status).canPaste) {
            engine.pasteShapes();
          } else if (text) {
            engine.pasteTextBox(text);
          }
          break;
        }
        case "delete":
          engine.deleteSelected();
          break;
        case "duplicate":
          engine.duplicateSelected();
          break;
        case "textBox":
          engine.addTextBox();
          break;
        case "shape":
          engine.addShape(c.preset);
          break;
        case "picture": {
          const path = await pickImage();
          if (!path) break;
          const ext = path.split(".").pop() ?? "png";
          if (!engine.insertPicture(await readFile(path), ext)) setError(t("app.badImage"));
          break;
        }
        case "tableMerge":
          engine.tableMerge();
          break;
        case "tableSplit":
          engine.tableSplit(c.rows, c.cols);
          break;
        case "cellFill":
          engine.setCellFill(c.color);
          break;
        case "cellBorders":
          engine.setCellBorders(c.which, c.color, c.width);
          break;
        case "cellAnchor":
          engine.setCellAnchor(c.anchor);
          break;
        case "tableStyle":
          engine.setTableStyle(c.id);
          break;
        case "tableFlag":
          engine.setTableFlag(c.flag, c.on);
          break;
        case "link":
          engine.setLink(c.url);
          break;
        case "group":
          engine.groupShapes(c.on);
          break;
        case "lock":
          engine.setLocked(c.on);
          break;
        case "insertTable":
          engine.insertTable(c.rows, c.cols);
          break;
        case "insertSmartArt":
          engine.insertSmartArt(c.layout);
          break;
        case "table":
          engine.tableCommand(c.command);
          break;
        case "newSlide":
          engine.addSlide(c.layout ?? -1);
          structural = true;
          break;
        case "duplicateSlide":
          engine.duplicateSlide(c.index ?? (engine.status() as Status).slideIndex);
          structural = true;
          break;
        case "deleteSlide":
          engine.deleteSlide(c.index ?? (engine.status() as Status).slideIndex);
          structural = true;
          break;
        case "moveSlide":
          engine.moveSlide(c.from, c.to);
          structural = true;
          break;
        case "toggle":
          engine.toggle(c.which);
          break;
        case "charSpacing":
          engine.setCharSpacing(c.pt);
          break;
        case "clearFormatting":
          engine.clearFormatting();
          break;
        case "lineSpacing":
          engine.setLineSpacing(c.multiple);
          break;
        case "fontSpec":
          await loadFonts([c.spec.family, c.spec.eaFamily].filter((f): f is string => !!f));
          engine.setFontSpec(c.spec);
          break;
        case "paragraphSpec":
          engine.setParagraphSpec(c.spec);
          break;
        case "textAnchor":
          engine.setTextAnchor(c.anchor);
          break;
        case "applyLayout":
          engine.applyLayout(c.layout);
          break;
        case "resetSlide":
          engine.resetSlide();
          break;
        case "selectAll":
          engine.keyDown("a", false, false, true, false);
          break;
        case "setFrame":
          engine.setFrame(c.x, c.y, c.w, c.h, c.rot);
          break;
        case "rotate":
          engine.rotateSelection(c.degrees);
          break;
        case "flip":
          engine.flipSelection(c.horizontal);
          break;
        case "changeShape":
          engine.changeShape(c.preset);
          break;
        case "shadow":
          engine.setShadow(c.preset);
          break;
        case "cropMode":
          engine.setCropMode(c.on);
          break;
        case "crop":
          engine.setCrop(c.left, c.top, c.right, c.bottom);
          break;
        case "resetPicture":
          engine.resetPicture();
          break;
        case "drawTool":
          engine.setDrawTool(c.tool);
          break;
        case "headerFooter":
          engine.setHeaderFooter(c.date, c.dateText, c.slideNumber, c.footer, c.footerText, c.skipTitle, c.all);
          structural = c.all;
          break;
        case "insertSymbol":
          engine.insertSymbol(c.text);
          break;
        case "showGuides":
          engine.setShowGuides(c.on);
          break;
        case "altText":
          engine.setAltText(c.text, c.decorative);
          break;
        case "snapToGrid":
          engine.setSnapToGrid(c.on);
          break;
        case "gridSpacing":
          engine.setGridSpacing(c.emu);
          break;
        case "addGuide":
          engine.addGuide(c.vertical);
          break;
        case "background":
          if (c.fill === "solid") engine.setBackgroundSolid(c.color);
          else engine.setBackgroundGradient(c.start, c.end, c.radial, c.angle);
          break;
        case "backgroundPicture": {
          const path = await pickImage();
          if (!path) break;
          const ext = path.split(".").pop() ?? "png";
          if (!engine.setBackgroundPicture(await readFile(path), ext)) setError(t("app.badImage"));
          break;
        }
        case "resetBackground":
          engine.resetBackground();
          break;
        case "hideBackgroundGraphics":
          engine.setHideBackgroundGraphics(c.on);
          break;
        case "backgroundFill":
          engine.setBackgroundFill(c.spec);
          break;
        case "backgroundToAll":
          engine.applyBackgroundToAll();
          structural = true;
          break;
        case "colorScheme":
        case "fontScheme":
        case "slideSize":
          try {
            if (c.kind === "colorScheme") engine.setColorScheme(c.name);
            else if (c.kind === "fontScheme") engine.setFontScheme(c.name);
            else engine.setSlideSize(c.width, c.height, c.fit);
          } catch (e) {
            setError(t("app.designFailed", { error: String(e) }));
          }
          structural = true;
          await deckStyleChanged();
          break;
        case "fontSize":
          engine.setFontSize(c.size);
          break;
        case "fontStep":
          engine.keyDown(c.up ? "]" : "[", false, false, true, false);
          break;
        case "fontFamily":
          await loadFonts([c.family]);
          engine.setFontFamily(c.family);
          break;
        case "textColor":
          engine.setTextColor(c.color);
          break;
        case "align":
          engine.setAlign(c.align);
          break;
        case "numbering":
          engine.toggleNumbering(c.scheme);
          break;
        case "bullets":
          engine.toggleBullets();
          break;
        case "level":
          engine.changeLevel(c.delta);
          break;
        case "fill":
          engine.setFill(c.color);
          break;
        case "fillSpec":
          engine.setFillSpec(c.spec);
          break;
        case "lineSpec":
          engine.setLineSpec(c.spec);
          break;
        case "shadowSpec":
          engine.setShadowSpec(c.spec);
          break;
        case "textBoxSpec":
          engine.setTextBoxSpec(c.spec);
          break;
        case "lockAspect":
          engine.setLockAspect(c.on);
          break;
        case "lineColor":
          engine.setLineColor(c.color);
          break;
        case "lineWidth":
          engine.setLineWidth(c.width);
          break;
        case "reorder":
          engine.reorder(c.order);
          break;
        case "alignShapes":
          engine.alignShapes(c.to);
          break;
        case "zoom":
          engine.setZoom(c.factor === "fit" ? 1 : engine.zoom() * c.factor);
          break;
        case "setTransition":
          engine.setTransition(c.id, c.option, c.durationMs, c.onClick, c.afterMs, c.all);
          structural = c.all;
          break;
        case "previewTransition":
          engine.previewTransition();
          break;
        case "previewAnimations":
          engine.previewAnimations();
          break;
        case "addAnimation":
          engine.addAnimation(c.class, c.id, c.subtype ?? -1);
          break;
        case "changeAnimation":
          engine.changeAnimation(c.index, c.class, c.id, c.subtype ?? -1);
          break;
        case "removeAnimation":
          engine.removeAnimation(c.index);
          break;
        case "moveAnimation":
          engine.moveAnimation(c.from, c.to);
          break;
        case "setAnimationTrigger":
          engine.setAnimationTrigger(c.index, c.trigger);
          break;
        case "setAnimationTiming":
          engine.setAnimationTiming(c.index, c.delayMs ?? -1, c.durMs ?? -1);
          break;
        case "selectAnimation":
          engine.selectAnimation(c.index);
          break;
      }
      if (structural) scheduleThumbs(true);
      refresh();
      // In the slide sorter, keys belong to the sorter, not the hidden canvas.
      if (viewModeRef.current === "sorter") document.querySelector<HTMLElement>(".slide-sorter")?.focus({ preventScroll: true });
      else focusCanvas();
    },
    [confirmDiscard, deckStyleChanged, documentReplaced, exportPdf, loadFonts, openPath, refresh, save, saveAs, scheduleThumbs, startShow, startRecording, endShow],
  );

  // Native menu items are forwarded here by the backend.
  useEffect(() => {
    const map: Record<string, Command> = {
      open: { kind: "open" },
      new: { kind: "new" },
      save: { kind: "save" },
      save_as: { kind: "saveAs" },
      export_pdf: { kind: "exportPdf" },
      export_video: { kind: "exportVideo" },
      undo: { kind: "undo" },
      redo: { kind: "redo" },
      new_slide: { kind: "newSlide" },
      duplicate_slide: { kind: "duplicateSlide" },
      insert_picture: { kind: "picture" },
      insert_text_box: { kind: "textBox" },
      insert_rect: { kind: "shape", preset: "rect" },
      insert_ellipse: { kind: "shape", preset: "ellipse" },
      group: { kind: "group", on: true },
      ungroup: { kind: "group", on: false },
      zoom_in: { kind: "zoom", factor: 1.25 },
      zoom_out: { kind: "zoom", factor: 0.8 },
      zoom_fit: { kind: "zoom", factor: "fit" },
      show_from_beginning: { kind: "slideShow", fromBeginning: true },
      show_from_current: { kind: "slideShow", fromBeginning: false },
      show_presenter: { kind: "slideShow", fromBeginning: false, presenter: true },
      toggle_notes: { kind: "toggleNotes" },
    };
    const unlisten = onMenu((id) => {
      const engine = engineRef.current;
      // Editing commands must not touch the deck while it is being presented.
      if (showingRef.current) return;
      // The slide sorter has no shapes to edit: inserting goes back to the normal view,
      // and the other shape commands do nothing there.
      if (viewModeRef.current === "sorter") {
        if (id.startsWith("insert_") || id === "find" || id === "replace") setViewMode("normal");
        else if (["cut", "copy", "paste", "delete", "duplicate", "select_all", "group", "ungroup"].includes(id)) return;
      }
      if (id === "preferences") {
        void openPreferences();
        return;
      }
      if (id === "about") {
        void openAbout();
        return;
      }
      if (id === "find" || id === "replace") {
        findRef.current?.open(id === "replace");
        return;
      }
      if (id === "find_next" || id === "find_previous") {
        findRef.current?.step(id === "find_previous");
        return;
      }
      // In a text field (find bar, ribbon inputs), undo/redo/select all act on the field.
      const field = document.activeElement;
      if (field instanceof HTMLInputElement && (id === "undo" || id === "redo" || id === "select_all")) {
        if (id === "select_all") field.select();
        else document.execCommand(id);
        return;
      }
      // In the notes pane, undo/redo/select all act on its text (WebKit's own undo history).
      const notes = notesFocused();
      if (notes && (id === "undo" || id === "redo")) {
        document.execCommand(id);
        return;
      }
      if (notes && id === "select_all") {
        notes.select();
        return;
      }
      if (id === "insert_link") {
        setLinkRequest((n) => n + 1);
      } else if (id === "select_all" && engine) {
        engine.keyDown("a", false, false, true, false);
        refresh();
      } else if (map[id]) {
        void run(map[id]);
      }
    });
    return () => void unlisten.then((f) => f());
  }, [run, refresh]);

  // F5 / Shift+F5 start the show, as in PowerPoint for Windows; F6 cycles the panes.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (showingRef.current) return;
      // ⌘1 opens the Format pane, as in PowerPoint for Mac.
      if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key === "1") {
        e.preventDefault();
        void run({ kind: "formatPane" });
        return;
      }
      if (e.key === "F6") {
        // F6 / Shift+F6 move between the panes, as in PowerPoint.
        e.preventDefault();
        cyclePanes(e.shiftKey);
        return;
      }
      if (e.key !== "F5") return;
      e.preventDefault();
      void run({ kind: "slideShow", fromBeginning: !e.shiftKey });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [run]);

  // Files opened from Finder while running replace the current deck, like File > Open.
  useEffect(() => {
    const unlisten = onOpenFiles(async ([path]) => {
      if (showingRef.current || !path) return;
      if (await confirmDiscard()) await openPath(path);
    });
    return () => void unlisten.then((f) => f());
  }, [confirmDiscard, openPath]);

  // Unsaved changes: window title marker and a prompt before closing.
  useEffect(() => {
    if (!inTauri) return;
    void getCurrentWindow().setTitle(`${dirty ? "● " : ""}${doc.name} - ${APP_NAME}`);
  }, [dirty, doc.name]);
  useEffect(() => {
    if (!inTauri) return;
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (!dirtyRef.current) return;
      event.preventDefault();
      if (await confirmDiscard()) {
        // Saved or discarded on purpose: nothing to recover.
        await recoveryClear(sessionId.current);
        await getCurrentWindow().destroy();
      }
    });
    return () => void unlisten.then((f) => f());
  }, [confirmDiscard]);

  /** Offers the newest autosave left by a session that crashed; true when it was opened. */
  const offerRecovery = useCallback(
    async (engine: Engine) => {
      const [newest] = await recoveryList().catch(() => []);
      if (!newest) return false;
      const answer = await askRecover(newest);
      if (answer === "discard") await recoveryClear(newest.id);
      if (answer !== "recover") return false;
      try {
        await engine.loadPptx(await recoveryRead(newest.id));
        await documentReplaced(newest.name, newest.path, newest.id);
        return true;
      } catch (e) {
        await showError(t("app.recoverFailed", { name: newest.name, error: String(e) }));
        return false;
      }
    },
    [documentReplaced],
  );

  const onReady = useCallback(
    async (engine: Engine) => {
      engineRef.current = engine;
      engine.setFallbackFonts([...FALLBACK_FAMILIES, EMOJI_FAMILY]);
      engine.setWorkspaceColor(cssColor("--workspace"));
      engine.setShowGrid(loadViewPrefs().grid);
      if (!inTauri) setError(t("app.inBrowser"));
      void listFonts().then(setFonts);
      await loadFonts(FALLBACK_FAMILIES);
      if (!(await offerRecovery(engine))) {
        const [first] = await startupFiles();
        if (first) await openPath(first);
        else await documentReplaced(t("doc.defaultName"), null);
      }
      void startAutomation({ engine: () => engineRef.current, openPath, refresh, run, documentReplaced });
    },
    [documentReplaced, loadFonts, offerRecovery, openPath, refresh, run],
  );

  return (
    <div className="app">
      <header className="title-bar" data-tauri-drag-region>
        <span className="app-name" data-tauri-drag-region>
          {APP_NAME}
        </span>
        <span className="doc-name" data-tauri-drag-region>
          {doc.name}
          {dirty && <span className="dirty-dot" title={t("title.unsaved")} />}
        </span>
      </header>
      <Ribbon hasNarration={hasNarration} tabRequest={tabRequest} painter={painter} status={status} fonts={fonts} layoutNames={layoutNames} layoutThumbs={layoutThumbs} loadLayoutThumbs={loadLayoutThumbs} run={run} linkRequest={linkRequest} tableStyles={tableStyles} notesOpen={notesPrefs.open} usePresenter={usePresenter} spellOn={spellOn} animPresets={animPresets} onTabChange={setRibbonTab} shapeIcons={shapeIcons} loadHeaderFooter={() => (engineRef.current?.headerFooter() as HeaderFooterInfo | undefined) ?? null}
        theme={theme}
        colorSchemes={schemes.colors}
        fontSchemes={schemes.fonts}
        slideSize={slideSize}
        loadBackground={() => (engineRef.current?.background() as BackgroundInfo | undefined) ?? null}
        viewMode={viewMode}
        rulerOn={viewPrefs.ruler}
        paneOpen={viewPrefs.pane}
        checkerOpen={viewPrefs.checker}
        textPaneOpen={diagramShown}
        loadAltText={() => (engineRef.current?.altText() as AltTextInfo | null | undefined) ?? null}
      />
      <main className={["workspace", viewMode === "sorter" && !showing && "sorting", (viewPrefs.pane || viewPrefs.checker || viewPrefs.format || diagramShown) && !showing && "with-pane", viewPrefs.format && !showing && "with-format"].filter(Boolean).join(" ")}>
        {viewMode === "sorter" && !showing && (
          <SlideSorter
            count={status.slideCount}
            current={status.slideIndex}
            thumbs={thumbs}
            transitions={status.transitionSlides}
            size={viewPrefs.sorterSize}
            labels={slideLabels}
            onSelect={(i) => {
              engineRef.current?.setSlide(i);
              refresh();
            }}
            onOpen={(i) => {
              engineRef.current?.setSlide(i);
              setViewMode("normal");
              refresh();
              focusCanvas();
            }}
            run={run}
          />
        )}
        <SlidePane
          count={status.slideCount}
          transitions={status.transitionSlides}
          current={status.slideIndex}
          thumbs={thumbs}
          labels={slideLabels}
          onSelect={(i) => {
            engineRef.current?.setSlide(i);
            refresh();
          }}
          run={run}
        />
        <section className={showing ? (reading ? "editor-area showing reading" : "editor-area showing") : "editor-area"}>
          <div className={viewPrefs.ruler && !showing ? "canvas-area with-ruler" : "canvas-area"} onContextMenu={onCanvasContextMenu}>
            <CanvasHost onReady={onReady} onError={setError} onChange={refresh} scroll={status.scroll} painter={painter} onPainterDone={() => setPainter(null)} />
            {viewPrefs.ruler && !showing && <Rulers engine={engineRef.current} slideSize={slideSize} />}
            {!showing && viewMode === "normal" && <A11yMirror engine={engineRef.current} version={`${status.revision}|${status.slideIndex}|${status.selectionCount}|${status.selectionName}|${JSON.stringify(status.frame ?? null)}|${status.zoom}|${viewPrefs.ruler}|${viewPrefs.pane}|${notesPrefs.open}`} />}
            {!showing && <FindBar ref={findRef} engine={engineRef.current} onChange={findChanged} showNotesMatch={showNotesMatch} />}
            {ribbonTab === "動畫" && !showing && <AnimationBadges animations={status.animations ?? []} />}
            {showMode === "slides" && <SlideShow state={showState} command={showCommand} recording={recording} onToggleView={audienceOpen ? undefined : () => setShowMode("presenter")} />}
            {showMode === "slides" && reading && <ReadingBar state={showState} command={showCommand} />}
            {showMode === "presenter" && engineRef.current && showState && (
              <PresenterView
                engine={engineRef.current}
                state={showState}
                command={showCommand}
                timer={showTimer.current}
                onToggleView={audienceOpen ? undefined : () => setShowMode("slides")}
              />
            )}
            {spellMenu && (
              <SpellMenu
                {...spellMenu}
                onPick={(word) => {
                  engineRef.current?.replaceSpelling(spellMenu.hit.key, spellMenu.hit.start, spellMenu.hit.length, word);
                  refresh();
                }}
                onIgnore={() => void invoke("spell_ignore", { word: spellMenu.hit.word }).then(recheckSpelling)}
                onLearn={() => void invoke("spell_learn", { word: spellMenu.hit.word }).then(recheckSpelling)}
                onClose={() => setSpellMenu(null)}
              />
            )}
            {ctxMenu && (
              <ContextMenu
                x={ctxMenu.x}
                y={ctxMenu.y}
                label={t("ctx.menu")}
                items={buildContextMenu(ctxMenu.kind, {
                  status,
                  run,
                  layoutNames,
                  openTab: (tab) => setTabRequest((q) => ({ tab, n: q.n + 1 })),
                  openFormatPane: () => void run({ kind: "formatPane", open: true }),
                  openLink: () => setLinkRequest((n) => n + 1),
                  openFont: () => setTextDialog("font"),
                  openParagraph: () => setTextDialog("paragraph"),
                  openAltText: () => setAltPopup({ x: ctxMenu.x, y: ctxMenu.y }),
                  endEditing: () => {
                    engineRef.current?.keyDown("Escape", false, false, false, false);
                    refresh();
                  },
                })}
                onClose={() => setCtxMenu(null)}
              />
            )}
            {altPopup && (
              <div className="ctx-popup" style={{ left: altPopup.x, top: altPopup.y }}>
                <AltTextForm
                  initial={(engineRef.current?.altText() as AltTextInfo | null | undefined) ?? { text: "", decorative: false }}
                  run={run}
                  close={() => setAltPopup(null)}
                />
              </div>
            )}
            {error && (
              <div className="banner" role="alert">
                {error}
                <button onClick={() => setError(null)} aria-label={t("common.close")}>
                  ×
                </button>
              </div>
            )}
          </div>
          {notesPrefs.open && !showing && (
            <NotesPane
              engine={engineRef.current}
              slideIndex={status.slideIndex}
              revision={status.revision}
              spellCheck={spellOn}
              height={notesPrefs.height}
              onResize={(height) => setNotesPrefs((p) => ({ ...p, height }))}
              onChange={refresh}
            />
          )}
        </section>
        {(viewPrefs.pane || viewPrefs.checker || viewPrefs.format || diagramShown) && !showing && (
          <div className="side-panels">
            {diagramShown && (
              <DiagramPane
                engine={engineRef.current}
                version={`${status.revision}|${status.slideIndex}|${status.selectionCount}|${status.selectionName}`}
                row={diagramRow}
                onRow={setDiagramRow}
                focusRequest={diagramFocus}
                focusRange={diagramRange}
                onChange={refresh}
                onClose={() => {
                  setDiagramPane(false);
                  focusCanvas();
                }}
              />
            )}
            {viewPrefs.format && (
              <FormatPane
                status={status}
                engine={engineRef.current}
                version={`${status.revision}|${status.slideIndex}|${status.selectionCount}|${status.selectionName}`}
                run={run}
                onClose={() => setViewPrefs((p) => ({ ...p, format: false }))}
              />
            )}
            {viewPrefs.checker && <CheckerPane engine={engineRef.current} issues={a11yIssues} run={run} onChange={refresh} onClose={() => setViewPrefs((p) => ({ ...p, checker: false }))} />}
            {viewPrefs.pane && (
              <SelectionPane
                engine={engineRef.current}
                version={`${status.revision}|${status.slideIndex}|${status.selectionCount}|${status.selectionName}|${JSON.stringify(status.frame ?? null)}`}
                onChange={refresh}
                onClose={() => setViewPrefs((p) => ({ ...p, pane: false }))}
              />
            )}
          </div>
        )}
      </main>
      {textDialog === "font" && <FontDialog status={status} fonts={fonts} run={run} onClose={() => { setTextDialog(null); focusCanvas(); }} />}
      {textDialog === "paragraph" && <ParagraphDialog status={status} run={run} onClose={() => { setTextDialog(null); focusCanvas(); }} />}
      {zoomOpen && <ZoomDialog percent={status.zoomPercent} relative={status.zoom} fit={status.zoomFit} run={run} onClose={() => { setZoomOpen(false); focusCanvas(); }} />}
      {videoOpen && <VideoDialog engine={engineRef.current} name={doc.name} onClose={() => setVideoOpen(false)} />}
      <StatusBar status={status} fontsLoading={fontsLoading} notesOpen={notesPrefs.open} viewMode={reading ? "reading" : viewMode} hasNarration={hasNarration} sorterZoom={viewPrefs.sorterSize / SORTER_BASE} a11yIssues={a11yIssues.length} run={run} onZoomDialog={() => setZoomOpen(true)} />
    </div>
  );
}
