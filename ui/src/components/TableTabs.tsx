// Contextual ribbon tabs for tables: 表格設計 (style, shading, borders) and 版面配置 (rows, merge, alignment).
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
    <div className="rb-group">
      <div className="rb-items">{children}</div>
      <div className="rb-label">{label}</div>
    </div>
  );
}

const FLAGS: [string, string][] = [
  ["firstRow", "標題列"],
  ["firstCol", "首欄"],
  ["lastRow", "合計列"],
  ["lastCol", "末欄"],
  ["bandRow", "帶狀列"],
  ["bandCol", "帶狀欄"],
];

const BORDERS: [string, string][] = [
  ["all", "所有框線"],
  ["outside", "外框線"],
  ["inside", "內框線"],
  ["top", "上框線"],
  ["bottom", "下框線"],
  ["left", "左框線"],
  ["right", "右框線"],
  ["none", "無框線"],
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
      <Group label="表格樣式選項">
        <div className="ts-flags">
          {FLAGS.map(([key, label]) => (
            <label key={key} className="ts-flag" onMouseDown={keepFocus}>
              <input type="checkbox" checked={!!flags[key]} onChange={(e) => run({ kind: "tableFlag", flag: key, on: e.target.checked })} />
              {label}
            </label>
          ))}
        </div>
      </Group>
      <Group label="表格樣式">
        <div className="ts-gallery">
          {styles.map((s) => (
            <button key={s.id} className={status.tableStyle === s.id ? "ts-item active" : "ts-item"} title={s.name} onMouseDown={keepFocus} onClick={() => run({ kind: "tableStyle", id: s.id })}>
              <StylePreview s={s} />
            </button>
          ))}
        </div>
      </Group>
      <Group label="網底與框線">
        <div className="rb-col">
          <ColorPicker icon={icons.fill} label="網底" noneLabel="無網底" onPick={(color) => run({ kind: "cellFill", color })} />
          <Popover
            button={(open, toggle) => (
              <button className={open ? "rb-btn active" : "rb-btn"} title="框線" onClick={toggle} onMouseDown={keepFocus}>
                {icons.borders}
                <span>框線</span>
              </button>
            )}
          >
            {(close) => (
              <div className="border-panel">
                <div className="border-options">
                  <label className="rb-field">
                    色彩
                    <input type="color" value={borderColor} onChange={(e) => setBorderColor(e.target.value)} />
                  </label>
                  <label className="rb-field">
                    寬度
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
      <Group label="列與欄">
        <Button large icon={icons.rowAbove} label="插入上方列" onClick={() => table("rowAbove")} />
        <Button large icon={icons.rowBelow} label="插入下方列" onClick={() => table("rowBelow")} />
        <Button large icon={icons.colLeft} label="插入左方欄" onClick={() => table("colLeft")} />
        <Button large icon={icons.colRight} label="插入右方欄" onClick={() => table("colRight")} />
      </Group>
      <Group label="刪除">
        <div className="rb-col">
          <Button icon={icons.deleteRow} label="刪除列" onClick={() => table("deleteRow")} />
          <Button icon={icons.deleteCol} label="刪除欄" onClick={() => table("deleteCol")} />
        </div>
      </Group>
      <Group label="合併">
        <Button large icon={icons.merge} label="合併儲存格" disabled={!status.canMerge} onClick={() => run({ kind: "tableMerge" })} />
        <Popover button={(open, toggle) => <Button large active={open} icon={icons.split} label="分割儲存格" onClick={toggle} />}>
          {(close) => (
            <div className="split-panel">
              <label className="rb-field">
                欄數
                <input type="number" min={1} max={20} value={cols} onChange={(e) => setCols(Math.max(1, Number(e.target.value)))} />
              </label>
              <label className="rb-field">
                列數
                <input type="number" min={1} max={20} value={rows} onChange={(e) => setRows(Math.max(1, Number(e.target.value)))} />
              </label>
              <button
                className="primary"
                onClick={() => {
                  run({ kind: "tableSplit", rows, cols });
                  close();
                }}
              >
                確定
              </button>
            </div>
          )}
        </Popover>
      </Group>
      <Group label="對齊">
        <div className="rb-col">
          <Button icon={icons.alignTop} label="靠上對齊" active={status.cellAnchor === "top"} onClick={() => run({ kind: "cellAnchor", anchor: "top" })} />
          <Button icon={icons.alignMiddle} label="垂直置中" active={status.cellAnchor === "middle"} onClick={() => run({ kind: "cellAnchor", anchor: "middle" })} />
          <Button icon={icons.alignBottom} label="靠下對齊" active={status.cellAnchor === "bottom"} onClick={() => run({ kind: "cellAnchor", anchor: "bottom" })} />
        </div>
      </Group>
    </>
  );
}
