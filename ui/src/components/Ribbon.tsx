import { useEffect, useState, type ReactNode } from "react";
import { icons } from "./icons";
import { ColorPicker } from "./ColorPicker";
import { LayoutPicker } from "./LayoutPicker";
import { TablePicker } from "./TablePicker";
import { Popover } from "./Popover";
import { TableDesignTab, TableLayoutTab } from "./TableTabs";
import { TransitionTab } from "./TransitionTab";
import { AnimationTab } from "./AnimationTab";
import { PictureFormatTab, RotateMenu, ShadowMenu, SizeGroup } from "./FormatGroups";
import { ShapeGallery, type PresetIcon } from "./ShapeGallery";
import { HeaderFooterDialog, type HeaderFooterInfo } from "./HeaderFooterDialog";
import { SymbolPicker } from "./SymbolPicker";
import { AltTextDialog, type AltTextInfo } from "./AltTextDialog";
import { DesignTab, type BackgroundInfo, type ColorSchemeInfo, type FontSchemeInfo, type ThemeInfo } from "./DesignTab";
import { LinkPicker } from "./LinkPicker";
import { focusables, moveFocus, ownsArrows } from "../lib/keyboard";
import { focusCanvas } from "./CanvasHost";
import { FONT_SIZES, type Command, type TableStyleInfo } from "../lib/commands";
import type { AnimationPreset, Status } from "../lib/status";

/** Character spacing presets (points), as PowerPoint names them. */
const CHAR_SPACINGS: [number, string][] = [
  [-3, "非常緊"],
  [-1.5, "緊"],
  [0, "標準"],
  [3, "鬆"],
  [6, "非常鬆"],
];
const LINE_SPACINGS = [1, 1.5, 2, 2.5, 3];
const ANCHORS = [
  ["top", "anchorTop", "上"],
  ["middle", "anchorMiddle", "中"],
  ["bottom", "anchorBottom", "下"],
] as const;

const TABS = ["常用", "插入", "設計", "圖形格式", "轉場", "動畫", "投影片放映", "校閱", "檢視"] as const;
/** Shown only while a table (or a cell in it) is the target, like PowerPoint's contextual tabs. */
const TABLE_TABS = ["表格設計", "版面配置"] as const;
const PICTURE_TAB = "圖片格式" as const;
/** Shown while a SmartArt is selected. */
const SMARTART_TAB = "SmartArt 設計" as const;
type Tab = (typeof TABS)[number] | (typeof TABLE_TABS)[number] | typeof PICTURE_TAB | typeof SMARTART_TAB;
const isTableTab = (t: Tab) => (TABLE_TABS as readonly string[]).includes(t);

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
    <button className={cls} onClick={props.onClick} disabled={props.disabled} title={props.label} aria-label={props.iconOnly ? props.label : undefined} aria-pressed={props.active} onMouseDown={keepFocus}>
      {props.icon}
      {!props.iconOnly && <span>{props.label}</span>}
    </button>
  );
}

function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="rb-group" role="group" aria-label={label}>
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
  /** Spell checking as you type. */
  spellOn: boolean;
  animPresets: AnimationPreset[];
  /** Told which tab is showing (the canvas shows animation order on 動畫). */
  onTabChange: (tab: string) => void;
  shapeIcons: Map<string, PresetIcon>;
  /** The current slide's footer items, read when the dialog opens. */
  loadHeaderFooter: () => HeaderFooterInfo | null;
  theme: ThemeInfo | null;
  colorSchemes: ColorSchemeInfo[];
  fontSchemes: FontSchemeInfo[];
  slideSize: [number, number];
  loadBackground: () => BackgroundInfo | null;
  viewMode: "normal" | "sorter";
  rulerOn: boolean;
  paneOpen: boolean;
  checkerOpen: boolean;
  /** The SmartArt text pane is showing. */
  textPaneOpen: boolean;
  /** The selected object's alternative text, read when the dialog opens. */
  loadAltText: () => AltTextInfo | null;
}

export function Ribbon({ status, fonts, layoutNames, layoutThumbs, loadLayoutThumbs, run, linkRequest, tableStyles, notesOpen, usePresenter, spellOn, animPresets, onTabChange, shapeIcons, loadHeaderFooter, theme, colorSchemes, fontSchemes, slideSize, loadBackground, viewMode, rulerOn, paneOpen, checkerOpen, textPaneOpen, loadAltText }: Props) {
  const [chosen, setTab] = useState<Tab>("常用");
  const tab = (isTableTab(chosen) && !status.inTable) || (chosen === PICTURE_TAB && !status.crop) || (chosen === SMARTART_TAB && !status.smartArt) ? "常用" : chosen;
  useEffect(() => onTabChange(tab), [tab, onTabChange]);
  const tabs: Tab[] = [...TABS, ...(status.inTable ? TABLE_TABS : []), ...(status.crop ? [PICTURE_TAB] : []), ...(status.smartArt ? [SMARTART_TAB] : [])];
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

  // 協助工具: alternative text of the selected object, and the selection pane.
  const accessGroup = (
    <Group label="協助工具">
      <AltTextDialog
        load={loadAltText}
        run={run}
        button={(open, toggle) => <Button large icon={icons.altText} label="替代文字" active={open} disabled={!status.frame || status.selectionCount !== 1} onClick={toggle} />}
      />
      <Button large icon={icons.selectionPane} label="選取窗格" active={paneOpen} onClick={() => run({ kind: "selectionPane" })} />
    </Group>
  );

  // Keyboard: ←/→ switch tabs on the tab row and move between controls in the
  // body; ↓/Enter goes from the tab row into the body, Esc goes back up and then
  // to the slide. Tab/Shift+Tab also move between controls (WebKit skips buttons).
  const onKeyDown = (e: React.KeyboardEvent) => {
    const el = e.target as HTMLElement;
    if (el.closest(".popover")) return;
    const ribbon = e.currentTarget;
    const tabsRow = ribbon.querySelector(".rb-tabs");
    const body = ribbon.querySelector(".rb-body");
    const activeTab = () => ribbon.querySelector<HTMLElement>(".rb-tabs [aria-selected='true']");
    if (tabsRow?.contains(el)) {
      const buttons = focusables(tabsRow);
      const at = buttons.indexOf(el);
      if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        const next = buttons[(at + (e.key === "ArrowLeft" ? -1 : 1) + buttons.length) % buttons.length];
        next.focus();
        next.click();
      } else if (e.key === "ArrowDown" || e.key === "Enter" || (e.key === "Tab" && !e.shiftKey)) {
        focusables(body)[0]?.focus();
      } else if (e.key === "Escape") {
        focusCanvas();
      } else return;
      e.preventDefault();
      return;
    }
    if (!body?.contains(el)) return;
    const items = focusables(body);
    if (e.key === "Tab" || ((e.key === "ArrowLeft" || e.key === "ArrowRight") && !ownsArrows(el))) {
      moveFocus(items, e.key === "ArrowLeft" || (e.key === "Tab" && e.shiftKey) ? -1 : 1);
    } else if (e.key === "Escape" || (e.key === "ArrowUp" && !ownsArrows(el))) {
      activeTab()?.focus();
    } else return;
    e.preventDefault();
  };

  return (
    <div className="ribbon" onKeyDown={onKeyDown}>
      <div className="rb-tabs" role="tablist">
        {tabs.map((t) => (
          <button key={t} role="tab" aria-selected={tab === t} className={[tab === t && "active", (isTableTab(t) || t === PICTURE_TAB || t === SMARTART_TAB) && "contextual"].filter(Boolean).join(" ")} onClick={() => setTab(t)} onMouseDown={keepFocus}>
            {t}
          </button>
        ))}
      </div>
      <div className="rb-body" role="tabpanel" aria-label={tab}>
        {tab === "常用" && (
          <>
            <Group label="剪貼簿">
              <Button large icon={icons.paste} label="貼上" onClick={() => run({ kind: "paste" })} />
              <div className="rb-col">
                <Button icon={icons.cut} label="剪下" disabled={!status.hasSelection} onClick={() => run({ kind: "cut" })} />
                <Button icon={icons.copy} label="複製" disabled={!status.hasSelection} onClick={() => run({ kind: "copy" })} />
              </div>
            </Group>
            <Group label="投影片">
              {newSlide(true)}
              <div className="rb-col">
                <LayoutPicker
                  names={layoutNames}
                  thumbs={layoutThumbs}
                  current={status.slideLayout}
                  onOpen={loadLayoutThumbs}
                  onPick={(layout) => run({ kind: "applyLayout", layout })}
                  button={(open, toggle) => <Button icon={icons.layout} label="版面配置 ▾" active={open} onClick={toggle} />}
                />
                <Button icon={icons.resetSlide} label="重設" disabled={(status.slideLayout ?? -1) < 0} onClick={() => run({ kind: "resetSlide" })} />
              </div>
            </Group>
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
                  <Button iconOnly icon={<s className="glyph">S</s>} label="刪除線" active={status.strike} disabled={!text} onClick={() => run({ kind: "toggle", which: "strike" })} />
                  <Button iconOnly icon={<span className="glyph">x²</span>} label="上標" active={(status.baseline ?? 0) > 0} disabled={!text} onClick={() => run({ kind: "toggle", which: "superscript" })} />
                  <Button iconOnly icon={<span className="glyph">x₂</span>} label="下標" active={(status.baseline ?? 0) < 0} disabled={!text} onClick={() => run({ kind: "toggle", which: "subscript" })} />
                  <Popover
                    button={(open, toggle) => <Button iconOnly icon={icons.charSpacing} label="字元間距" active={open} disabled={!text} onClick={toggle} />}
                  >
                    {(close) => (
                      <div className="menu-panel">
                        {CHAR_SPACINGS.map(([pt, label]) => (
                          <button
                            key={pt}
                            className={Math.abs((status.charSpacing ?? 0) - pt) < 0.01 ? "menu-item active" : "menu-item"}
                            onMouseDown={keepFocus}
                            onClick={() => {
                              run({ kind: "charSpacing", pt });
                              close();
                            }}
                          >
                            {label}
                          </button>
                        ))}
                      </div>
                    )}
                  </Popover>
                  <Button iconOnly icon={icons.clearFormat} label="清除所有格式設定" disabled={!text} onClick={() => run({ kind: "clearFormatting" })} />
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
                  <Popover button={(open, toggle) => <Button iconOnly icon={icons.lineSpacing} label="行距" active={open} disabled={!text} onClick={toggle} />}>
                    {(close) => (
                      <div className="menu-panel">
                        {LINE_SPACINGS.map((m) => (
                          <button
                            key={m}
                            className={Math.abs((status.lineSpacing ?? 0) - m) < 0.01 ? "menu-item active" : "menu-item"}
                            onMouseDown={keepFocus}
                            onClick={() => {
                              run({ kind: "lineSpacing", multiple: m });
                              close();
                            }}
                          >
                            {m.toFixed(1)}
                          </button>
                        ))}
                      </div>
                    )}
                  </Popover>
                  <Popover
                    button={(open, toggle) => (
                      <Button iconOnly icon={status.textAnchor === "middle" ? icons.anchorMiddle : status.textAnchor === "bottom" ? icons.anchorBottom : icons.anchorTop} label="對齊文字" active={open} disabled={!status.textAnchor} onClick={toggle} />
                    )}
                  >
                    {(close) => (
                      <div className="menu-panel">
                        {ANCHORS.map(([a, icon, label]) => (
                          <button
                            key={a}
                            className={status.textAnchor === a ? "menu-item active" : "menu-item"}
                            onMouseDown={keepFocus}
                            onClick={() => {
                              run({ kind: "textAnchor", anchor: a });
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
              <div className="rb-col">
                <Button icon={icons.find} label="尋找 (⌘F)" onClick={() => run({ kind: "find", replace: false })} />
                <Button icon={icons.replace} label="取代 (⌥⌘F)" onClick={() => run({ kind: "find", replace: true })} />
                <Button icon={icons.selectAll} label="全選 (⌘A)" onClick={() => run({ kind: "selectAll" })} />
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
            <Group label="圖例">
              <ShapeGallery
                icons={shapeIcons}
                active={status.drawTool}
                onPick={(tool) => run({ kind: "drawTool", tool })}
                button={(open, toggle) => <Button large active={open || !!status.drawTool} icon={icons.shapes} label="圖案" onClick={toggle} />}
              />
            </Group>
            <Group label="文字">
              <Button large icon={icons.textbox} label="文字方塊" onClick={() => run({ kind: "textBox" })} />
              <div className="rb-col">
                <HeaderFooterDialog
                  load={loadHeaderFooter}
                  onApply={(c) => run({ kind: "headerFooter", ...c })}
                  button={(open, toggle) => <Button icon={icons.headerFooter} label="頁首及頁尾" active={open} onClick={toggle} />}
                />
                <SymbolPicker onPick={(text) => run({ kind: "insertSymbol", text })} button={(open, toggle) => <Button icon={icons.symbol} label="符號" active={open} onClick={toggle} />} />
              </div>
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
            <Group label="圖案">
              <ShapeGallery
                icons={shapeIcons}
                lines={false}
                onPick={(preset) => run({ kind: "changeShape", preset })}
                button={(open, toggle) => (
                  <button className={open ? "rb-btn active" : "rb-btn"} disabled={!status.canChangeShape} onMouseDown={keepFocus} onClick={toggle} title="變更圖案">
                    {icons.changeShape}
                    <span>變更圖案 ▾</span>
                  </button>
                )}
              />
              <ShadowMenu status={status} disabled={!shapes || status.shadow === undefined} run={run} />
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
              <div className="rb-col">
                <RotateMenu disabled={!status.frame} run={run} />
              </div>
            </Group>
            {accessGroup}
            <SizeGroup status={status} run={run} />
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
        {tab === "設計" && <DesignTab theme={theme} colorSchemes={colorSchemes} fontSchemes={fontSchemes} slideSize={slideSize} loadBackground={loadBackground} run={run} />}
        {tab === "表格設計" && <TableDesignTab status={status} styles={tableStyles} run={run} />}
        {tab === "版面配置" && <TableLayoutTab status={status} run={run} />}
        {tab === "轉場" && <TransitionTab status={status} run={run} />}
        {tab === PICTURE_TAB && (
          <>
            <PictureFormatTab status={status} run={run} />
            {accessGroup}
          </>
        )}
        {tab === "動畫" && <AnimationTab status={status} run={run} presets={animPresets} />}
        {tab === SMARTART_TAB && (
          <>
            <Group label="建立圖形">
              <Button large icon={icons.addShapeAfter} label="在後面新增圖案" onClick={() => run({ kind: "smartArt", command: "addAfter" })} />
              <div className="rb-col">
                <Button icon={icons.addShapeBefore} label="在前面新增圖案" onClick={() => run({ kind: "smartArt", command: "addBefore" })} />
                <Button icon={icons.addChild} label="新增下層圖案" onClick={() => run({ kind: "smartArt", command: "addChild" })} />
                <Button icon={icons.textPane} label="文字窗格" active={textPaneOpen} onClick={() => run({ kind: "textPane" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.outdent} label="升階" onClick={() => run({ kind: "smartArt", command: "promote" })} />
                <Button icon={icons.indent} label="降階" onClick={() => run({ kind: "smartArt", command: "demote" })} />
                <Button icon={icons.delete} label="刪除圖案" onClick={() => run({ kind: "smartArt", command: "delete" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.moveUp} label="上移" onClick={() => run({ kind: "smartArt", command: "moveUp" })} />
                <Button icon={icons.moveDown} label="下移" onClick={() => run({ kind: "smartArt", command: "moveDown" })} />
              </div>
            </Group>
            {accessGroup}
          </>
        )}
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
            <Group label="匯出">
              <Button large icon={icons.exportVideo} label="匯出為視訊" disabled={status.slideCount === 0} onClick={() => run({ kind: "exportVideo" })} />
            </Group>
          </>
        )}
        {tab === "校閱" && (
          <>
            <Group label="校訂">
              <Button large icon={icons.spelling} label="拼字檢查" active={spellOn} onClick={() => run({ kind: "spellCheck", on: !spellOn })} />
            </Group>
            <Group label="協助工具">
              <Button large icon={icons.checkAccess} label="檢查協助工具" active={checkerOpen} onClick={() => run({ kind: "checker" })} />
              <AltTextDialog
                load={loadAltText}
                run={run}
                button={(open, toggle) => <Button large icon={icons.altText} label="替代文字" active={open} disabled={!status.frame || status.selectionCount !== 1} onClick={toggle} />}
              />
            </Group>
          </>
        )}
        {tab === "檢視" && (
          <>
            <Group label="簡報檢視">
              <Button large icon={icons.normalView} label="標準" active={viewMode === "normal"} onClick={() => run({ kind: "viewMode", mode: "normal" })} />
              <Button large icon={icons.sorterView} label="投影片瀏覽" active={viewMode === "sorter"} onClick={() => run({ kind: "viewMode", mode: "sorter" })} />
            </Group>
            <Group label="顯示">
              <div className="rb-checks">
                <label className="rb-check">
                  <input type="checkbox" checked={rulerOn} onChange={() => run({ kind: "toggleRuler" })} />
                  尺規
                </label>
                <label className="rb-check">
                  <input type="checkbox" checked={!!status.showGrid} onChange={(e) => run({ kind: "showGrid", on: e.target.checked })} />
                  格線
                </label>
                <label className="rb-check">
                  <input type="checkbox" checked={!!status.showGuides} onChange={(e) => run({ kind: "showGuides", on: e.target.checked })} />
                  輔助線
                </label>
              </div>
              <GridMenu status={status} run={run} />
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

/** Grid spacings offered, in centimeters. */
const GRID_SPACINGS = [0.1, 0.2, 0.25, 0.5, 1, 2];
const EMU_PER_CM = 360_000;

/** 格線及輔助線: spacing, snapping and new guides. */
function GridMenu({ status, run }: { status: Status; run: (c: Command) => void }) {
  const spacing = (status.gridSpacing ?? 72_000) / EMU_PER_CM;
  return (
    <Popover
      button={(open, toggle) => (
        <button className={open ? "rb-btn active" : "rb-btn"} title="格線及輔助線" onMouseDown={keepFocus} onClick={toggle}>
          {icons.grid}
          <span>格線設定 ▾</span>
        </button>
      )}
    >
      {(close) => (
        <div className="hf-dialog grid-menu" role="dialog" aria-label="格線及輔助線">
          <div className="hf-title">貼齊</div>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.snapToGrid} onChange={(e) => run({ kind: "snapToGrid", on: e.target.checked })} />
            物件貼齊格線
          </label>
          <div className="hf-title sep">格線設定</div>
          <label className="hf-row">
            間距
            <select value={GRID_SPACINGS.find((g) => Math.abs(g - spacing) < 0.005) ?? ""} onChange={(e) => run({ kind: "gridSpacing", emu: Number(e.target.value) * EMU_PER_CM })}>
              {!GRID_SPACINGS.some((g) => Math.abs(g - spacing) < 0.005) && <option value="">{spacing.toFixed(2)} 公分</option>}
              {GRID_SPACINGS.map((g) => (
                <option key={g} value={g}>
                  {g} 公分
                </option>
              ))}
            </select>
          </label>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.showGrid} onChange={(e) => run({ kind: "showGrid", on: e.target.checked })} />
            在螢幕上顯示格線
          </label>
          <div className="hf-title sep">輔助線設定</div>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.showGuides} onChange={(e) => run({ kind: "showGuides", on: e.target.checked })} />
            在螢幕上顯示繪圖輔助線
          </label>
          <div className="hf-buttons">
            <button onClick={() => run({ kind: "addGuide", vertical: true })}>新增垂直輔助線</button>
            <button onClick={() => run({ kind: "addGuide", vertical: false })}>新增水平輔助線</button>
          </div>
          <div className="hf-note">拖曳輔助線可移動，拖出投影片即可刪除。</div>
          <div className="hf-buttons">
            <button className="primary" onClick={close}>
              完成
            </button>
          </div>
        </div>
      )}
    </Popover>
  );
}
