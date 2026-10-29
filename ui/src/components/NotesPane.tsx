import { useEffect, useRef } from "react";
import type { App as Engine } from "../wasm/ppt_wasm";

interface Props {
  engine: Engine | null;
  slideIndex: number;
  /** Document revision: notes may have changed through undo/redo. */
  revision: number;
  height: number;
  onResize: (height: number) => void;
  onChange: () => void;
}

export const MIN_NOTES_HEIGHT = 48;

/**
 * Speaker notes of the current slide, under the canvas. The textarea is
 * uncontrolled so WebKit keeps its own undo history while typing.
 */
export function NotesPane({ engine, slideIndex, revision, height, onResize, onChange }: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const shownSlide = useRef(-1);

  // Load the notes when the slide changes, or when they changed elsewhere (undo) while not typing here.
  useEffect(() => {
    const ta = ref.current;
    if (!ta || !engine) return;
    if (shownSlide.current !== slideIndex || document.activeElement !== ta) {
      const text = engine.notes(slideIndex);
      if (ta.value !== text) ta.value = text;
      shownSlide.current = slideIndex;
    }
  }, [engine, slideIndex, revision]);

  const startResize = (e: React.PointerEvent) => {
    e.preventDefault();
    const startY = e.clientY;
    const startH = height;
    const max = window.innerHeight * 0.6;
    const move = (ev: PointerEvent) => onResize(Math.round(Math.min(max, Math.max(MIN_NOTES_HEIGHT, startH + startY - ev.clientY))));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className="notes-pane" style={{ height }}>
      <div className="notes-splitter" onPointerDown={startResize} role="separator" aria-orientation="horizontal" aria-label="調整備忘稿高度" />
      <textarea
        ref={ref}
        className="notes-input"
        placeholder="按一下以新增備忘稿"
        aria-label="備忘稿"
        spellCheck={false}
        onFocus={() => {
          engine?.beginNotesEdit();
          onChange();
        }}
        onInput={(e) => {
          engine?.setNotes(e.currentTarget.value);
          onChange();
        }}
      />
    </div>
  );
}

/** Whether keyboard focus is in the notes pane (menu edit commands then act on its text). */
export function notesFocused(): HTMLTextAreaElement | null {
  const el = document.activeElement;
  return el instanceof HTMLTextAreaElement && el.classList.contains("notes-input") ? el : null;
}
