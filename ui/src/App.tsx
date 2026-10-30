import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { availableMonitors, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import type { App as Engine } from "./wasm/ppt_wasm";
import { CanvasHost, focusCanvas } from "./components/CanvasHost";
import { Ribbon } from "./components/Ribbon";
import { SlidePane } from "./components/SlidePane";
import { SlideShow } from "./components/SlideShow";
import { PresenterView, type ShowTimer } from "./components/PresenterView";
import { AUDIENCE_LABEL, type ShowCommand, type ShowState } from "./lib/show";
import { MIN_NOTES_HEIGHT, NotesPane, notesFocused } from "./components/NotesPane";
import { StatusBar } from "./components/StatusBar";
import {
  askSaveChanges,
  inTauri,
  listFonts,
  onMenu,
  pickImage,
  pickPptx,
  pickSavePath,
  readFile,
  showError,
  startupFiles,
  writeFile,
} from "./lib/backend";
import { startAutomation } from "./lib/automation";
import { SHAPES_MARKER, type Command, type TableStyleInfo } from "./lib/commands";
import { EMOJI_FAMILY, ensureFonts, FALLBACK_FAMILIES } from "./lib/fonts";
import { EMPTY_STATUS, type AnimationPreset, type Status } from "./lib/status";
import { AnimationBadges } from "./components/AnimationTab";
import { FindBar, type FindBarHandle } from "./components/FindBar";
import { SpellMenu, type SpellHit } from "./components/SpellMenu";

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

interface Doc {
  name: string;
  path: string | null;
  savedRevision: number;
}

export function App() {
  const engineRef = useRef<Engine | null>(null);
  const [status, setStatus] = useState<Status>(EMPTY_STATUS);
  const [doc, setDoc] = useState<Doc>({ name: "簡報1", path: null, savedRevision: 0 });
  const [error, setError] = useState<string | null>(null);
  const [fontsLoading, setFontsLoading] = useState(false);
  const [fonts, setFonts] = useState<string[]>([]);
  const [layoutNames, setLayoutNames] = useState<string[]>([]);
  const [layoutThumbs, setLayoutThumbs] = useState<(ImageData | undefined)[]>([]);
  const [thumbs, setThumbs] = useState<Map<number, ImageData>>(new Map());
  const [linkRequest, setLinkRequest] = useState(0);
  const [tableStyles, setTableStyles] = useState<TableStyleInfo[]>([]);
  const [animPresets, setAnimPresets] = useState<AnimationPreset[]>([]);
  const [ribbonTab, setRibbonTab] = useState("");
  const findRef = useRef<FindBarHandle>(null);
  /** Slide show in progress: the slides full screen, or the presenter view. */
  const [showMode, setShowMode] = useState<"slides" | "presenter" | null>(null);
  const showing = showMode !== null;
  const [showState, setShowState] = useState<ShowState | null>(null);
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

  // --- Thumbnails -------------------------------------------------------------
  const thumbState = useRef({ revision: -1, count: -1, pendingAll: false, timer: 0, running: false });
  const renderThumbs = useCallback(async (indices: number[]) => {
    const engine = engineRef.current;
    if (!engine) return;
    for (const i of indices) {
      try {
        const t = (await engine.renderThumbnail(i, THUMB_WIDTH, false)) as { width: number; height: number; pixels: Uint8Array };
        const image = new ImageData(new Uint8ClampedArray(t.pixels), t.width, t.height);
        setThumbs((prev) => new Map(prev).set(i, image));
      } catch {
        // The slide may have been deleted while rendering.
      }
    }
  }, []);

  /** Re-renders the current slide's thumbnail, or all of them, shortly after edits settle. */
  const scheduleThumbs = useCallback(
    (all: boolean) => {
      const st = thumbState.current;
      st.pendingAll ||= all;
      window.clearTimeout(st.timer);
      st.timer = window.setTimeout(async () => {
        const engine = engineRef.current;
        if (!engine) return;
        const s = engine.status() as Status;
        const everything = st.pendingAll || s.slideCount !== st.count;
        st.pendingAll = false;
        st.count = s.slideCount;
        st.revision = s.revision;
        if (everything) {
          setThumbs((prev) => new Map([...prev].filter(([i]) => i < s.slideCount)));
          await renderThumbs(Array.from({ length: s.slideCount }, (_, i) => i));
        } else {
          await renderThumbs([s.slideIndex]);
        }
      }, all ? 30 : 250);
    },
    [renderThumbs],
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

  /** Right click on a misspelled word: its corrections. */
  const onCanvasContextMenu = useCallback(
    (e: React.MouseEvent) => {
      const engine = engineRef.current;
      const canvas = document.querySelector(".canvas-host canvas");
      if (!engine || !canvas || !spellOn || showingRef.current) return;
      const r = canvas.getBoundingClientRect();
      const hit = engine.spellingAt(e.clientX - r.left, e.clientY - r.top) as SpellHit | null;
      if (!hit) return;
      e.preventDefault();
      const area = e.currentTarget.getBoundingClientRect();
      const at = { x: e.clientX - area.left, y: e.clientY - area.top, hit };
      setSpellMenu({ ...at, guesses: null });
      void invoke<string[]>("spell_guesses", { text: hit.text, start: hit.start, len: hit.length })
        .catch(() => [])
        .then((guesses) => setSpellMenu((m) => (m?.hit === hit ? { ...m, guesses } : m)));
    },
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
      try {
        await ensureFonts(engine, families, refresh);
      } catch (e) {
        console.error("font loading failed", e);
      } finally {
        setFontsLoading(false);
        scheduleThumbs(true);
      }
    },
    [refresh, scheduleThumbs],
  );

  /** Called after a document is loaded or created. */
  const documentReplaced = useCallback(
    async (name: string, path: string | null) => {
      const engine = engineRef.current;
      if (!engine) return;
      setDoc({ name, path, savedRevision: engine.revision() });
      setLayoutNames(engine.layoutNames() as string[]);
      setTableStyles(engine.tableStyles() as TableStyleInfo[]);
      setAnimPresets(engine.animationPresets() as AnimationPreset[]);
      setLayoutThumbs([]);
      setThumbs(new Map());
      refresh();
      scheduleThumbs(true);
      await loadFonts(engine.fontFamilies() as string[]);
    },
    [loadFonts, refresh, scheduleThumbs],
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
      await showError(`無法儲存檔案：${String(e)}`);
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
      await showError(`無法匯出 PDF：${String(e)}`);
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
        engine.loadPptx(await readFile(path));
        setError(null);
        await documentReplaced(path.split("/").pop()?.replace(/\.pptx$/i, "") ?? path, path);
      } catch (e) {
        setError(`無法開啟檔案：${String(e)}`);
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
      title: "投影片放映",
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
    async (fromBeginning: boolean, presenter?: boolean, forceAudience = false) => {
      const engine = engineRef.current;
      if (!engine || showingRef.current || !engine.startShow(fromBeginning)) return;
      showTimer.current = { elapsed: 0, since: Date.now() };
      syncShow();
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
    },
    [openAudience, syncShow],
  );

  const endShow = useCallback(async () => {
    const engine = engineRef.current;
    if (!engine || !showingRef.current) return;
    engine.endShow();
    setShowMode(null);
    setShowState(null);
    setAudienceOpen(false);
    if (inTauri) await (await WebviewWindow.getByLabel(AUDIENCE_LABEL))?.close();
    refresh();
    focusCanvas();
    if (inTauri && !wasFullscreen.current) await getCurrentWindow().setFullscreen(false);
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
    if (!showState || showState.slide < 0 || showState.blank || showState.afterMs < 0 || showState.step < showState.steps) return;
    const delay = showState.afterMs + (showState.transition ? showState.transitionMs : 0);
    const id = window.setTimeout(() => showCommand({ name: "next" }), delay);
    return () => window.clearTimeout(id);
  }, [showState, showCommand]);

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
        await startShow(c.fromBeginning, c.presenter, c.forceAudience);
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
          await documentReplaced("簡報1", null);
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
          engine.undo();
          structural = true;
          break;
        case "redo":
          engine.redo();
          structural = true;
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
          if (!engine.insertPicture(await readFile(path), ext)) setError("無法讀取這個圖片檔。");
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
        case "insertTable":
          engine.insertTable(c.rows, c.cols);
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
      focusCanvas();
    },
    [confirmDiscard, documentReplaced, exportPdf, loadFonts, openPath, refresh, save, saveAs, scheduleThumbs, startShow],
  );

  // Native menu items are forwarded here by the backend.
  useEffect(() => {
    const map: Record<string, Command> = {
      open: { kind: "open" },
      new: { kind: "new" },
      save: { kind: "save" },
      save_as: { kind: "saveAs" },
      export_pdf: { kind: "exportPdf" },
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

  // F5 / Shift+F5 start the show, as in PowerPoint for Windows.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "F5" || showingRef.current) return;
      e.preventDefault();
      void run({ kind: "slideShow", fromBeginning: !e.shiftKey });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [run]);

  // Unsaved changes: window title marker and a prompt before closing.
  useEffect(() => {
    if (!inTauri) return;
    void getCurrentWindow().setTitle(`${dirty ? "● " : ""}${doc.name} - ww-ppt`);
  }, [dirty, doc.name]);
  useEffect(() => {
    if (!inTauri) return;
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (!dirtyRef.current) return;
      event.preventDefault();
      if (await confirmDiscard()) await getCurrentWindow().destroy();
    });
    return () => void unlisten.then((f) => f());
  }, [confirmDiscard]);

  const onReady = useCallback(
    async (engine: Engine) => {
      engineRef.current = engine;
      engine.setFallbackFonts([...FALLBACK_FAMILIES, EMOJI_FAMILY]);
      if (!inTauri) setError("目前在瀏覽器中執行：沒有原生字型與檔案存取，請用 npm run tauri dev 啟動。");
      void listFonts().then(setFonts);
      await loadFonts(FALLBACK_FAMILIES);
      const [first] = await startupFiles();
      if (first) await openPath(first);
      else await documentReplaced("簡報1", null);
      void startAutomation({ engine: () => engineRef.current, openPath, refresh, run });
    },
    [documentReplaced, loadFonts, openPath, refresh, run],
  );

  return (
    <div className="app">
      <header className="title-bar" data-tauri-drag-region>
        <span className="app-name" data-tauri-drag-region>
          ww-ppt
        </span>
        <span className="doc-name" data-tauri-drag-region>
          {doc.name}
          {dirty && <span className="dirty-dot" title="有未儲存的變更" />}
        </span>
      </header>
      <Ribbon status={status} fonts={fonts} layoutNames={layoutNames} layoutThumbs={layoutThumbs} loadLayoutThumbs={loadLayoutThumbs} run={run} linkRequest={linkRequest} tableStyles={tableStyles} notesOpen={notesPrefs.open} usePresenter={usePresenter} spellOn={spellOn} animPresets={animPresets} onTabChange={setRibbonTab} />
      <main className="workspace">
        <SlidePane
          count={status.slideCount}
          transitions={status.transitionSlides}
          current={status.slideIndex}
          thumbs={thumbs}
          onSelect={(i) => {
            engineRef.current?.setSlide(i);
            refresh();
          }}
          run={run}
        />
        <section className={showing ? "editor-area showing" : "editor-area"}>
          <div className="canvas-area" onContextMenu={onCanvasContextMenu}>
            <CanvasHost onReady={onReady} onError={setError} onChange={refresh} />
            {!showing && <FindBar ref={findRef} engine={engineRef.current} onChange={findChanged} showNotesMatch={showNotesMatch} />}
            {ribbonTab === "動畫" && !showing && <AnimationBadges animations={status.animations ?? []} />}
            {showMode === "slides" && <SlideShow state={showState} command={showCommand} onToggleView={audienceOpen ? undefined : () => setShowMode("presenter")} />}
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
            {error && (
              <div className="banner" role="alert">
                {error}
                <button onClick={() => setError(null)} aria-label="關閉">
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
      </main>
      <StatusBar status={status} fontsLoading={fontsLoading} notesOpen={notesPrefs.open} run={run} />
    </div>
  );
}
