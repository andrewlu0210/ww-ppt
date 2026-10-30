import { useEffect, useRef } from "react";

/** A misspelled word under the pointer (`App::spellingAt`); ranges in UTF-16 units. */
export interface SpellHit {
  key: string;
  text: string;
  start: number;
  length: number;
  word: string;
}

interface Props {
  /** Where the menu opens, in canvas-area pixels. */
  x: number;
  y: number;
  hit: SpellHit;
  /** Suggestions, or null while they load. */
  guesses: string[] | null;
  onPick: (word: string) => void;
  onIgnore: () => void;
  onLearn: () => void;
  onClose: () => void;
}

const MAX_GUESSES = 6;

/** The right-click menu on a misspelled word: corrections, ignore, add to the dictionary. */
export function SpellMenu({ x, y, hit, guesses, onPick, onIgnore, onLearn, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);
  const item = (label: string, act: () => void, bold = false) => (
    <button
      key={label}
      className={bold ? "spell-item guess" : "spell-item"}
      role="menuitem"
      onMouseDown={(e) => e.preventDefault()}
      onClick={() => {
        act();
        onClose();
      }}
    >
      {label}
    </button>
  );
  return (
    <div ref={ref} className="spell-menu" role="menu" aria-label={`「${hit.word}」的拼字建議`} style={{ left: x, top: y }}>
      {guesses === null && <div className="spell-note">正在尋找建議…</div>}
      {guesses?.length === 0 && <div className="spell-note">沒有拼字建議</div>}
      {guesses?.slice(0, MAX_GUESSES).map((g) => item(g, () => onPick(g), true))}
      <div className="spell-sep" />
      {item("忽略拼字", onIgnore)}
      {item("加入字典", onLearn)}
    </div>
  );
}
