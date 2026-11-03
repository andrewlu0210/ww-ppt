import { t } from "../i18n";
import type { ShowCommand, ShowState } from "../lib/show";

interface Props {
  state: ShowState | null;
  command: (c: ShowCommand) => void;
}

/** Navigation bar at the bottom of the reading view: previous, page number, next. */
export function ReadingBar({ state, command }: Props) {
  if (!state) return null;
  const atEnd = state.slide < 0;
  const current = atEnd ? state.count : state.slide + 1;
  // Keep focus on the show so the keyboard keeps working after a click.
  const keep = (e: React.MouseEvent) => e.preventDefault();
  return (
    <div className="reading-bar" role="toolbar" aria-label={t("status.reading")} onPointerDown={(e) => e.stopPropagation()}>
      <button onMouseDown={keep} onClick={() => command({ name: "prev" })} disabled={state.slide === 0} title={t("reading.prev")} aria-label={t("reading.prev")}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M14.5 6l-6 6 6 6" />
        </svg>
      </button>
      <span aria-live="polite">{t("reading.page", { current, total: state.count })}</span>
      <button onMouseDown={keep} onClick={() => command({ name: "next" })} disabled={atEnd} title={t("reading.next")} aria-label={t("reading.next")}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M9.5 6l6 6-6 6" />
        </svg>
      </button>
    </div>
  );
}
