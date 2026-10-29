import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { App as Engine } from "./wasm/ppt_wasm";
import { CanvasHost, focusCanvas } from "./components/CanvasHost";
import { Ribbon } from "./components/Ribbon";
import { SlidePane } from "./components/SlidePane";
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
import { SHAPES_MARKER, type Command } from "./lib/commands";
import { EMOJI_FAMILY, ensureFonts, FALLBACK_FAMILIES } from "./lib/fonts";
import { EMPTY_STATUS, type Status } from "./lib/status";

const THUMB_WIDTH = 320;

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

  const run = useCallback(
    async (c: Command) => {
      const engine = engineRef.current;
      if (!engine) return;
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
      }
      if (structural) scheduleThumbs(true);
      refresh();
      focusCanvas();
    },
    [confirmDiscard, documentReplaced, loadFonts, openPath, refresh, save, saveAs, scheduleThumbs],
  );

  // Native menu items are forwarded here by the backend.
  useEffect(() => {
    const map: Record<string, Command> = {
      open: { kind: "open" },
      new: { kind: "new" },
      save: { kind: "save" },
      save_as: { kind: "saveAs" },
      undo: { kind: "undo" },
      redo: { kind: "redo" },
      new_slide: { kind: "newSlide" },
      duplicate_slide: { kind: "duplicateSlide" },
      insert_picture: { kind: "picture" },
      insert_text_box: { kind: "textBox" },
      insert_rect: { kind: "shape", preset: "rect" },
      insert_ellipse: { kind: "shape", preset: "ellipse" },
      zoom_in: { kind: "zoom", factor: 1.25 },
      zoom_out: { kind: "zoom", factor: 0.8 },
      zoom_fit: { kind: "zoom", factor: "fit" },
    };
    const unlisten = onMenu((id) => {
      const engine = engineRef.current;
      if (id === "select_all" && engine) {
        engine.keyDown("a", false, false, true, false);
        refresh();
      } else if (map[id]) {
        void run(map[id]);
      }
    });
    return () => void unlisten.then((f) => f());
  }, [run, refresh]);

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
      <Ribbon status={status} fonts={fonts} layoutNames={layoutNames} layoutThumbs={layoutThumbs} loadLayoutThumbs={loadLayoutThumbs} run={run} />
      <main className="workspace">
        <SlidePane
          count={status.slideCount}
          current={status.slideIndex}
          thumbs={thumbs}
          onSelect={(i) => {
            engineRef.current?.setSlide(i);
            refresh();
          }}
          run={run}
        />
        <section className="editor-area">
          <CanvasHost onReady={onReady} onError={setError} onChange={refresh} />
          {error && (
            <div className="banner" role="alert">
              {error}
              <button onClick={() => setError(null)} aria-label="關閉">
                ×
              </button>
            </div>
          )}
        </section>
      </main>
      <StatusBar status={status} fontsLoading={fontsLoading} run={run} />
    </div>
  );
}
