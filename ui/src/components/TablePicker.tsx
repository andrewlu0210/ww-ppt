import { t } from "../i18n";
import { useState } from "react";
import { Popover } from "./Popover";

const ROWS = 8;
const COLS = 10;

/** PowerPoint-style grid: hover to choose rows × columns, click to insert. */
export function TablePicker(props: { button: (open: boolean, toggle: () => void) => React.ReactNode; onPick: (rows: number, cols: number) => void }) {
  const [hover, setHover] = useState<[number, number]>([0, 0]);
  return (
    <Popover button={props.button}>
      {(close) => (
        <div className="table-picker" onMouseLeave={() => setHover([0, 0])}>
          <div className="table-picker-title">{hover[0] ? t("tablePicker.size", { cols: hover[1], rows: hover[0] }) : t("tablePicker.insert")}</div>
          <div className="table-picker-grid" style={{ gridTemplateColumns: `repeat(${COLS}, 1fr)` }}>
            {Array.from({ length: ROWS * COLS }, (_, i) => {
              const r = Math.floor(i / COLS) + 1;
              const c = (i % COLS) + 1;
              const on = r <= hover[0] && c <= hover[1];
              return (
                <button
                  key={i}
                  className={on ? "table-cell on" : "table-cell"}
                  aria-label={t("tablePicker.cellAria", { rows: r, cols: c })}
                  onMouseEnter={() => setHover([r, c])}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => {
                    props.onPick(r, c);
                    close();
                  }}
                />
              );
            })}
          </div>
        </div>
      )}
    </Popover>
  );
}
