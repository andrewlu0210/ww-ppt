import { useState, type ReactNode } from "react";
import type { Command } from "../lib/commands";
import type { Status } from "../lib/status";
import { icons } from "./icons";
import { Popover } from "./Popover";

const EMU_PER_CM = 360_000;
const keepFocus = (e: React.MouseEvent) => e.preventDefault();

/** A number box that applies on Enter or when it loses focus (not on every keystroke). */
function NumberField({ label, value, unit, step, disabled, onCommit }: { label: string; value: number | undefined; unit: string; step: number; disabled?: boolean; onCommit: (v: number) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const shown = value === undefined ? "" : String(Math.round(value * 100) / 100);
  const commit = () => {
    if (draft === null) return;
    const v = Number(draft);
    setDraft(null);
    if (draft.trim() !== "" && Number.isFinite(v) && v !== value) onCommit(v);
  };
  return (
    <label className="rb-field num-field">
      {label}
      <input
        type="number"
        step={step}
        disabled={disabled}
        value={draft ?? shown}
        onFocus={(e) => setDraft(e.target.value)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            setDraft(null);
            e.currentTarget.blur();
          }
        }}
      />
      <span className="num-unit">{unit}</span>
    </label>
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

/** Size, position and rotation of the selection, in centimeters and degrees. */
export function SizeGroup({ status, run }: { status: Status; run: (c: Command) => void }) {
  const f = status.frame;
  const cm = (emu: number | undefined) => (emu === undefined ? undefined : emu / EMU_PER_CM);
  const set = (patch: Partial<{ x: number; y: number; w: number; h: number; rot: number }>) => {
    if (!f) return;
    const next = { x: f.x, y: f.y, w: f.w, h: f.h, rot: f.rot, ...patch };
    run({ kind: "setFrame", ...next });
  };
  return (
    <Group label="大小與位置">
      <div className="rb-col">
        <NumberField label="高度" unit="公分" step={0.1} value={cm(f?.h)} disabled={!f} onCommit={(v) => set({ h: Math.max(1, v * EMU_PER_CM) })} />
        <NumberField label="寬度" unit="公分" step={0.1} value={cm(f?.w)} disabled={!f} onCommit={(v) => set({ w: Math.max(1, v * EMU_PER_CM) })} />
        <NumberField label="旋轉" unit="°" step={1} value={f?.single ? f.rot : undefined} disabled={!f?.single} onCommit={(v) => set({ rot: v })} />
      </div>
      <div className="rb-col">
        <NumberField label="水平" unit="公分" step={0.1} value={cm(f?.x)} disabled={!f} onCommit={(v) => set({ x: v * EMU_PER_CM })} />
        <NumberField label="垂直" unit="公分" step={0.1} value={cm(f?.y)} disabled={!f} onCommit={(v) => set({ y: v * EMU_PER_CM })} />
      </div>
    </Group>
  );
}

const ROTATIONS: [string, string, Command][] = [
  ["rotateRight", "向右旋轉 90°", { kind: "rotate", degrees: 90 }],
  ["rotateLeft", "向左旋轉 90°", { kind: "rotate", degrees: -90 }],
  ["flipV", "垂直翻轉", { kind: "flip", horizontal: false }],
  ["flipH", "水平翻轉", { kind: "flip", horizontal: true }],
];

/** Rotate and flip menu. */
export function RotateMenu({ disabled, run }: { disabled: boolean; run: (c: Command) => void }) {
  return (
    <Popover
      button={(open, toggle) => (
        <button className={open ? "rb-btn active" : "rb-btn"} disabled={disabled} onMouseDown={keepFocus} onClick={toggle} title="旋轉或翻轉">
          {icons.rotateRight}
          <span>旋轉 ▾</span>
        </button>
      )}
    >
      {(close) => (
        <div className="menu-panel">
          {ROTATIONS.map(([icon, label, c]) => (
            <button
              key={icon}
              className="menu-item"
              onMouseDown={keepFocus}
              onClick={() => {
                run(c);
                close();
              }}
            >
              <span className="menu-icon">{icons[icon]}</span>
              {label}
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}

const SHADOWS: [string, string][] = [
  ["none", "無陰影"],
  ["bottomRight", "右下方位移"],
  ["bottom", "下方位移"],
  ["right", "右方位移"],
  ["bottomLeft", "左下方位移"],
  ["center", "置中（柔邊）"],
];

/** Shadow presets (outer shadows). */
export function ShadowMenu({ status, disabled, run }: { status: Status; disabled: boolean; run: (c: Command) => void }) {
  return (
    <Popover
      button={(open, toggle) => (
        <button className={open ? "rb-btn active" : "rb-btn"} disabled={disabled} onMouseDown={keepFocus} onClick={toggle} title="圖案效果：陰影">
          {icons.shadow}
          <span>陰影 ▾</span>
        </button>
      )}
    >
      {(close) => (
        <div className="menu-panel">
          {SHADOWS.map(([preset, label]) => (
            <button
              key={preset}
              className={status.shadow === preset ? "menu-item active" : "menu-item"}
              onMouseDown={keepFocus}
              onClick={() => {
                run({ kind: "shadow", preset });
                close();
              }}
            >
              {label}
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}

/** Contextual tab for a selected picture: crop and reset. */
export function PictureFormatTab({ status, run }: { status: Status; run: (c: Command) => void }) {
  const c = status.crop;
  const pct = (v: number | undefined) => (v === undefined ? undefined : v * 100);
  const set = (side: "left" | "top" | "right" | "bottom", v: number) => {
    if (!c) return;
    const next = { ...c, [side]: Math.min(99, Math.max(0, v)) / 100 };
    run({ kind: "crop", ...next });
  };
  return (
    <>
      <Group label="調整">
        <button className="rb-btn large" disabled={!c} onMouseDown={keepFocus} onClick={() => run({ kind: "resetPicture" })} title="移除裁剪、旋轉和翻轉，回到圖片原本的比例">
          {icons.resetPicture}
          <span>重設圖片</span>
        </button>
      </Group>
      <Group label="大小">
        <button className={status.cropMode ? "rb-btn large active" : "rb-btn large"} aria-pressed={!!status.cropMode} disabled={!c} onMouseDown={keepFocus} onClick={() => run({ kind: "cropMode", on: !status.cropMode })} title="拖曳控點裁剪圖片；按 Enter 或 Esc 結束">
          {icons.crop}
          <span>裁剪</span>
        </button>
        <div className="rb-col">
          <NumberField label="左" unit="%" step={1} value={pct(c?.left)} disabled={!c} onCommit={(v) => set("left", v)} />
          <NumberField label="右" unit="%" step={1} value={pct(c?.right)} disabled={!c} onCommit={(v) => set("right", v)} />
        </div>
        <div className="rb-col">
          <NumberField label="上" unit="%" step={1} value={pct(c?.top)} disabled={!c} onCommit={(v) => set("top", v)} />
          <NumberField label="下" unit="%" step={1} value={pct(c?.bottom)} disabled={!c} onCommit={(v) => set("bottom", v)} />
        </div>
      </Group>
    </>
  );
}
