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

export async function pickSavePath(defaultName: string): Promise<string | null> {
  const path = await save({ defaultPath: `${defaultName}.pptx`, filters: [{ name: "PowerPoint 簡報", extensions: ["pptx"] }] });
  if (!path) return null;
  return path.toLowerCase().endsWith(".pptx") ? path : `${path}.pptx`;
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
