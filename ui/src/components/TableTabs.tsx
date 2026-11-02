// Contextual ribbon tabs for tables: 表格設計 (style, shading, borders) and 版面配置 (rows, merge, alignment).
import { t } from "../i18n";
import { useState, type ReactNode } from "react";
import { icons } from "./icons";
import { ColorPicker } from "./ColorPicker";
import { Popover } from "./Popover";
import type { Command, TableStyleInfo } from "../lib/commands";
import type { Status } from "../lib/status";

const keepFocus = (e: React.MouseEvent) => e.preventDefault();

function Button(props: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean; large?: boolean; active?: boolean }) {
  const cls = ["rb-btn", props.large && "large", props.active && "active"].filter(Boolean).join(" ");
  return (
    <button className={cls} onClick={props.onClick} disabled={props.disabled} title={props.label} aria-pressed={props.active} onMouseDown={keepFocus}>
      {props.icon}
      <span>{props.label}</span>
    </button>
  );
}

function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="rb-group" role="group" aria-label={label}>
      <div className="rb-items">{children}</div>
    </div>
  );
}

const FLAGS: [string, string][] = [
  ["firstRow", t("table.headerRow")],
  ["firstCol", t("table.firstColumn")],
  ["lastRow", t("table.totalRow")],
  ["lastCol", t("table.lastColumn")],
  ["bandRow", t("table.bandedRows")],
  ["bandCol", t("table.bandedColumns")],
];

const BORDERS: [string, string][] = [
  ["all", t("table.allBorders")],
  ["outside", t("table.outsideBorders")],
  ["inside", t("table.insideBorders")],
  ["top", t("table.topBorder")],
  ["bottom", t("table.bottomBorder")],
  ["left", t("table.leftBorder")],
  ["right", t("table.rightBorder")],
  ["none", t("table.noBorder")],
];

/** A small table drawn with the style's colors. */
function StylePreview({ s }: { s: TableStyleInfo }) {
  const line = s.line || "#ffffff";
  return (
    <span className="ts-preview" style={{ borderColor: line }}>
      {[0, 1, 2, 3].map((r) => (
        <span key={r} className="ts-row" style={{ background: r === 0 ? s.header || "transparent" : r % 2 === 1 ? s.band || s.body || "transparent" : s.body || "transparent", borderColor: line }} />
      ))}
    </span>
  );
}

export function TableDesignTab({ status, styles, run }: { status: Status; styles: TableStyleInfo[]; run: (c: Command) => void }) {
  const [borderColor, setBorderColor] = useState("#000000");
  const [borderWidth, setBorderWidth] = useState(1);
  const flags = status.tableFlags ?? {};
  return (
    <>
      <Group label={t("table.styleOptions")}>
        <div className="ts-flags">
          {FLAGS.map(([key, label]) => (
            <label key={key} className="ts-flag" onMouseDown={keepFocus}>
              <input type="checkbox" checked={!!flags[key]} onChange={(e) => run({ kind: "tableFlag", flag: key, on: e.target.checked })} />
              {label}
            </label>
          ))}
        </div>
      </Group>
      <Group label={t("table.styles")}>
        <div className="ts-gallery">
          {styles.map((s) => (
            <button key={s.id} className={status.tableStyle === s.id ? "ts-item active" : "ts-item"} title={s.name} onMouseDown={keepFocus} onClick={() => run({ kind: "tableStyle", id: s.id })}>
              <StylePreview s={s} />
            </button>
          ))}
        </div>
      </Group>
      <Group label={t("table.shadingBorders")}>
        <div className="rb-col">
          <ColorPicker icon={icons.fill} label={t("table.shading")} noneLabel={t("table.noShading")} onPick={(color) => run({ kind: "cellFill", color })} />
          <Popover
            button={(open, toggle) => (
              <button className={open ? "rb-btn active" : "rb-btn"} title={t("table.borders")} onClick={toggle} onMouseDown={keepFocus}>
                {icons.borders}
                <span>{t("table.borders")}</span>
              </button>
            )}
          >
            {(close) => (
              <div className="border-panel">
                <div className="border-options">
                  <label className="rb-field">
                    {t("design.color")}
                    <input type="color" value={borderColor} onChange={(e) => setBorderColor(e.target.value)} />
                  </label>
                  <label className="rb-field">
                    {t("design.width")}
                    <select value={borderWidth} onChange={(e) => setBorderWidth(Number(e.target.value))}>
                      {[0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6].map((w) => (
                        <option key={w} value={w}>
                          {w} pt
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                {BORDERS.map(([which, label]) => (
                  <button
                    key={which}
                    className="border-item"
                    onMouseDown={keepFocus}
                    onClick={() => {
                      run({ kind: "cellBorders", which, color: borderColor, width: borderWidth });
                      close();
                    }}
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
          </Popover>
        </div>
      </Group>
    </>
  );
}

export function TableLayoutTab({ status, run }: { status: Status; run: (c: Command) => void }) {
  const [rows, setRows] = useState(1);
  const [cols, setCols] = useState(2);
  const table = (command: "rowAbove" | "rowBelow" | "colLeft" | "colRight" | "deleteRow" | "deleteCol") => run({ kind: "table", command });
  return (
    <>
      <Group label={t("table.rowsColumns")}>
        <Button large icon={icons.rowAbove} label={t("table.insertAbove")} onClick={() => table("rowAbove")} />
        <Button large icon={icons.rowBelow} label={t("table.insertBelow")} onClick={() => table("rowBelow")} />
        <Button large icon={icons.colLeft} label={t("table.insertLeft")} onClick={() => table("colLeft")} />
        <Button large icon={icons.colRight} label={t("table.insertRight")} onClick={() => table("colRight")} />
      </Group>
      <Group label={t("table.delete")}>
        <div className="rb-col">
          <Button icon={icons.deleteRow} label={t("table.deleteRows")} onClick={() => table("deleteRow")} />
          <Button icon={icons.deleteCol} label={t("table.deleteColumns")} onClick={() => table("deleteCol")} />
        </div>
      </Group>
      <Group label={t("table.merge")}>
        <Button large icon={icons.merge} label={t("table.mergeCells")} disabled={!status.canMerge} onClick={() => run({ kind: "tableMerge" })} />
        <Popover button={(open, toggle) => <Button large active={open} icon={icons.split} label={t("table.splitCells")} onClick={toggle} />}>
          {(close) => (
            <div className="split-panel">
              <label className="rb-field">
                {t("table.columns")}
                <input type="number" min={1} max={20} value={cols} onChange={(e) => setCols(Math.max(1, Number(e.target.value)))} />
              </label>
              <label className="rb-field">
                {t("table.rows")}
                <input type="number" min={1} max={20} value={rows} onChange={(e) => setRows(Math.max(1, Number(e.target.value)))} />
              </label>
              <button
                className="primary"
                onClick={() => {
                  run({ kind: "tableSplit", rows, cols });
                  close();
                }}
              >
                {t("common.ok")}
              </button>
            </div>
          )}
        </Popover>
      </Group>
      <Group label={t("group.align")}>
        <div className="rb-col">
          <Button icon={icons.alignTop} label={t("table.alignTop")} active={status.cellAnchor === "top"} onClick={() => run({ kind: "cellAnchor", anchor: "top" })} />
          <Button icon={icons.alignMiddle} label={t("table.centerVertically")} active={status.cellAnchor === "middle"} onClick={() => run({ kind: "cellAnchor", anchor: "middle" })} />
          <Button icon={icons.alignBottom} label={t("table.alignBottom")} active={status.cellAnchor === "bottom"} onClick={() => run({ kind: "cellAnchor", anchor: "bottom" })} />
        </div>
      </Group>
    </>
  );
}
