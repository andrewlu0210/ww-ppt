// Dev-only script driver (see src-tauri/src/automation.rs). Commands use CSS
// pixels relative to the canvas, e.g. `click 400 300 2`, `drag 1 2 3 4`,
// `text 你好`, `compose ㄋㄧˇ`, `commit 你`, `key Enter meta`, `shot name`.
import { lastAnnouncement } from "./announce";
import { nextFrame, takeMarks, wasmMemoryMB } from "./perf";
import { invoke } from "@tauri-apps/api/core";
import { emit, emitTo } from "@tauri-apps/api/event";
import { availableMonitors } from "@tauri-apps/api/window";
import type { App } from "../wasm/ppt_wasm";
import { exportVideo, inTauri, onVideoEvents, writeFile } from "./backend";
import type { Command } from "./commands";

interface Hooks {
  engine: () => App | null;
  openPath: (path: string) => Promise<void>;
  refresh: () => void;
  run: (c: Command) => Promise<void>;
}

const log = (line: string) => invoke("automation_log", { line });

async function capture(engine: App, name: string) {
  const canvas = document.querySelector<HTMLCanvasElement>(".canvas-host canvas");
  if (!canvas) throw new Error("no canvas");
  // The WebGPU drawing buffer is only readable in the task that presented it.
  engine.invalidate();
  engine.render();
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("capture failed");
  await invoke("automation_write", new Uint8Array(await blob.arrayBuffer()), { headers: { name } });
}


/** p50 / p95 / max of `xs`, formatted. */
function spread(xs: number[]): string {
  if (xs.length === 0) return "-";
  const s = [...xs].sort((a, b) => a - b);
  const q = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return `p50 ${q(0.5).toFixed(1)} / p95 ${q(0.95).toFixed(1)} / max ${s[s.length - 1].toFixed(1)} ms`;
}

const frame = () => new Promise<number>((r) => requestAnimationFrame((t) => r(t)));

type A11yItem = { id: number; role: string; rect: [number, number, number, number] };

/** Drags an object of the current slide around for `frames` frames, one move per frame. */
async function benchDrag(engine: App, frames: number, pick: string) {
  const items = (engine.a11ySlide() as { items: A11yItem[] }).items;
  // A picture or shape (else anything), not too large to grab.
  const target = items.find((i) => i.role === pick) ?? items.find((i) => i.role === "image" || i.role === "shape") ?? items[0];
  if (!target) throw new Error("no object to drag");
  const [x, y, w, h] = target.rect;
  const cx = x + w / 2;
  const cy = y + h / 2;
  // Let idle work (thumbnails, prefetching) finish, then hold the pointer down
  // the way a real drag does, so background work stays out of the way.
  await new Promise((r) => setTimeout(r, 1500));
  window.dispatchEvent(new PointerEvent("pointerdown"));
  engine.pointerDown(cx, cy, false, true, false, 1);
  const handle: number[] = [];
  const scene: number[] = [];
  const total: number[] = [];
  const intervals: number[] = [];
  let last = await frame();
  for (let i = 1; i <= frames; i++) {
    const a = (i / frames) * Math.PI * 4;
    const t0 = performance.now();
    engine.pointerMove(cx + Math.cos(a) * 60 - 60, cy + Math.sin(a) * 40, false, i % 2 === 0, false);
    handle.push(performance.now() - t0);
    const t = await frame();
    intervals.push(t - last);
    last = t;
    const st = engine.status() as { frameMs: number; sceneMs: number };
    scene.push(st.sceneMs);
    total.push(st.frameMs);
  }
  engine.pointerUp(cx, cy, false, false, false);
  window.dispatchEvent(new PointerEvent("pointerup"));
  engine.undo();
  const refresh = intervals.slice().sort((a, b) => a - b)[Math.floor(intervals.length / 2)];
  const dropped = intervals.filter((d) => d > refresh * 1.5).length;
  return [
    `bench drag (${target.role}, ${frames} frames): display frame ${refresh.toFixed(1)} ms (${(1000 / refresh).toFixed(0)} Hz), dropped ${dropped}`,
    `  pointer move ${spread(handle)}`,
    `  scene build  ${spread(scene)}`,
    `  whole frame  ${spread(total)}`,
    `  frame interval ${spread(intervals)}`,
  ];
}

/** Types `count` characters at the end of the current slide's first text, timing each to its frame. */
async function benchType(engine: App, count: number) {
  const items = (engine.a11ySlide() as { items: A11yItem[] }).items;
  const target = items.find((i) => i.role === "text") ?? items.find((i) => i.role === "heading");
  if (!target) throw new Error("no text on this slide");
  const [x, y, w, h] = target.rect;
  engine.pointerDown(x + w / 2, y + h / 2, false, false, false, 2);
  engine.pointerUp(x + w / 2, y + h / 2, false, false, false);
  engine.keyDown("End", false, false, true, false);
  await new Promise((r) => setTimeout(r, 1500));
  await frame();
  const handle: number[] = [];
  const latency: number[] = [];
  const scene: number[] = [];
  const chars = "測試文字abc ";
  for (let i = 0; i < count; i++) {
    // A key press, as typing makes (background work waits for quiet).
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Unidentified" }));
    const t0 = performance.now();
    engine.insertText(chars[i % chars.length]);
    handle.push(performance.now() - t0);
    // The app's own frame callback (registered first) renders; this one runs after it.
    await frame();
    latency.push(performance.now() - t0);
    scene.push((engine.status() as { sceneMs: number }).sceneMs);
  }
  engine.keyDown("Escape", false, false, false, false);
  for (let i = 0; i < count + 2; i++) engine.undo();
  return [
    `bench type (${count} characters): input to frame ${spread(latency)}`,
    `  insert (layout) ${spread(handle)}`,
    `  scene build     ${spread(scene)}`,
  ];
}

async function step(cmd: string, h: Hooks) {
  const engine = h.engine();
  if (!engine) throw new Error("engine not ready");
  const [op, ...args] = cmd.split(" ");
  const rest = cmd.slice(op.length + 1);
  const num = (i: number) => Number(args[i]);
  const has = (flag: string) => args.includes(flag);
  switch (op) {
    case "click":
      engine.pointerDown(num(0), num(1), has("shift"), false, false, args[2] ? num(2) : 1);
      engine.pointerUp(num(0), num(1), false, false, false);
      break;
    case "drag": {
      const steps = 10;
      engine.pointerDown(num(0), num(1), false, false, false, 1);
      for (let i = 1; i <= steps; i++) {
        const t = i / steps;
        engine.pointerMove(num(0) + (num(2) - num(0)) * t, num(1) + (num(3) - num(1)) * t, has("shift"), false, false);
      }
      engine.pointerUp(num(2), num(3), false, false, false);
      break;
    }
    // Split drags, to capture guides and the marquee mid-gesture.
    case "down":
      engine.pointerDown(num(0), num(1), has("shift"), false, false, 1);
      break;
    case "move":
      engine.pointerMove(num(0), num(1), has("shift"), has("alt"), false);
      break;
    case "up":
      engine.pointerUp(num(0), num(1), false, false, false);
      break;
    case "key":
      engine.keyDown(args[0], has("shift"), has("alt"), has("meta"), false);
      break;
    case "text":
      engine.insertText(rest);
      break;
    case "compose":
      engine.composeUpdate(rest, -1);
      break;
    case "commit":
      engine.composeEnd(rest);
      break;
    case "slide":
      engine.setSlide(num(0));
      break;
    case "shape":
      engine.addShape(args[0]);
      break;
    case "addtext":
      engine.addTextBox();
      break;
    case "open":
      await h.openPath(rest);
      break;
    case "zoom":
      engine.setZoom(num(0));
      break;
    case "run":
      await h.run(JSON.parse(rest) as Command);
      break;
    case "save":
      await writeFile(rest, engine.savePptx());
      break;
    case "pdf":
      await writeFile(rest, engine.exportPdf());
      break;
    case "thumbinfo": {
      const t = (await engine.renderThumbnail(num(0), 320, false)) as { width: number; height: number; pixels: Uint8Array };
      let dark = 0;
      for (let i = 0; i < t.pixels.length; i += 4) if (t.pixels[i] < 200) dark++;
      const thumbs = document.querySelectorAll<HTMLCanvasElement>(".slide-thumb");
      await log(`thumbinfo ${t.width}x${t.height} bytes=${t.pixels.length} darkPixels=${dark} domCanvases=${thumbs.length} sizes=${Array.from(thumbs).map((c) => `${c.width}x${c.height}`).join(",")}`);
      break;
    }
    case "thumbs": {
      // Captures the slide pane thumbnails as rendered in the DOM.
      const canvases = Array.from(document.querySelectorAll<HTMLCanvasElement>(".slide-thumb"));
      const sheet = document.createElement("canvas");
      sheet.width = 320;
      sheet.height = Math.max(1, canvases.length * 190);
      const ctx = sheet.getContext("2d")!;
      canvases.forEach((c, i) => c.width > 0 && ctx.drawImage(c, 0, i * 190, 320, 180));
      const blob = await new Promise<Blob | null>((resolve) => sheet.toBlob(resolve, "image/png"));
      if (blob) await invoke("automation_write", new Uint8Array(await blob.arrayBuffer()), { headers: { name: args[0] ?? "thumbs" } });
      break;
    }
    case "shot":
      await capture(engine, args[0] ?? "shot");
      break;
    // Slide show input goes through the DOM layer, like real key presses.
    case "showkey": {
      const layer = document.querySelector<HTMLElement>(".slide-show, .presenter");
      if (!layer) throw new Error("no slide show");
      layer.dispatchEvent(new KeyboardEvent("keydown", { key: args[0] === "Space" ? " " : args[0], altKey: has("alt"), bubbles: true }));
      break;
    }
    case "showclick": {
      const layer = document.querySelector<HTMLElement>(".slide-show");
      if (!layer) throw new Error("no slide show");
      layer.dispatchEvent(new PointerEvent("pointerdown", { button: num(0) || 0, bubbles: true }));
      break;
    }
    // Notes pane: type (as the user would, so WebKit's undo sees it) and read back.
    case "notestype": {
      const ta = document.querySelector<HTMLTextAreaElement>(".notes-input");
      if (!ta) throw new Error("no notes pane");
      ta.focus();
      document.execCommand("insertText", false, rest);
      break;
    }
    // SmartArt text pane: type into row N as the user would, e.g. `rowtype 2 新的文字`.
    case "rowtype": {
      const ta = document.querySelectorAll<HTMLTextAreaElement>(".diagram-input")[num(0)];
      if (!ta) throw new Error(`no text pane row ${args[0]}`);
      ta.focus();
      document.execCommand("insertText", false, args.slice(1).join(" "));
      break;
    }
    // Video export through the backend, as the dialog does: `video out.mp4 1280 2`.
    case "video": {
      const result = new Promise<string>((resolve) => {
        const off = onVideoEvents(
          () => {},
          (r) => {
            void off.then((f) => f());
            resolve(JSON.stringify(r));
          },
        );
      });
      await exportVideo(args[0], engine.savePptx(), { width: num(1) || 1280, fps: 30, seconds: num(2) || 2, timings: true });
      await log(`video ${await result}`);
      break;
    }
    case "notesget": {
      const ta = document.querySelector<HTMLTextAreaElement>(".notes-input");
      const s = engine.status() as { slideIndex: number };
      await log(`notes pane=${JSON.stringify(ta?.value ?? null)} engine=${JSON.stringify(engine.notes(s.slideIndex))} focused=${document.activeElement === ta}`);
      break;
    }
    // Same path as a native menu item.
    case "menu":
      await emit("menu", args[0]);
      break;
    case "monitors":
      await log(`monitors ${JSON.stringify((await availableMonitors()).map((m) => ({ name: m.name, pos: m.position, size: m.size, scale: m.scaleFactor })))}`);
      break;
    // Input as if pressed in the audience window.
    case "audiencekey":
      await emit("show-input", { name: args[0] });
      break;
    case "audienceshot":
      await emitTo("audience", "audience-shot", args[0] ?? "audience");
      break;
    case "domshow":
      await log(`dom presenter=${document.querySelector(".presenter") !== null} slideshow=${document.querySelector(".slide-show") !== null} count=${document.querySelector(".pv-count")?.textContent ?? ""} notes=${JSON.stringify(document.querySelector(".pv-notes")?.textContent?.slice(0, 40) ?? "")}`);
      break;
    case "blur":
      (document.activeElement as HTMLElement | null)?.blur();
      break;
    case "showstate":
      await log(`showstate ${JSON.stringify(engine.showState())} end=${document.querySelector(".show-end") !== null} slide=${(engine.status() as { slideIndex: number }).slideIndex}`);
      break;
    case "transition": {
      const st = engine.status() as { slideIndex: number; transition?: unknown; transitionSlides?: boolean[] };
      await log(`transition slide=${st.slideIndex} ${JSON.stringify(st.transition ?? null)} marks=${JSON.stringify(st.transitionSlides)}`);
      break;
    }
    case "fill": {
      // Types into a React input: `fill .find-input 營收`.
      const [sel, ...words] = args;
      const el = document.querySelector<HTMLInputElement>(sel);
      if (!el) throw new Error(`no ${sel}`);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(el, words.join(" "));
      el.dispatchEvent(new Event("input", { bubbles: true }));
      break;
    }
    case "press": {
      // A key on an element: `press .find-input Enter shift`.
      const el = document.querySelector<HTMLElement>(args[0]);
      if (!el) throw new Error(`no ${args[0]}`);
      el.dispatchEvent(new KeyboardEvent("keydown", { key: args[1], shiftKey: has("shift"), metaKey: has("meta"), bubbles: true, cancelable: true }));
      break;
    }
    case "rclick": {
      // A right click on the canvas, in canvas CSS pixels.
      const canvas = document.querySelector<HTMLCanvasElement>(".canvas-host canvas")!;
      const r = canvas.getBoundingClientRect();
      canvas.dispatchEvent(new MouseEvent("contextmenu", { clientX: r.left + num(0), clientY: r.top + num(1), bubbles: true, cancelable: true }));
      break;
    }
    case "clickel":
      document.querySelector<HTMLElement>(rest)?.click();
      break;
    case "tab":
      Array.from(document.querySelectorAll<HTMLButtonElement>(".rb-tabs [role=tab]"))
        .find((b) => b.textContent === rest)
        ?.click();
      break;
    case "perf": {
      // Timing marks since the last `perf`, and the WASM heap size.
      for (const [label, ms] of takeMarks()) await log(`perf ${label}: ${ms} ms`);
      const [now, peak, images] = Array.from(engine.memoryStats()).map((b) => (b / 1048576).toFixed(0));
      await log(`perf wasm memory ${wasmMemoryMB().toFixed(0)} MB; heap in use ${now} MB, peak ${peak} MB, decoded images ${images} MB`);
      break;
    }
    case "time": {
      // Times a command: `time run {...}` or `time slide 3`.
      const t = performance.now();
      await step(rest, h);
      await nextFrame();
      await log(`time ${rest}: ${(performance.now() - t).toFixed(1)} ms`);
      break;
    }
    case "benchdrag":
      // `benchdrag 120 [image|shape|text]`
      for (const line of await benchDrag(engine, num(0) || 120, args[1] ?? "image")) await log(line);
      break;
    case "benchtype":
      // `benchtype 40`
      for (const line of await benchType(engine, num(0) || 40)) await log(line);
      break;
    case "spoken":
      // The last screen reader announcement.
      await log(`spoken ${lastAnnouncement}`);
      break;
    case "aria": {
      // Accessible names of the elements matching a selector.
      const els = Array.from(document.querySelectorAll<HTMLElement>(rest));
      await log(`aria ${rest} n=${els.length} ${JSON.stringify(els.slice(0, 12).map((e) => `${e.tagName.toLowerCase()}${e.getAttribute("role") ? `[${e.getAttribute("role")}]` : ""} ${e.getAttribute("aria-label") ?? e.textContent?.trim().slice(0, 40)}`))}`);
      break;
    }
    case "focused": {
      // What has keyboard focus, e.g. `focused` → "BUTTON.rb-btn 色彩 ▾".
      const el = document.activeElement as HTMLElement | null;
      await log(`focused ${el?.tagName}.${(el?.className || "").toString().split(" ").join(".")} ${el?.textContent?.trim().slice(0, 30) ?? ""}`);
      break;
    }
    case "dom": {
      // Count and text of the elements matching a selector, e.g. `dom .anim-badge`.
      const els = Array.from(document.querySelectorAll<HTMLElement>(rest));
      await log(`dom ${rest} n=${els.length} ${JSON.stringify(els.slice(0, 12).map((e) => `${e.textContent?.trim().slice(0, 60)}${e.style.left ? `@${e.style.left},${e.style.top}` : ""}`))}`);
      break;
    }
    case "status":
      await log(`status ${JSON.stringify(engine.status())} ime=${JSON.stringify(Array.from(engine.imeRect()))}`);
      break;
    default:
      throw new Error(`unknown command: ${op}`);
  }
  h.refresh();
}

export async function startAutomation(h: Hooks) {
  if (!inTauri || !(await invoke<boolean>("automation_enabled"))) return;
  await log("ready");
  let busy = false;
  window.setInterval(async () => {
    if (busy) return;
    busy = true;
    try {
      for (const cmd of await invoke<string[]>("automation_poll")) {
        try {
          await step(cmd, h);
          await log(`ok ${cmd}`);
        } catch (e) {
          await log(`error ${cmd}: ${String(e)}`);
        }
      }
    } finally {
      busy = false;
    }
  }, 300);
}
