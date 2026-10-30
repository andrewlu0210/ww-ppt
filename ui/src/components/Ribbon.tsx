import { useEffect, useState, type ReactNode } from "react";
import { icons } from "./icons";
import { ColorPicker } from "./ColorPicker";
import { LayoutPicker } from "./LayoutPicker";
import { TablePicker } from "./TablePicker";
import { Popover } from "./Popover";
import { TableDesignTab, TableLayoutTab } from "./TableTabs";
import { TransitionTab } from "./TransitionTab";
import { AnimationTab } from "./AnimationTab";
import { LinkPicker } from "./LinkPicker";
import { FONT_SIZES, type Command, type TableStyleInfo } from "../lib/commands";
import type { Status } from "../lib/status";

const TABS = ["常用", "插入", "圖形格式", "轉場", "動畫", "投影片放映", "檢視"] as const;
/** Shown only while a table (or a cell in it) is the target, like PowerPoint's contextual tabs. */
const TABLE_TABS = ["表格設計", "版面配置"] as const;
type Tab = (typeof TABS)[number] | (typeof TABLE_TABS)[number];
const isTableTab = (t: Tab) => (TABLE_TABS as readonly string[]).includes(t);

const SHAPES: [string, string][] = [
  ["rect", "矩形"],
  ["roundRect", "圓角矩形"],
  ["ellipse", "橢圓"],
  ["triangle", "三角形"],
  ["rightArrow", "向右箭號"],
  ["star5", "星形"],
  ["line", "直線"],
];

/** Numbering schemes offered in the gallery, with a sample. */
const NUMBERINGS: [string, string][] = [
  ["arabicPeriod", "1. 2. 3."],
  ["arabicParenR", "1) 2) 3)"],
  ["romanUcPeriod", "I. II. III."],
  ["alphaUcPeriod", "A. B. C."],
  ["alphaLcParenR", "a) b) c)"],
  ["alphaLcPeriod", "a. b. c."],
  ["romanLcPeriod", "i. ii. iii."],
  ["circleNumDbPlain", "① ② ③"],
  ["ea1ChtPeriod", "一. 二. 三."],
];

/** Keeps keyboard focus on the canvas input while clicking ribbon controls. */
const keepFocus = (e: React.MouseEvent) => e.preventDefault();

function Button(props: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean; large?: boolean; active?: boolean; iconOnly?: boolean }) {
  const cls = ["rb-btn", props.large && "large", props.active && "active", props.iconOnly && "icon-only"].filter(Boolean).join(" ");
  return (
    <button className={cls} onClick={props.onClick} disabled={props.disabled} title={props.label} aria-pressed={props.active} onMouseDown={keepFocus}>
      {props.icon}
      {!props.iconOnly && <span>{props.label}</span>}
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

interface Props {
  status: Status;
  fonts: string[];
  layoutNames: string[];
  layoutThumbs: (ImageData | undefined)[];
  loadLayoutThumbs: () => void;
  run: (c: Command) => void;
  /** Bumped by the ⌘K menu item to open the link panel. */
  linkRequest: number;
  tableStyles: TableStyleInfo[];
  notesOpen: boolean;
  usePresenter: boolean;
}

export function Ribbon({ status, fonts, layoutNames, layoutThumbs, loadLayoutThumbs, run, linkRequest, tableStyles, notesOpen, usePresenter }: Props) {
  const [chosen, setTab] = useState<Tab>("常用");
  const tab = isTableTab(chosen) && !status.inTable ? "常用" : chosen;
  const tabs: Tab[] = status.inTable ? [...TABS, ...TABLE_TABS] : [...TABS];
  useEffect(() => {
    if (linkRequest > 0) setTab("插入");
  }, [linkRequest]);
  const text = status.hasText;
  const shapes = status.hasSelection && !status.editingText;
  const [sizeDraft, setSizeDraft] = useState<string | null>(null);
  const [fontDraft, setFontDraft] = useState<string | null>(null);

  const newSlide = (large: boolean) => (
    <LayoutPicker
      names={layoutNames}
      thumbs={layoutThumbs}
      onOpen={loadLayoutThumbs}
      onPick={(layout) => run({ kind: "newSlide", layout })}
      button={(open, toggle) => (
        <span className="split-btn">
          <Button large={large} icon={icons.newSlide} label="新增投影片" onClick={() => run({ kind: "newSlide" })} />
          <button className={open ? "split-arrow active" : "split-arrow"} title="選擇版面配置" onClick={toggle} onMouseDown={keepFocus}>
            ▾
          </button>
        </span>
      )}
    />
  );

  return (
    <div className="ribbon">
      <div className="rb-tabs" role="tablist">
        {tabs.map((t) => (
          <button key={t} role="tab" aria-selected={tab === t} className={[tab === t && "active", isTableTab(t) && "contextual"].filter(Boolean).join(" ")} onClick={() => setTab(t)} onMouseDown={keepFocus}>
            {t}
          </button>
        ))}
      </div>
      <div className="rb-body">
        {tab === "常用" && (
          <>
            <Group label="剪貼簿">
              <Button large icon={icons.paste} label="貼上" onClick={() => run({ kind: "paste" })} />
              <div className="rb-col">
                <Button icon={icons.cut} label="剪下" disabled={!status.hasSelection} onClick={() => run({ kind: "cut" })} />
                <Button icon={icons.copy} label="複製" disabled={!status.hasSelection} onClick={() => run({ kind: "copy" })} />
              </div>
            </Group>
            <Group label="投影片">{newSlide(true)}</Group>
            <Group label="字型">
              <div className="rb-col">
                <div className="rb-row">
                  <input
                    className="font-family"
                    list="font-list"
                    value={fontDraft ?? status.fontFamily ?? ""}
                    disabled={!text}
                    placeholder="字型"
                    onFocus={(e) => setFontDraft(e.target.value)}
                    onChange={(e) => {
                      setFontDraft(e.target.value);
                      // Picking from the list applies immediately.
                      if (fonts.includes(e.target.value)) {
                        run({ kind: "fontFamily", family: e.target.value });
                        setFontDraft(null);
                      }
                    }}
                    onBlur={() => setFontDraft(null)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && e.currentTarget.value) {
                        run({ kind: "fontFamily", family: e.currentTarget.value });
                        setFontDraft(null);
                      }
                    }}
                  />
                  <datalist id="font-list">
                    {fonts.map((f) => (
                      <option key={f} value={f} />
                    ))}
                  </datalist>
                  <input
                    className="font-size"
                    list="size-list"
                    disabled={!text}
                    value={sizeDraft ?? (status.fontSize ? String(Math.round(status.fontSize * 10) / 10) : "")}
                    onFocus={(e) => setSizeDraft(e.target.value)}
                    onChange={(e) => {
                      setSizeDraft(e.target.value);
                      const n = Number(e.target.value);
                      if (FONT_SIZES.includes(n)) {
                        run({ kind: "fontSize", size: n });
                        setSizeDraft(null);
                      }
                    }}
                    onBlur={() => setSizeDraft(null)}
                    onKeyDown={(e) => {
                      const n = Number(e.currentTarget.value);
                      if (e.key === "Enter" && n > 0) {
                        run({ kind: "fontSize", size: n });
                        setSizeDraft(null);
                        e.currentTarget.blur();
                      }
                    }}
                  />
                  <datalist id="size-list">
                    {FONT_SIZES.map((s) => (
                      <option key={s} value={s} />
                    ))}
                  </datalist>
                  <Button iconOnly icon={<b className="glyph">A⁺</b>} label="放大字型 (⌘])" disabled={!text} onClick={() => run({ kind: "fontStep", up: true })} />
                  <Button iconOnly icon={<b className="glyph">A⁻</b>} label="縮小字型 (⌘[)" disabled={!text} onClick={() => run({ kind: "fontStep", up: false })} />
                </div>
                <div className="rb-row">
                  <Button iconOnly icon={<b className="glyph">B</b>} label="粗體 (⌘B)" active={status.bold} disabled={!text} onClick={() => run({ kind: "toggle", which: "bold" })} />
                  <Button iconOnly icon={<i className="glyph serif">I</i>} label="斜體 (⌘I)" active={status.italic} disabled={!text} onClick={() => run({ kind: "toggle", which: "italic" })} />
                  <Button iconOnly icon={<u className="glyph">U</u>} label="底線 (⌘U)" active={status.underline} disabled={!text} onClick={() => run({ kind: "toggle", which: "underline" })} />
                  <ColorPicker icon={icons.fontColor} label="字型色彩" value={status.textColor} disabled={!text} onPick={(color) => color && run({ kind: "textColor", color })} />
                </div>
              </div>
            </Group>
            <Group label="段落">
              <div className="rb-col">
                <div className="rb-row">
                  <Button iconOnly icon={icons.bulletList} label="項目符號" active={status.bullet} disabled={!text} onClick={() => run({ kind: "bullets" })} />
                  <Popover
                    button={(open, toggle) => (
                      <span className="split-btn">
                        <Button iconOnly icon={icons.numberList} label="編號" active={!!status.numbering} disabled={!text} onClick={() => run({ kind: "numbering", scheme: status.numbering || "arabicPeriod" })} />
                        <button className={open ? "split-arrow active" : "split-arrow"} title="編號格式" disabled={!text} onClick={toggle} onMouseDown={keepFocus}>
                          ▾
                        </button>
                      </span>
                    )}
                  >
                    {(close) => (
                      <div className="numbering-panel">
                        {NUMBERINGS.map(([scheme, sample]) => (
                          <button
                            key={scheme}
                            className={status.numbering === scheme ? "numbering-item active" : "numbering-item"}
                            onMouseDown={keepFocus}
                            onClick={() => {
                              run({ kind: "numbering", scheme });
                              close();
                            }}
                          >
                            {sample}
                          </button>
                        ))}
                      </div>
                    )}
                  </Popover>
                  <Button iconOnly icon={icons.outdent} label="減少清單階層" disabled={!text} onClick={() => run({ kind: "level", delta: -1 })} />
                  <Button iconOnly icon={icons.indent} label="增加清單階層" disabled={!text} onClick={() => run({ kind: "level", delta: 1 })} />
                </div>
                <div className="rb-row">
                  {(
                    [
                      ["left", icons.alignLeft, "靠左對齊 (⌘L)"],
                      ["center", icons.alignCenter, "置中 (⌘E)"],
                      ["right", icons.alignRight, "靠右對齊 (⌘R)"],
                      ["justify", icons.alignJustify, "左右對齊 (⌘J)"],
                    ] as const
                  ).map(([a, icon, label]) => (
                    <Button key={a} iconOnly icon={icon} label={label} active={status.align === a} disabled={!text} onClick={() => run({ kind: "align", align: a })} />
                  ))}
                </div>
              </div>
            </Group>
            <Group label="繪圖">
              <div className="rb-col">
                <ColorPicker icon={icons.fill} label="圖案填滿" value={status.fill} disabled={!shapes} noneLabel="無填滿" onPick={(color) => run({ kind: "fill", color })} />
                <ColorPicker icon={icons.outline} label="圖案外框" value={status.lineColor} disabled={!shapes} noneLabel="無外框" onPick={(color) => run({ kind: "lineColor", color })} />
              </div>
            </Group>
            <Group label="編輯">
              <div className="rb-col">
                <Button icon={icons.undo} label="復原" disabled={!status.canUndo} onClick={() => run({ kind: "undo" })} />
                <Button icon={icons.redo} label="取消復原" disabled={!status.canRedo} onClick={() => run({ kind: "redo" })} />
              </div>
            </Group>
          </>
        )}
        {tab === "插入" && (
          <>
            <Group label="投影片">{newSlide(true)}</Group>
            <Group label="表格">
              <TablePicker
                onPick={(rows, cols) => run({ kind: "insertTable", rows, cols })}
                button={(open, toggle) => <Button large active={open} icon={icons.table} label="表格" onClick={toggle} />}
              />
            </Group>
            <Group label="影像">
              <Button large icon={icons.picture} label="圖片" onClick={() => run({ kind: "picture" })} />
            </Group>
            <Group label="文字">
              <Button large icon={icons.textbox} label="文字方塊" onClick={() => run({ kind: "textBox" })} />
            </Group>
            <Group label="連結">
              <LinkPicker
                current={status.link}
                disabled={!text}
                request={linkRequest}
                onApply={(url) => run({ kind: "link", url })}
                button={(open, toggle) => <Button large active={open} disabled={!text} icon={icons.link} label="連結 (⌘K)" onClick={toggle} />}
              />
            </Group>
            <Group label="圖例">
              {SHAPES.map(([preset, label]) => (
                <Button key={preset} icon={icons[preset]} label={label} onClick={() => run({ kind: "shape", preset })} />
              ))}
            </Group>
          </>
        )}
        {tab === "圖形格式" && (
          <>
            <Group label="圖形樣式">
              <div className="rb-col">
                <ColorPicker icon={icons.fill} label="圖案填滿" value={status.fill} disabled={!shapes} noneLabel="無填滿" onPick={(color) => run({ kind: "fill", color })} />
                <ColorPicker icon={icons.outline} label="圖案外框" value={status.lineColor} disabled={!shapes} noneLabel="無外框" onPick={(color) => run({ kind: "lineColor", color })} />
              </div>
              <div className="rb-col">
                <label className="rb-field">
                  外框寬度
                  <select
                    disabled={!shapes}
                    value={status.lineWidth ?? ""}
                    onChange={(e) => run({ kind: "lineWidth", width: Number(e.target.value) })}
                    onMouseDown={(e) => e.stopPropagation()}
                  >
                    <option value="" disabled>
                      —
                    </option>
                    {[0.25, 0.5, 0.75, 1, 1.5, 2.25, 3, 4.5, 6].map((w) => (
                      <option key={w} value={w}>
                        {w} pt
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </Group>
            <Group label="排列">
              <div className="rb-col">
                <Button icon={icons.front} label="移到最上層" disabled={!shapes} onClick={() => run({ kind: "reorder", order: "front" })} />
                <Button icon={icons.back} label="移到最下層" disabled={!shapes} onClick={() => run({ kind: "reorder", order: "back" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.front} label="上移一層" disabled={!shapes} onClick={() => run({ kind: "reorder", order: "forward" })} />
                <Button icon={icons.back} label="下移一層" disabled={!shapes} onClick={() => run({ kind: "reorder", order: "backward" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.group} label="組成群組 (⌘⌥G)" disabled={!status.canGroup} onClick={() => run({ kind: "group", on: true })} />
                <Button icon={icons.ungroup} label="取消群組 (⌘⌥⇧G)" disabled={!status.canUngroup} onClick={() => run({ kind: "group", on: false })} />
              </div>
            </Group>
            <Group label="對齊">
              {(
                [
                  ["left", "靠左"],
                  ["center", "水平置中"],
                  ["right", "靠右"],
                  ["top", "靠上"],
                  ["middle", "垂直置中"],
                  ["bottom", "靠下"],
                ] as const
              ).map(([to, label]) => (
                <Button key={to} icon={icons.alignShapes} label={label} disabled={!shapes} onClick={() => run({ kind: "alignShapes", to })} />
              ))}
            </Group>
          </>
        )}
        {tab === "表格設計" && <TableDesignTab status={status} styles={tableStyles} run={run} />}
        {tab === "版面配置" && <TableLayoutTab status={status} run={run} />}
        {tab === "轉場" && <TransitionTab status={status} run={run} />}
        {tab === "動畫" && <AnimationTab status={status} run={run} />}
        {tab === "投影片放映" && (
          <>
            <Group label="開始投影片放映">
              <Button icon={icons.playStart} label="從首張投影片" disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: true })} />
              <Button icon={icons.playCurrent} label="從目前投影片" disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false })} />
              <Button icon={icons.presenter} label="簡報者檢視" disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false, presenter: true })} />
            </Group>
            <Group label="螢幕">
              <label className="rb-check" title="接上第二個螢幕時，投影片在外接螢幕放映，這個螢幕顯示簡報者檢視">
                <input type="checkbox" checked={usePresenter} onChange={(e) => run({ kind: "usePresenter", on: e.target.checked })} />
                使用簡報者檢視
              </label>
            </Group>
          </>
        )}
        {tab === "檢視" && (
          <>
            <Group label="顯示">
              <Button icon={icons.notes} label="備忘稿" active={notesOpen} onClick={() => run({ kind: "toggleNotes" })} />
            </Group>
            <Group label="縮放">
              <Button icon={icons.zoomIn} label="放大" onClick={() => run({ kind: "zoom", factor: 1.25 })} />
              <Button icon={icons.zoomOut} label="縮小" onClick={() => run({ kind: "zoom", factor: 0.8 })} />
              <Button icon={icons.fit} label="符合視窗" onClick={() => run({ kind: "zoom", factor: "fit" })} />
            </Group>
          </>
        )}
      </div>
    </div>
  );
}
