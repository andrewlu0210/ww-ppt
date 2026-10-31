// Thin wrappers over the native Tauri commands and dialogs.
import { locale, t } from "../i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { message, open, save } from "@tauri-apps/plugin-dialog";

export const inTauri = "__TAURI_INTERNALS__" in window;

export interface ResolvedFont {
  family: string;
  paths: string[];
}

export async function readFile(path: string): Promise<Uint8Array> {
  const buf = await invoke<ArrayBuffer>("read_file", { path });
  return new Uint8Array(buf);
}

/** A font file, with large collections trimmed to the regular and bold weights. */
export async function readFont(path: string): Promise<Uint8Array> {
  const buf = await invoke<ArrayBuffer>("read_font", { path });
  return new Uint8Array(buf);
}

/** Writes raw bytes; the path travels in a header so the body stays binary. */
export function writeFile(path: string, bytes: Uint8Array): Promise<void> {
  return invoke("write_file", bytes, { headers: { path: encodeURIComponent(path) } });
}

export function resolveFont(family: string): Promise<ResolvedFont | null> {
  return invoke<ResolvedFont | null>("resolve_font", { family });
}

export function listFonts(): Promise<string[]> {
  return inTauri ? invoke<string[]>("list_fonts") : Promise.resolve([]);
}

export function startupFiles(): Promise<string[]> {
  return inTauri ? invoke<string[]>("startup_files") : Promise.resolve([]);
}

export async function pickPptx(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, filters: [{ name: t("file.pptxFilter"), extensions: ["pptx"] }] });
  return typeof picked === "string" ? picked : null;
}

export async function pickImage(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, filters: [{ name: t("file.imageFilter"), extensions: ["png", "jpg", "jpeg", "gif"] }] });
  return typeof picked === "string" ? picked : null;
}

const SAVE_FILTERS = { pptx: t("file.pptxFilter"), pdf: t("file.pdfFilter"), mp4: t("file.mp4Filter") };

export async function pickSavePath(defaultName: string, ext: keyof typeof SAVE_FILTERS = "pptx"): Promise<string | null> {
  const path = await save({ defaultPath: `${defaultName}.${ext}`, filters: [{ name: SAVE_FILTERS[ext], extensions: [ext] }] });
  if (!path) return null;
  return path.toLowerCase().endsWith(`.${ext}`) ? path : `${path}.${ext}`;
}

/** The app's display name in the interface language ("旺-Point" in Chinese). */
export const APP_NAME = t("appName");

/** "save" | "discard" | "cancel" for a document with unsaved changes. */
export async function askSaveChanges(name: string): Promise<"save" | "discard" | "cancel"> {
  const [save, dontSave] = [t("save.save"), t("save.dontSave")];
  const result = await message(t("save.ask", { name }), {
    title: APP_NAME,
    kind: "warning",
    buttons: { yes: save, no: dontSave, cancel: t("save.cancel") },
  });
  return result === save ? "save" : result === dontSave ? "discard" : "cancel";
}

export function showError(text: string): Promise<unknown> {
  return message(text, { title: APP_NAME, kind: "error" });
}

/** Files macOS asks to open while the app is running (Finder, Open With, Dock). */
export function onOpenFiles(handler: (paths: string[]) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<string[]>("open-files", (e) => handler(e.payload));
}

export function onMenu(handler: (id: string) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<string>("menu", (e) => handler(e.payload));
}

export interface VideoSettings {
  width: number;
  fps: number;
  /** Seconds each slide stays (after its transition and animations). */
  seconds: number;
  /** Use slides' recorded "advance after" times. */
  timings: boolean;
}

export interface VideoResult {
  ok: boolean;
  cancelled: boolean;
  error: string | null;
  path: string;
}

/** Starts a video export of `deck` (PPTX bytes); progress and the result arrive as events. */
export function exportVideo(path: string, deck: Uint8Array, s: VideoSettings): Promise<void> {
  return invoke("export_video", deck, {
    headers: { path: encodeURIComponent(path), width: String(s.width), fps: String(s.fps), seconds: String(s.seconds), timings: s.timings ? "1" : "0" },
  });
}

export function cancelVideoExport(): Promise<void> {
  return inTauri ? invoke("cancel_video_export") : Promise.resolve();
}

/** Subscribes to an export's progress (frames done of total) and outcome. */
export async function onVideoEvents(progress: (done: number, total: number) => void, done: (r: VideoResult) => void): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  const a = await listen<{ done: number; total: number }>("video-progress", (e) => progress(e.payload.done, e.payload.total));
  const b = await listen<VideoResult>("video-done", (e) => done(e.payload));
  return () => {
    a();
    b();
  };
}

/** An autosaved copy left by a session that did not end normally (`recovery_list`). */
export interface RecoveryInfo {
  id: string;
  name: string;
  path: string | null;
  /** Milliseconds since the epoch. */
  savedAt: number;
}

/** Autosaves the open document for crash recovery. */
export function recoveryWrite(id: string, name: string, path: string | null, bytes: Uint8Array): Promise<void> {
  if (!inTauri) return Promise.resolve();
  return invoke("recovery_write", bytes, { headers: { id, name: encodeURIComponent(name), path: encodeURIComponent(path ?? "") } });
}

/** Removes a recovery copy (the document was saved or discarded). */
export function recoveryClear(id: string): Promise<void> {
  return inTauri ? invoke("recovery_clear", { id }) : Promise.resolve();
}

export function recoveryList(): Promise<RecoveryInfo[]> {
  return inTauri ? invoke<RecoveryInfo[]>("recovery_list") : Promise.resolve([]);
}

export async function recoveryRead(id: string): Promise<Uint8Array> {
  return new Uint8Array(await invoke<ArrayBuffer>("recovery_read", { id }));
}

/** Offers an autosaved copy: "recover", "discard" or "later" (kept for next time). */
export async function askRecover(info: RecoveryInfo): Promise<"recover" | "discard" | "later"> {
  const when = new Date(info.savedAt).toLocaleString(locale, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const [recover, discard] = [t("recover.recover"), t("recover.discard")];
  const result = await message(t("recover.ask", { app: APP_NAME, name: info.name, when }), {
    title: t("recover.title"),
    kind: "warning",
    buttons: { yes: recover, no: discard, cancel: t("recover.later") },
  });
  return result === recover ? "recover" : result === discard ? "discard" : "later";
}
