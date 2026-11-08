// Plays the sounds and videos of a slide show: what the engine reads from the
// file (`mediaClips`) is started and stopped as the show moves between slides
// and clicks. Videos are shown over the slide at the picture's frame.
import type { App as Engine } from "../wasm/ppt_wasm";
import { t } from "../i18n";

interface Clip {
  slide: number;
  /** The picture's id on the slide (0 for a narration). */
  shape: number;
  kind: "audio" | "video";
  key: string;
  /** 0: with the slide; n: with click n. */
  start: number;
  volume: number;
  repeat: boolean;
  /** Slides it plays over, counting its own. */
  slides: number;
  trimStartMs: number;
  trimEndMs: number;
  /** The picture's frame as fractions of the slide. */
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where the slide is drawn, for placing videos over it: `host` is what a video is added to. */
export interface SlideBox {
  host: HTMLElement;
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Finds the slide on screen: the presenter view's current-slide box when that is up, else
 * the show screen, which the slide fills as large as its aspect ratio (width / height) allows.
 */
export function domSlideBox(aspect: number): () => SlideBox | null {
  return () => {
    const pv = document.querySelector<HTMLElement>(".pv-current");
    if (pv) return { host: pv, left: 0, top: 0, width: pv.clientWidth, height: pv.clientHeight };
    const host = document.querySelector<HTMLElement>(".slide-show");
    if (!host) return null;
    const [cw, ch] = [host.clientWidth, host.clientHeight];
    const width = Math.min(cw, ch * aspect);
    const height = width / aspect;
    return { host, left: (cw - width) / 2, top: (ch - height) / 2, width, height };
  };
}

/**
 * Finds the slide in the editing canvas: the engine knows where it is drawn (it follows the zoom,
 * the scroll and the window), and the canvas sits at the top left of its host, so the engine's
 * CSS px are the host's. Null while the canvas is not there or a show has taken the engine.
 */
export function canvasSlideBox(engine: Engine): () => SlideBox | null {
  return () => {
    const host = document.querySelector<HTMLElement>(".canvas-host");
    const r = engine.slideRect();
    if (!host || r.length !== 4) return null;
    return { host, left: r[0], top: r[1], width: r[2], height: r[3] };
  };
}

/** What a window plays: the main window plays everything; the audience window only shows videos, silently. */
export interface MediaOptions {
  audio: boolean;
  muted: boolean;
  /** Called when the viewer pauses, plays or seeks a video, so the other window can do the same. */
  onControl?: (m: ControlMsg) => void;
}

/** A video told to pause, play or move to `t` seconds (which video: its slide, picture and file). */
export interface ControlMsg {
  slide: number;
  shape: number;
  key: string;
  action: "pause" | "play" | "seek";
  t: number;
}

const BAR_AUTO_HIDE_MS = 2200;
const ICON_PLAY = '<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true"><path d="M3.5 2l8 5-8 5z"/></svg>';
const ICON_PAUSE = '<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true"><rect x="3" y="2" width="3" height="10" rx="0.5"/><rect x="8" y="2" width="3" height="10" rx="0.5"/></svg>';
const ICON_VOLUME = '<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true"><path d="M2 5h2.5L8 2v10L4.5 9H2z"/><path d="M10 4.5a3.5 3.5 0 010 5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>';
const ICON_MUTED = '<svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor" aria-hidden="true"><path d="M2 5h2.5L8 2v10L4.5 9H2z"/><path d="M10 5l3 4M13 5l-3 4" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>';

function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const MIME: Record<string, string> = { m4a: "audio/mp4", mp3: "audio/mpeg", wav: "audio/wav", aac: "audio/aac", mp4: "video/mp4", m4v: "video/mp4", mov: "video/quicktime" };

/** The show's media being played, for the dev automation's `mediastate`. */
let active: ShowMedia | null = null;
export const activeShowMedia = () => active;

export class ShowMedia {
  private clips: Clip[];
  private playing = new Map<number, HTMLMediaElement>();
  private urls = new Map<string, string>();
  private paused = false;
  /** Videos the viewer paused: they stay paused when the screen comes back or the view moves. */
  private manual = new Set<number>();
  private bar: HTMLDivElement | null = null;
  private barClip = -1;
  private barTimer = 0;
  private at = { slide: 0, step: 0, steps: 0 };

  /** `slideBox`: where the slide is on screen now (null while the show has no screen to put a video on). */
  constructor(
    private engine: Engine,
    private slideBox: () => SlideBox | null,
    private options: MediaOptions = { audio: true, muted: false },
  ) {
    this.clips = (engine.mediaClips() as Clip[]).filter((c) => c.slides >= 1 && (options.audio || c.kind === "video"));
    active = this;
    window.addEventListener("resize", this.relayout);
    // The window that plays the sound also lets the viewer pause and seek.
    if (options.audio) {
      window.addEventListener("pointermove", this.onMove);
      window.addEventListener("pointerdown", this.onDown, true);
    }
  }

  /** The video under the pointer (client px), if any. */
  private clipAt(x: number, y: number): number {
    let found = -1;
    for (const [i, el] of this.playing) {
      if (!(el instanceof HTMLVideoElement)) continue;
      const r = el.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) found = i;
    }
    return found;
  }

  private onMove = (e: PointerEvent) => {
    if (this.paused) return;
    const i = this.clipAt(e.clientX, e.clientY);
    if (i < 0) this.hideBar();
    else this.showBar(i);
  };

  /** A click on a video pauses or plays it instead of moving the show on. */
  private onDown = (e: PointerEvent) => {
    if (e.button !== 0 || this.paused || (this.bar && this.bar.contains(e.target as Node))) return;
    const i = this.clipAt(e.clientX, e.clientY);
    if (i < 0) return;
    e.preventDefault();
    e.stopPropagation();
    this.togglePause(i);
    this.showBar(i);
  };

  private video(i: number): HTMLVideoElement | null {
    const el = this.playing.get(i);
    return el instanceof HTMLVideoElement ? el : null;
  }

  /** The trimmed part of a video, as [start, end] seconds. */
  private range(i: number, el: HTMLVideoElement): [number, number] {
    const c = this.clips[i];
    const start = c.trimStartMs / 1000;
    const d = Number.isFinite(el.duration) ? el.duration : 0;
    return [start, d > 0 ? Math.max(start, d - c.trimEndMs / 1000) : start];
  }

  private emitControl(i: number, action: ControlMsg["action"], t: number) {
    const c = this.clips[i];
    this.options.onControl?.({ slide: c.slide, shape: c.shape, key: c.key, action, t });
  }

  togglePause(i: number) {
    const el = this.video(i);
    if (!el) return;
    if (el.paused || el.ended) {
      const [start, end] = this.range(i, el);
      if (el.ended || (end > start && el.currentTime >= end - 0.05)) el.currentTime = start;
      this.manual.delete(i);
      void el.play().catch(() => {});
      this.emitControl(i, "play", el.currentTime);
    } else {
      this.manual.add(i);
      el.pause();
      this.emitControl(i, "pause", el.currentTime);
    }
  }

  /** Moves to `t` seconds within the trimmed part. */
  private seekTo(i: number, t: number) {
    const el = this.video(i);
    if (!el) return;
    const [start, end] = this.range(i, el);
    el.currentTime = end > start ? Math.min(end, start + Math.max(0, t)) : start + Math.max(0, t);
    this.emitControl(i, "seek", el.currentTime);
  }

  /** Does what the other window's viewer did (the clips are found by slide, picture and file). */
  apply(m: ControlMsg) {
    const i = this.clips.findIndex((c) => c.slide === m.slide && c.shape === m.shape && c.key === m.key);
    const el = i >= 0 ? this.video(i) : null;
    if (!el) return;
    if (m.action === "seek") el.currentTime = m.t;
    else if (m.action === "pause") {
      this.manual.add(i);
      el.pause();
      el.currentTime = m.t;
    } else {
      this.manual.delete(i);
      el.currentTime = m.t;
      if (!this.paused) void el.play().catch(() => {});
    }
  }

  private makeBar(): HTMLDivElement {
    const bar = document.createElement("div");
    bar.className = "media-controls show-media-controls";
    bar.setAttribute("role", "group");
    bar.setAttribute("aria-label", t("media.controls"));
    bar.innerHTML = `<button type="button" data-a="play"></button><span class="media-time" data-a="time">0:00</span><input type="range" class="media-seek" data-a="seek" min="0" max="1" step="0.1" value="0" aria-label="${t("media.position")}"><span class="media-time" data-a="len">0:00</span><button type="button" data-a="mute"></button><input type="range" class="media-volume" data-a="volume" min="0" max="1" step="0.05" aria-label="${t("media.volume")}">`;
    // Keeps these presses from moving the show on.
    bar.addEventListener("pointerdown", (e) => e.stopPropagation());
    bar.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest("button")?.dataset.a;
      const el = this.video(this.barClip);
      if (!el) return;
      if (a === "play") this.togglePause(this.barClip);
      else if (a === "mute") el.muted = !el.muted;
      this.syncBar();
    });
    bar.addEventListener("input", (e) => {
      const input = e.target as HTMLInputElement;
      const el = this.video(this.barClip);
      if (!el) return;
      if (input.dataset.a === "seek") this.seekTo(this.barClip, Number(input.value));
      else if (input.dataset.a === "volume") {
        el.volume = Number(input.value);
        el.muted = el.volume === 0;
      }
      this.syncBar();
    });
    return bar;
  }

  private showBar(i: number) {
    const el = this.video(i);
    if (!el || el.offsetWidth < 160) return;
    if (!this.bar) this.bar = this.makeBar();
    if (this.bar.parentElement !== el.parentElement) el.parentElement?.appendChild(this.bar);
    this.barClip = i;
    this.bar.style.display = "";
    this.positionBar();
    this.syncBar();
    window.clearTimeout(this.barTimer);
    this.barTimer = window.setTimeout(() => this.hideBar(), BAR_AUTO_HIDE_MS);
  }

  private hideBar() {
    window.clearTimeout(this.barTimer);
    if (this.bar) this.bar.style.display = "none";
    this.barClip = -1;
  }

  /** At the bottom of the video, inside it. */
  private positionBar() {
    const el = this.video(this.barClip);
    if (!el || !this.bar) return;
    Object.assign(this.bar.style, { left: `${el.offsetLeft + 8}px`, top: `${el.offsetTop + el.offsetHeight - 42}px`, width: `${el.offsetWidth - 16}px` });
  }

  /** Shows the video's state in the bar. */
  private syncBar() {
    const el = this.video(this.barClip);
    const bar = this.bar;
    if (!el || !bar) return;
    const [start, end] = this.range(this.barClip, el);
    const length = Math.max(0, end - start);
    const time = Math.max(0, Math.min(length, el.currentTime - start));
    const playing = !el.paused && !el.ended;
    const set = (a: string) => bar.querySelector<HTMLElement>(`[data-a="${a}"]`)!;
    const play = set("play");
    play.innerHTML = playing ? ICON_PAUSE : ICON_PLAY;
    play.setAttribute("aria-label", playing ? t("media.pause") : t("media.play"));
    play.title = playing ? t("media.pause") : t("media.play");
    set("time").textContent = clock(time);
    set("len").textContent = clock(length);
    const seek = set("seek") as HTMLInputElement;
    seek.max = String(Math.max(length, 0.01));
    seek.value = String(time);
    seek.setAttribute("aria-valuetext", `${clock(time)} / ${clock(length)}`);
    const mute = set("mute");
    mute.innerHTML = el.muted || el.volume === 0 ? ICON_MUTED : ICON_VOLUME;
    mute.setAttribute("aria-label", el.muted ? t("media.unmute") : t("media.mute"));
    mute.title = el.muted ? t("media.unmute") : t("media.mute");
    (set("volume") as HTMLInputElement).value = String(el.muted ? 0 : el.volume);
  }

  /** Puts the playing videos where the slide is now (it moves with the window and with the view). */
  relayout = () => {
    const box = this.slideBox();
    if (!box) return;
    for (const [i, el] of this.playing) {
      if (!(el instanceof HTMLVideoElement)) continue;
      // A video moved into another view stops by itself; it carries on from where it was.
      if (el.parentElement !== box.host) {
        box.host.appendChild(el);
        if (!this.paused && !el.ended && !this.manual.has(i)) void el.play().catch(() => {});
      }
      this.place(el, box, this.clips[i]);
    }
    if (this.barClip >= 0) {
      if (this.video(this.barClip)) this.positionBar();
      else this.hideBar();
    }
  };

  /** What is playing: kind, file, position (s) and whether it is paused. */
  describe() {
    return { at: this.at, trace: this.trace, playing: this.list() };
  }

  private list() {
    return [...this.playing].map(([i, el]) => ({ kind: this.clips[i].kind, key: this.clips[i].key, t: Number(el.currentTime.toFixed(2)), paused: el.paused, volume: el.volume, error: el.error?.message }));
  }

  /** Whether the show has anything to play. */
  get any(): boolean {
    return this.clips.length > 0;
  }

  /** The show is on `slide` with `step` of its `steps` clicks played (-1: the end screen). */
  update(slide: number, step: number, steps: number) {
    this.at = { slide, step, steps };
    this.clips.forEach((c, i) => {
      const live = this.playing.has(i);
      if (live && (slide < c.slide || slide > c.slide + c.slides - 1)) this.stopClip(i);
      // A clip waits for its click; one whose click never comes (the slide has fewer) starts at once.
      else if (!live && !this.done.has(i) && slide === c.slide && (c.start === 0 || c.start <= step || c.start > steps)) this.startClip(i, c);
    });
    // The view may have changed (slides, presenter view) since the videos were put up.
    this.relayout();
    // Leaving a slide lets its clips play again when the show comes back to it.
    for (const i of [...this.done]) if (slide < this.clips[i].slide || slide > this.clips[i].slide + this.clips[i].slides - 1) this.done.delete(i);
  }

  /** Black or white screen: everything pauses until the slides are back. */
  setBlank(blank: boolean) {
    if (blank === this.paused) return;
    this.paused = blank;
    for (const el of this.playing.values()) {
      if (blank) el.pause();
    }
    if (blank) this.hideBar();
    else for (const [i, el] of this.playing) if (!this.manual.has(i)) void el.play().catch(() => {});
  }

  /** Stops everything and releases the media. */
  stop() {
    for (const i of [...this.playing.keys()]) this.stopClip(i);
    this.done.clear();
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear();
    window.removeEventListener("resize", this.relayout);
    window.removeEventListener("pointermove", this.onMove);
    window.removeEventListener("pointerdown", this.onDown, true);
    window.clearTimeout(this.barTimer);
    this.bar?.remove();
    this.bar = null;
    this.manual.clear();
    if (active === this) active = null;
  }

  /** Clips that have started on the slide being shown (they do not start again until it is left). */
  private done = new Set<number>();

  private url(c: Clip): string | null {
    const known = this.urls.get(c.key);
    if (known) return known;
    const bytes = this.engine.mediaBytes(c.key);
    if (!bytes) return null;
    const ext = c.key.split(".").pop()?.toLowerCase() ?? "";
    const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: MIME[ext] ?? (c.kind === "video" ? "video/mp4" : "audio/mp4") }));
    this.urls.set(c.key, url);
    return url;
  }

  /** Events for the dev automation's `mediastate`. */
  trace: string[] = [];

  private startClip(i: number, c: Clip) {
    try {
      this.startClipUnchecked(i, c);
    } catch (e) {
      this.trace.push(`clip ${i} failed: ${String(e)}`);
    }
  }

  private startClipUnchecked(i: number, c: Clip) {
    const url = this.url(c);
    // A video needs the show's screen to be there; until it is, the next update tries again.
    const box = c.kind === "video" ? this.slideBox() : null;
    this.trace.push(`start ${i} ${c.kind} url=${!!url} host=${c.kind === "video" ? !!box : "-"}`);
    if (c.kind === "video" && !box) return;
    this.done.add(i);
    if (!url) return;
    let el: HTMLMediaElement;
    if (c.kind === "video" && box) {
      if (c.w <= 0 || c.h <= 0) return;
      const v = document.createElement("video");
      v.className = "show-video";
      v.playsInline = true;
      this.place(v, box, c);
      box.host.appendChild(v);
      el = v;
    } else {
      el = new Audio();
    }
    el.src = url;
    el.volume = Math.max(0, Math.min(1, c.volume));
    el.muted = this.options.muted;
    const from = c.trimStartMs / 1000;
    const seek = () => {
      el.currentTime = from;
    };
    el.addEventListener("loadedmetadata", () => {
      if (from > 0) seek();
    });
    // The trimmed end, then either round again or stop (a video keeps its last frame).
    const finish = () => {
      if (c.repeat) {
        seek();
        void el.play().catch(() => {});
      } else {
        el.pause();
      }
    };
    el.addEventListener("ended", finish);
    for (const ev of ["timeupdate", "play", "pause", "volumechange", "loadedmetadata"]) el.addEventListener(ev, () => this.barClip === i && this.syncBar());
    if (c.trimEndMs > 0) {
      el.addEventListener("timeupdate", () => {
        if (el.duration && el.currentTime >= el.duration - c.trimEndMs / 1000) finish();
      });
    }
    this.playing.set(i, el);
    if (!this.paused) void el.play().catch(() => {});
  }

  /** Puts a video over its picture on the slide. */
  private place(v: HTMLVideoElement, box: SlideBox, c: Clip) {
    Object.assign(v.style, { left: `${box.left + c.x * box.width}px`, top: `${box.top + c.y * box.height}px`, width: `${c.w * box.width}px`, height: `${c.h * box.height}px` });
  }

  private stopClip(i: number) {
    const el = this.playing.get(i);
    this.playing.delete(i);
    this.done.delete(i);
    this.manual.delete(i);
    if (this.barClip === i) this.hideBar();
    if (!el) return;
    el.pause();
    el.removeAttribute("src");
    el.load();
    if (el instanceof HTMLVideoElement) el.remove();
  }
}
