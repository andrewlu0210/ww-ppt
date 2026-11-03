// Plays the sounds and videos of a slide show: what the engine reads from the
// file (`mediaClips`) is started and stopped as the show moves between slides
// and clicks. Videos are shown over the slide at the picture's frame.
import type { App as Engine } from "../wasm/ppt_wasm";

interface Clip {
  slide: number;
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

/** What a window plays: the main window plays everything; the audience window only shows videos, silently. */
export interface MediaOptions {
  audio: boolean;
  muted: boolean;
}

const MIME: Record<string, string> = { m4a: "audio/mp4", mp3: "audio/mpeg", wav: "audio/wav", aac: "audio/aac", mp4: "video/mp4", m4v: "video/mp4", mov: "video/quicktime" };

/** The show's media being played, for the dev automation's `mediastate`. */
let active: ShowMedia | null = null;
export const activeShowMedia = () => active;

export class ShowMedia {
  private clips: Clip[];
  private playing = new Map<number, HTMLMediaElement>();
  private urls = new Map<string, string>();
  private paused = false;
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
        if (!this.paused && !el.ended) void el.play().catch(() => {});
      }
      this.place(el, box, this.clips[i]);
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
      else void el.play().catch(() => {});
    }
  }

  /** Stops everything and releases the media. */
  stop() {
    for (const i of [...this.playing.keys()]) this.stopClip(i);
    this.done.clear();
    for (const url of this.urls.values()) URL.revokeObjectURL(url);
    this.urls.clear();
    window.removeEventListener("resize", this.relayout);
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
    if (!el) return;
    el.pause();
    el.removeAttribute("src");
    el.load();
    if (el instanceof HTMLVideoElement) el.remove();
  }
}
