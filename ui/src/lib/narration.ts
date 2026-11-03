// Narration recording during a slide show: one microphone stream, one recording per
// slide. The takes are kept until the show ends, then written into the deck.

/** What one slide's recording came to. */
export interface Take {
  slide: number;
  blob: Blob;
  /** How long the slide stayed on screen (ms). */
  stayMs: number;
}

/** Recordings shorter than this are dropped: flipping past a slide must not wipe its narration. */
export const MIN_TAKE_MS = 1000;

/** The container the web view can record AAC in (what PowerPoint plays as .m4a). */
const MIME = "audio/mp4";

export class NarrationRecorder {
  private takes = new Map<number, Take>();
  private current: { slide: number; recorder: MediaRecorder; chunks: Blob[]; startedAt: number; stopped: Promise<void> } | null = null;

  private constructor(private stream: MediaStream) {}

  /** Asks for the microphone. Throws when it is refused or recording AAC is not supported. */
  static async open(): Promise<NarrationRecorder> {
    if (typeof MediaRecorder === "undefined" || !MediaRecorder.isTypeSupported(MIME)) throw new Error("unsupported");
    return new NarrationRecorder(await navigator.mediaDevices.getUserMedia({ audio: true }));
  }

  /** Ends the recording of the slide on screen and starts one for `slide` (`null`: none). */
  async switchTo(slide: number | null): Promise<void> {
    await this.stopCurrent();
    if (slide === null) return;
    const recorder = new MediaRecorder(this.stream, { mimeType: MIME });
    const chunks: Blob[] = [];
    recorder.ondataavailable = (e) => e.data.size > 0 && chunks.push(e.data);
    const stopped = new Promise<void>((resolve) => (recorder.onstop = () => resolve()));
    recorder.start();
    this.current = { slide, recorder, chunks, startedAt: performance.now(), stopped };
  }

  private async stopCurrent(): Promise<void> {
    const c = this.current;
    if (!c) return;
    this.current = null;
    const stayMs = performance.now() - c.startedAt;
    if (c.recorder.state !== "inactive") c.recorder.stop();
    await c.stopped;
    // Visiting a slide again replaces the earlier take.
    if (stayMs >= MIN_TAKE_MS && c.chunks.length > 0) this.takes.set(c.slide, { slide: c.slide, blob: new Blob(c.chunks, { type: MIME }), stayMs: Math.round(stayMs) });
  }

  /** Milliseconds the slide on screen has been recording. */
  elapsed(): number {
    return this.current ? performance.now() - this.current.startedAt : 0;
  }

  /** Stops, releases the microphone and returns the takes in slide order. */
  async finish(): Promise<Take[]> {
    await this.stopCurrent();
    this.stream.getTracks().forEach((t) => t.stop());
    return [...this.takes.values()].sort((a, b) => a.slide - b.slide);
  }
}
