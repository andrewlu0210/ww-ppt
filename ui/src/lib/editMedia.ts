// Plays the selected video in place in the editing canvas. Unlike `ShowMedia` it knows nothing of
// clicks or slides: one video at a time, started by the user, over its picture on the slide.
import type { App as Engine } from "../wasm/ppt_wasm";
import { MIME, type SlideBox } from "./showMedia";

interface Clip {
  slide: number;
  shape: number;
  kind: "audio" | "video";
  key: string;
  volume: number;
  trimStartMs: number;
  trimEndMs: number;
  /** The picture's frame as fractions of the slide. */
  x: number;
  y: number;
  w: number;
  h: number;
}

/** What the controls show. Times are in seconds within the trimmed part. */
export interface EditMediaState {
  /** Whether a video is selected (the controls show only then). */
  available: boolean;
  /** Whether its bytes are loaded into a player (they are on the first play). */
  loaded: boolean;
  playing: boolean;
  time: number;
  /** Length of the trimmed part; 0 until the file's length is known. */
  length: number;
  volume: number;
  muted: boolean;
}

const IDLE: EditMediaState = { available: false, loaded: false, playing: false, time: 0, length: 0, volume: 1, muted: false };

export class EditMedia {
  private clip: Clip | null = null;
  private el: HTMLVideoElement | null = null;
  private url: string | null = null;
  private volume = 1;
  private muted = false;
  private listeners = new Set<() => void>();

  constructor(
    private engine: Engine,
    private slideBox: () => SlideBox | null,
  ) {
    window.addEventListener("resize", this.relayout);
  }

  /** Calls `fn` whenever the state changes; returns the way to stop. */
  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  private emit() {
    this.listeners.forEach((fn) => fn());
  }

  /** The video of `shape` on `slide` becomes the one to play; anything else (0, or no video) stops the player. */
  select(slide: number, shape: number) {
    const next = shape > 0 ? this.find(slide, shape) : null;
    if (next && this.clip && next.key === this.clip.key && next.slide === this.clip.slide && next.shape === this.clip.shape) {
      this.clip = next;
      this.relayout();
      return;
    }
    this.release();
    this.clip = next;
    if (next) {
      this.volume = Math.max(0, Math.min(1, next.volume));
      this.muted = false;
    }
    this.emit();
  }

  private find(slide: number, shape: number): Clip | null {
    const clips = this.engine.mediaClips() as Clip[];
    return clips.find((c) => c.kind === "video" && c.slide === slide && c.shape === shape && c.w > 0 && c.h > 0) ?? null;
  }

  /** The picture's frame (it may have moved or been resized) and where the slide is now. */
  relayout = () => {
    if (!this.clip) return;
    const fresh = this.find(this.clip.slide, this.clip.shape);
    if (!fresh) {
      this.select(-1, 0);
      return;
    }
    this.clip = fresh;
    const box = this.slideBox();
    if (box && this.el) this.place(this.el, box, fresh);
  };

  /** The frame on screen, in the host's CSS px (for putting the controls under it). */
  frame(): { left: number; top: number; width: number; height: number } | null {
    const box = this.slideBox();
    if (!box || !this.clip) return null;
    const c = this.clip;
    return { left: box.left + c.x * box.width, top: box.top + c.y * box.height, width: c.w * box.width, height: c.h * box.height };
  }

  state(): EditMediaState {
    if (!this.clip) return IDLE;
    const el = this.el;
    const [start, end] = this.range();
    return {
      available: true,
      loaded: !!el,
      playing: !!el && !el.paused && !el.ended,
      time: el ? Math.max(0, Math.min(end - start, el.currentTime - start)) : 0,
      length: Math.max(0, end - start),
      volume: this.volume,
      muted: this.muted,
    };
  }

  /** The trimmed part of the file, as [start, end] seconds. */
  private range(): [number, number] {
    const c = this.clip;
    const d = this.el && Number.isFinite(this.el.duration) ? this.el.duration : 0;
    if (!c) return [0, 0];
    const start = c.trimStartMs / 1000;
    return [start, d > 0 ? Math.max(start, d - c.trimEndMs / 1000) : start];
  }

  /** Plays from where it is, or from the start when it had reached the end. */
  play() {
    const c = this.clip;
    if (!c) return;
    const el = this.ensure();
    if (!el) return;
    const [start, end] = this.range();
    if (el.ended || (end > start && el.currentTime >= end - 0.05)) el.currentTime = start;
    void el.play().catch(() => {});
  }

  pause() {
    this.el?.pause();
  }

  toggle() {
    if (this.state().playing) this.pause();
    else this.play();
  }

  /** Moves to `t` seconds within the trimmed part (loads the video if it was not yet, showing its frame). */
  seek(t: number) {
    const el = this.ensure();
    if (!el) return;
    const [start, end] = this.range();
    const target = start + Math.max(0, t);
    el.currentTime = end > start ? Math.min(end, target) : target;
    this.emit();
  }

  setVolume(v: number) {
    this.volume = Math.max(0, Math.min(1, v));
    if (this.volume > 0) this.muted = false;
    if (this.el) {
      this.el.volume = this.volume;
      this.el.muted = this.muted;
    }
    this.emit();
  }

  setMuted(m: boolean) {
    this.muted = m;
    if (this.el) this.el.muted = m;
    this.emit();
  }

  /** Stops, takes the player away and lets go of the bytes. */
  stop() {
    this.release();
    this.clip = null;
    window.removeEventListener("resize", this.relayout);
    this.listeners.clear();
  }

  /** Takes the player away (the clip stays selected; a new play starts from the beginning). */
  private release() {
    const el = this.el;
    this.el = null;
    if (el) {
      el.pause();
      el.removeAttribute("src");
      el.load();
      el.remove();
    }
    if (this.url) {
      URL.revokeObjectURL(this.url);
      this.url = null;
    }
  }

  /** The player, made on first use: the bytes are read from the engine and turned into a URL only now. */
  private ensure(): HTMLVideoElement | null {
    if (this.el) return this.el;
    const c = this.clip;
    const box = this.slideBox();
    if (!c || !box) return null;
    const bytes = this.engine.mediaBytes(c.key);
    if (!bytes) return null;
    const ext = c.key.split(".").pop()?.toLowerCase() ?? "";
    this.url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: MIME[ext] ?? "video/mp4" }));
    const v = document.createElement("video");
    v.className = "show-video";
    v.playsInline = true;
    v.preload = "auto";
    v.volume = this.volume;
    v.muted = this.muted;
    this.place(v, box, c);
    box.host.appendChild(v);
    const start = c.trimStartMs / 1000;
    v.addEventListener("loadedmetadata", () => {
      if (start > 0 && v.currentTime < start) v.currentTime = start;
      this.emit();
    });
    // The trimmed end counts as the end of the video.
    v.addEventListener("timeupdate", () => {
      const [, end] = this.range();
      if (c.trimEndMs > 0 && !v.paused && end > 0 && v.currentTime >= end) v.pause();
      this.emit();
    });
    for (const ev of ["play", "pause", "ended", "seeked"]) v.addEventListener(ev, () => this.emit());
    v.src = this.url;
    this.el = v;
    this.emit();
    return v;
  }

  private place(v: HTMLVideoElement, box: SlideBox, c: Clip) {
    Object.assign(v.style, { left: `${box.left + c.x * box.width}px`, top: `${box.top + c.y * box.height}px`, width: `${c.w * box.width}px`, height: `${c.h * box.height}px` });
  }
}
