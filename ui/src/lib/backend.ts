// Thin wrappers over the native Tauri commands and dialogs.
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
  const picked = await open({ multiple: false, directory: false, filters: [{ name: "PowerPoint 簡報", extensions: ["pptx"] }] });
  return typeof picked === "string" ? picked : null;
}

export async function pickImage(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: false, filters: [{ name: "圖片", extensions: ["png", "jpg", "jpeg", "gif"] }] });
  return typeof picked === "string" ? picked : null;
}

const SAVE_FILTERS = { pptx: "PowerPoint 簡報", pdf: "PDF 文件", mp4: "MPEG-4 視訊" };

export async function pickSavePath(defaultName: string, ext: keyof typeof SAVE_FILTERS = "pptx"): Promise<string | null> {
  const path = await save({ defaultPath: `${defaultName}.${ext}`, filters: [{ name: SAVE_FILTERS[ext], extensions: [ext] }] });
  if (!path) return null;
  return path.toLowerCase().endsWith(`.${ext}`) ? path : `${path}.${ext}`;
}

/** "save" | "discard" | "cancel" for a document with unsaved changes. */
export async function askSaveChanges(name: string): Promise<"save" | "discard" | "cancel"> {
  const result = await message(`要儲存「${name}」的變更嗎？`, {
    title: "ww-ppt",
    kind: "warning",
    buttons: { yes: "儲存", no: "不要儲存", cancel: "取消" },
  });
  return result === "儲存" ? "save" : result === "不要儲存" ? "discard" : "cancel";
}

export function showError(text: string): Promise<unknown> {
  return message(text, { title: "ww-ppt", kind: "error" });
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
  const when = new Date(info.savedAt).toLocaleString("zh-TW", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const result = await message(`ww-ppt 上次沒有正常結束。要復原「${info.name}」嗎？\n\n自動儲存的版本：${when}`, {
    title: "復原文件",
    kind: "warning",
    buttons: { yes: "復原", no: "捨棄", cancel: "稍後再說" },
  });
  return result === "復原" ? "recover" : result === "捨棄" ? "discard" : "later";
}
