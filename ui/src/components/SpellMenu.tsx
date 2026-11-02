import { t } from "../i18n";
import { ContextMenu, type MenuItem } from "./ContextMenu";

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
  const items: MenuItem[] = [];
  if (guesses === null) items.push({ label: t("spell.looking"), disabled: true });
  else if (guesses.length === 0) items.push({ label: t("spell.none"), disabled: true });
  for (const g of guesses?.slice(0, MAX_GUESSES) ?? []) items.push({ label: g, bold: true, onSelect: () => onPick(g) });
  items.push({ sep: true }, { label: t("spell.ignore"), onSelect: onIgnore }, { label: t("spell.learn"), onSelect: onLearn });
  return <ContextMenu x={x} y={y} items={items} label={t("spell.suggestionsFor", { word: hit.word })} onClose={onClose} />;
}
