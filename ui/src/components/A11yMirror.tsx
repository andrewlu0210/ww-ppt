import { t } from "../i18n";
import { useEffect, useState } from "react";
import type { App } from "../wasm/ppt_wasm";
import { focusCanvas } from "./CanvasHost";

interface Paragraph {
  text: string;
  level: number;
  bullet: boolean;
}

interface Item {
  id: number;
  role: "heading" | "text" | "image" | "table" | "chart" | "shape";
  label: string;
  altText: string;
  paragraphs: Paragraph[];
  cells: string[][];
  rect: [number, number, number, number];
  selected: boolean;
}

interface SlideInfo {
  description: string;
  items: Item[];
}

/** Paragraphs as HTML: bulleted runs become lists, the rest paragraphs. */
function Paragraphs({ paragraphs }: { paragraphs: Paragraph[] }) {
  const out: React.ReactNode[] = [];
  let list: Paragraph[] = [];
  const flush = () => {
    if (list.length > 0) {
      out.push(
        <ul key={out.length}>
          {list.map((p, i) => (
            <li key={i} aria-level={p.level + 1}>
              {p.text}
            </li>
          ))}
        </ul>,
      );
      list = [];
    }
  };
  for (const p of paragraphs) {
    if (p.bullet) list.push(p);
    else {
      flush();
      out.push(<p key={out.length}>{p.text}</p>);
    }
  }
  flush();
  return <>{out}</>;
}

/**
 * The slide as HTML for screen readers: one element per object, in reading order,
 * laid over the object on the canvas (invisible), so the VoiceOver cursor outlines
 * it. Activating one (VO-Space) selects that object.
 */
export function A11yMirror({ engine, version }: { engine: App | null; version: string }) {
  const [info, setInfo] = useState<SlideInfo | null>(null);
  const [size, setSize] = useState(0);
  useEffect(() => {
    const onResize = () => setSize((n) => n + 1);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  useEffect(() => {
    // After the canvas has laid out at its new size.
    const raf = requestAnimationFrame(() => setInfo(engine ? (engine.a11ySlide() as SlideInfo) : null));
    return () => cancelAnimationFrame(raf);
  }, [engine, version, size]);
  if (!info) return null;
  const select = (id: number) => {
    engine?.selectInPane(id, false);
    focusCanvas();
  };
  return (
    <section className="a11y-layer" aria-label={info.description}>
      {info.items.map((item) => {
        const [x, y, w, h] = item.rect;
        const style = { left: x, top: y, width: w, height: h };
        const common = { className: "a11y-item", style, "aria-current": item.selected || undefined, onClick: () => select(item.id) };
        const name = item.altText ? `${item.label}：${item.altText}` : item.label;
        switch (item.role) {
          case "heading":
            return (
              <h2 key={item.id} {...common}>
                {item.paragraphs.map((p) => p.text).join(" ")}
              </h2>
            );
          case "text":
            return (
              <div key={item.id} {...common} role="group" aria-label={name}>
                <Paragraphs paragraphs={item.paragraphs} />
              </div>
            );
          case "table":
            return (
              <table key={item.id} {...common} aria-label={name}>
                <tbody>
                  {item.cells.map((row, r) => (
                    <tr key={r}>
                      {row.map((c, i) => (
                        <td key={i}>{c}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            );
          default: {
            // Pictures, charts and plain shapes: their name and description, plus any text on them.
            const text = item.paragraphs.map((p) => p.text).join(" ");
            return <div key={item.id} {...common} role="img" aria-label={text ? t("a11y.withText", { name, text }) : name} />;
          }
        }
      })}
    </section>
  );
}
