import { t } from "../i18n";
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
import { Combobox } from "./Combobox";
import { LinkPicker } from "./LinkPicker";
import { focusables, moveFocus, ownsArrows } from "../lib/keyboard";
import { focusCanvas } from "./CanvasHost";
import { FONT_SIZES, type Command, type TableStyleInfo } from "../lib/commands";
import type { AnimationPreset, Status } from "../lib/status";

/** Character spacing presets (points), as PowerPoint names them. */
const CHAR_SPACINGS: [number, string][] = [
  [-3, t("ribbon.spacingVeryTight")],
  [-1.5, t("ribbon.spacingTight")],
  [0, t("ribbon.spacingNormal")],
  [3, t("ribbon.spacingLoose")],
  [6, t("ribbon.spacingVeryLoose")],
];
const LINE_SPACINGS = [1, 1.5, 2, 2.5, 3];
const ANCHORS = [
  ["top", "anchorTop", t("ribbon.anchorTop")],
  ["middle", "anchorMiddle", t("ribbon.anchorMiddle")],
  ["bottom", "anchorBottom", t("ribbon.anchorBottom")],
] as const;

const TABS = ["常用", "插入", "設計", "圖形格式", "轉場", "動畫", "投影片放映", "校閱", "檢視"] as const;
/** Shown only while a table (or a cell in it) is the target, like PowerPoint's contextual tabs. */
const TABLE_TABS = ["表格設計", "版面配置"] as const;
const PICTURE_TAB = "圖片格式" as const;
/** Shown while a SmartArt is selected. */
const SMARTART_TAB = "SmartArt 設計" as const;
type Tab = (typeof TABS)[number] | (typeof TABLE_TABS)[number] | typeof PICTURE_TAB | typeof SMARTART_TAB;
const isTableTab = (id: Tab) => (TABLE_TABS as readonly string[]).includes(id);
/** Tabs are identified by their Traditional Chinese names; these are what is shown. */
const TAB_LABELS: Record<Tab, string> = {
  常用: t("tab.home"),
  插入: t("tab.insert"),
  設計: t("tab.design"),
  圖形格式: t("tab.shapeFormat"),
  轉場: t("tab.transitions"),
  動畫: t("tab.animations"),
  投影片放映: t("tab.slideShow"),
  校閱: t("tab.review"),
  檢視: t("tab.view"),
  表格設計: t("tab.tableDesign"),
  版面配置: t("tab.tableLayout"),
  圖片格式: t("tab.pictureFormat"),
  "SmartArt 設計": t("tab.smartArtDesign"),
};

/** Numbering schemes offered in the gallery, with a sample. */
export const NUMBERINGS: [string, string][] = [
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

/** Layouts of the Insert > SmartArt menu (`ppt_ooxml::smartart_kinds`). */
const SMARTART_LAYOUTS = ["vList2", "hList1", "lProcess2", "process1", "process2", "process3", "hProcess9", "StepDownProcess", "radial2", "chevron1", "hChevron3", "chevron2", "cycle2", "cycle3", "cycle5", "cycle8", "pyramid1", "orgChart1", "hierarchy1", "hierarchy2"] as const;

function Button(props: { icon: ReactNode; label: string; onClick: (e: React.MouseEvent) => void; disabled?: boolean; large?: boolean; active?: boolean; iconOnly?: boolean }) {
  const cls = ["rb-btn", props.large && "large", props.active && "active", props.iconOnly && "icon-only"].filter(Boolean).join(" ");
  return (
    <button className={cls} onClick={props.onClick} disabled={props.disabled} title={props.label} aria-label={props.iconOnly ? props.label : undefined} aria-pressed={props.active} onMouseDown={keepFocus}>
      {props.icon}
      {!props.iconOnly && <span>{props.label}</span>}
    </button>
  );
}

/** `launcher`: opens the group's dialog (the small corner arrow of a PowerPoint group). */
function Group({ label, children, launcher, launcherLabel, launcherDisabled }: { label: string; children: ReactNode; launcher?: () => void; launcherLabel?: string; launcherDisabled?: boolean }) {
  return (
    <div className="rb-group" role="group" aria-label={label}>
      <div className="rb-items">{children}</div>
      {launcher && (
        <button className="rb-launcher" title={launcherLabel} aria-label={launcherLabel} disabled={launcherDisabled} onMouseDown={keepFocus} onClick={launcher}>
          <svg width="9" height="9" viewBox="0 0 9 9" aria-hidden="true">
            <path d="M1 1h4M1 1v4M3.5 3.5 8 8M8 5v3H5" fill="none" stroke="currentColor" strokeWidth="1" />
          </svg>
        </button>
      )}
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
  /** The format painter is waiting for a target. */
  painter: "once" | "sticky" | null;
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
  /** The current slide has recorded narration. */
  hasNarration: boolean;
  rulerOn: boolean;
  paneOpen: boolean;
  checkerOpen: boolean;
  /** The SmartArt text pane is showing. */
  textPaneOpen: boolean;
  /** The selected object's alternative text, read when the dialog opens. */
  loadAltText: () => AltTextInfo | null;
  /** Switch to a tab (`n` changes with each request, so the same tab can be asked for again). */
  tabRequest?: { tab: string; n: number };
  /** Open the Font and Paragraph dialogs (the corner arrows of those groups). */
  onFontDialog: () => void;
  onParagraphDialog: () => void;
}

export function Ribbon({ status, fonts, layoutNames, layoutThumbs, loadLayoutThumbs, run, linkRequest, painter, tableStyles, notesOpen, usePresenter, spellOn, animPresets, onTabChange, shapeIcons, loadHeaderFooter, theme, colorSchemes, fontSchemes, slideSize, loadBackground, viewMode, hasNarration, rulerOn, paneOpen, checkerOpen, textPaneOpen, loadAltText, tabRequest, onFontDialog, onParagraphDialog }: Props) {
  const [chosen, setTab] = useState<Tab>("常用");
  const tab = (isTableTab(chosen) && !status.inTable) || (chosen === PICTURE_TAB && !status.crop) || (chosen === SMARTART_TAB && !status.smartArt) ? "常用" : chosen;
  useEffect(() => onTabChange(tab), [tab, onTabChange]);
  const tabs: Tab[] = [...TABS, ...(status.inTable ? TABLE_TABS : []), ...(status.crop ? [PICTURE_TAB] : []), ...(status.smartArt ? [SMARTART_TAB] : [])];
  useEffect(() => {
    if (linkRequest > 0) setTab("插入");
  }, [linkRequest]);
  useEffect(() => {
    if (tabRequest && (TABS as readonly string[]).includes(tabRequest.tab)) setTab(tabRequest.tab as Tab);
  }, [tabRequest]);
  const text = status.hasText;
  const shapes = status.hasSelection && !status.editingText;

  const newSlide = (large: boolean) => (
    <LayoutPicker
      names={layoutNames}
      thumbs={layoutThumbs}
      onOpen={loadLayoutThumbs}
      onPick={(layout) => run({ kind: "newSlide", layout })}
      button={(open, toggle) => (
        <span className="split-btn">
          <Button large={large} icon={icons.newSlide} label={t("ribbon.newSlide")} onClick={() => run({ kind: "newSlide" })} />
          <button className={open ? "split-arrow active" : "split-arrow"} title={t("ribbon.chooseLayout")} onClick={toggle} onMouseDown={keepFocus}>
            ▾
          </button>
        </span>
      )}
    />
  );

  // 協助工具: alternative text of the selected object, and the selection pane.
  const accessGroup = (
    <Group label={t("group.accessibility")}>
      <AltTextDialog
        load={loadAltText}
        run={run}
        button={(open, toggle) => <Button large icon={icons.altText} label={t("ribbon.altText")} active={open} disabled={!status.frame || status.selectionCount !== 1} onClick={toggle} />}
      />
      <Button large icon={icons.selectionPane} label={t("ribbon.selectionPane")} active={paneOpen} onClick={() => run({ kind: "selectionPane" })} />
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
        {tabs.map((id) => (
          <button key={id} role="tab" aria-selected={tab === id} className={[tab === id && "active", (isTableTab(id) || id === PICTURE_TAB || id === SMARTART_TAB) && "contextual"].filter(Boolean).join(" ")} onClick={() => setTab(id)} onMouseDown={keepFocus}>
            {TAB_LABELS[id]}
          </button>
        ))}
      </div>
      <div className="rb-body" role="tabpanel" aria-label={TAB_LABELS[tab]}>
        {tab === "常用" && (
          <>
            <Group label={t("group.clipboard")}>
              <Button large icon={icons.paste} label={t("ribbon.paste")} onClick={() => run({ kind: "paste" })} />
              <div className="rb-col">
                <Button icon={icons.cut} label={t("ribbon.cut")} disabled={!status.hasSelection} onClick={() => run({ kind: "cut" })} />
                <Button icon={icons.copy} label={t("ribbon.copy")} disabled={!status.hasSelection} onClick={() => run({ kind: "copy" })} />
                {/* A click paints once; a double click stays on until Esc or another click. */}
                <Button
                  icon={icons.formatPainter}
                  label={t("ribbon.formatPainter")}
                  active={painter !== null}
                  disabled={!status.hasSelection && painter === null && !status.editingText}
                  onClick={(e) => run({ kind: "formatPainter", mode: e.detail >= 2 ? "sticky" : painter ? "off" : "once" })}
                />
              </div>
            </Group>
            <Group label={t("group.slides")}>
              {newSlide(true)}
              <div className="rb-col">
                <LayoutPicker
                  names={layoutNames}
                  thumbs={layoutThumbs}
                  current={status.slideLayout}
                  onOpen={loadLayoutThumbs}
                  onPick={(layout) => run({ kind: "applyLayout", layout })}
                  button={(open, toggle) => <Button icon={icons.layout} label={t("ribbon.layoutMenu")} active={open} onClick={toggle} />}
                />
                <Button icon={icons.resetSlide} label={t("ribbon.reset")} disabled={(status.slideLayout ?? -1) < 0} onClick={() => run({ kind: "resetSlide" })} />
              </div>
            </Group>
            <Group label={t("group.font")} launcher={onFontDialog} launcherLabel={t("ctx.font")} launcherDisabled={!text}>
              <div className="rb-col">
                <div className="rb-row">
                  <Combobox
                    className="font-family"
                    value={status.fontFamily ?? ""}
                    options={fonts}
                    previewFonts
                    disabled={!text}
                    placeholder={t("group.font")}
                    ariaLabel={t("group.font")}
                    onCommit={(family) => run({ kind: "fontFamily", family })}
                  />
                  <Combobox
                    className="font-size"
                    value={status.fontSize ? String(Math.round(status.fontSize * 10) / 10) : ""}
                    options={FONT_SIZES.map(String)}
                    disabled={!text}
                    ariaLabel={t("ribbon.fontSize")}
                    onCommit={(v) => Number(v) > 0 && run({ kind: "fontSize", size: Number(v) })}
                  />
                  <Button iconOnly icon={<b className="glyph">A⁺</b>} label={t("ribbon.growFont")} disabled={!text} onClick={() => run({ kind: "fontStep", up: true })} />
                  <Button iconOnly icon={<b className="glyph">A⁻</b>} label={t("ribbon.shrinkFont")} disabled={!text} onClick={() => run({ kind: "fontStep", up: false })} />
                </div>
                <div className="rb-row">
                  <Button iconOnly icon={<b className="glyph">B</b>} label={t("ribbon.bold")} active={status.bold} disabled={!text} onClick={() => run({ kind: "toggle", which: "bold" })} />
                  <Button iconOnly icon={<i className="glyph serif">I</i>} label={t("ribbon.italic")} active={status.italic} disabled={!text} onClick={() => run({ kind: "toggle", which: "italic" })} />
                  <Button iconOnly icon={<u className="glyph">U</u>} label={t("ribbon.underline")} active={status.underline} disabled={!text} onClick={() => run({ kind: "toggle", which: "underline" })} />
                  <Button iconOnly icon={<s className="glyph">S</s>} label={t("ribbon.strike")} active={status.strike} disabled={!text} onClick={() => run({ kind: "toggle", which: "strike" })} />
                  <Button iconOnly icon={<span className="glyph">x²</span>} label={t("ribbon.superscript")} active={(status.baseline ?? 0) > 0} disabled={!text} onClick={() => run({ kind: "toggle", which: "superscript" })} />
                  <Button iconOnly icon={<span className="glyph">x₂</span>} label={t("ribbon.subscript")} active={(status.baseline ?? 0) < 0} disabled={!text} onClick={() => run({ kind: "toggle", which: "subscript" })} />
                  <Popover
                    button={(open, toggle) => <Button iconOnly icon={icons.charSpacing} label={t("ribbon.charSpacing")} active={open} disabled={!text} onClick={toggle} />}
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
                  <Button iconOnly icon={icons.clearFormat} label={t("ribbon.clearFormatting")} disabled={!text} onClick={() => run({ kind: "clearFormatting" })} />
                  <ColorPicker icon={icons.fontColor} label={t("ribbon.fontColor")} value={status.textColor} disabled={!text} onPick={(color) => color && run({ kind: "textColor", color })} />
                </div>
              </div>
            </Group>
            <Group label={t("group.paragraph")} launcher={onParagraphDialog} launcherLabel={t("ctx.paragraph")} launcherDisabled={!text}>
              <div className="rb-col">
                <div className="rb-row">
                  <Button iconOnly icon={icons.bulletList} label={t("ribbon.bullets")} active={status.bullet} disabled={!text} onClick={() => run({ kind: "bullets" })} />
                  <Popover
                    button={(open, toggle) => (
                      <span className="split-btn">
                        <Button iconOnly icon={icons.numberList} label={t("ribbon.numbering")} active={!!status.numbering} disabled={!text} onClick={() => run({ kind: "numbering", scheme: status.numbering || "arabicPeriod" })} />
                        <button className={open ? "split-arrow active" : "split-arrow"} title={t("ribbon.numberingFormat")} disabled={!text} onClick={toggle} onMouseDown={keepFocus}>
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
                  <Button iconOnly icon={icons.outdent} label={t("ribbon.outdent")} disabled={!text} onClick={() => run({ kind: "level", delta: -1 })} />
                  <Button iconOnly icon={icons.indent} label={t("ribbon.indent")} disabled={!text} onClick={() => run({ kind: "level", delta: 1 })} />
                </div>
                <div className="rb-row">
                  {(
                    [
                      ["left", icons.alignLeft, t("ribbon.alignLeft")],
                      ["center", icons.alignCenter, t("ribbon.alignCenter")],
                      ["right", icons.alignRight, t("ribbon.alignRight")],
                      ["justify", icons.alignJustify, t("ribbon.justify")],
                    ] as const
                  ).map(([a, icon, label]) => (
                    <Button key={a} iconOnly icon={icon} label={label} active={status.align === a} disabled={!text} onClick={() => run({ kind: "align", align: a })} />
                  ))}
                  <Popover button={(open, toggle) => <Button iconOnly icon={icons.lineSpacing} label={t("ribbon.lineSpacing")} active={open} disabled={!text} onClick={toggle} />}>
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
                      <Button iconOnly icon={status.textAnchor === "middle" ? icons.anchorMiddle : status.textAnchor === "bottom" ? icons.anchorBottom : icons.anchorTop} label={t("ribbon.alignText")} active={open} disabled={!status.textAnchor} onClick={toggle} />
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
            <Group label={t("group.drawing")}>
              <div className="rb-col">
                <ColorPicker icon={icons.fill} label={t("ribbon.shapeFill")} value={status.fill} disabled={!shapes} noneLabel={t("ribbon.noFill")} onPick={(color) => run({ kind: "fill", color })} />
                <ColorPicker icon={icons.outline} label={t("ribbon.shapeOutline")} value={status.lineColor} disabled={!shapes} noneLabel={t("ribbon.noOutline")} onPick={(color) => run({ kind: "lineColor", color })} />
              </div>
            </Group>
            <Group label={t("group.editing")}>
              <div className="rb-col">
                <Button icon={icons.undo} label={t("ribbon.undo")} disabled={!status.canUndo} onClick={() => run({ kind: "undo" })} />
                <Button icon={icons.redo} label={t("ribbon.redo")} disabled={!status.canRedo} onClick={() => run({ kind: "redo" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.find} label={t("ribbon.find")} onClick={() => run({ kind: "find", replace: false })} />
                <Button icon={icons.replace} label={t("ribbon.replace")} onClick={() => run({ kind: "find", replace: true })} />
                <Button icon={icons.selectAll} label={t("ribbon.selectAll")} onClick={() => run({ kind: "selectAll" })} />
              </div>
            </Group>
          </>
        )}
        {tab === "插入" && (
          <>
            <Group label={t("group.slides")}>{newSlide(true)}</Group>
            <Group label={t("ribbon.table")}>
              <TablePicker
                onPick={(rows, cols) => run({ kind: "insertTable", rows, cols })}
                button={(open, toggle) => <Button large active={open} icon={icons.table} label={t("ribbon.table")} onClick={toggle} />}
              />
            </Group>
            <Group label={t("ribbon.smartArt")}>
              <Popover button={(open, toggle) => <Button large active={open} icon={icons.smartArt} label={t("ribbon.smartArt")} onClick={toggle} />}>
                {(close) => (
                  <div className="menu-panel">
                    {SMARTART_LAYOUTS.map((layout) => (
                      <button
                        key={layout}
                        className="menu-item"
                        onMouseDown={keepFocus}
                        onClick={() => {
                          run({ kind: "insertSmartArt", layout });
                          close();
                        }}
                      >
                        {t(`smartArt.${layout}`)}
                      </button>
                    ))}
                  </div>
                )}
              </Popover>
            </Group>
            <Group label={t("group.images")}>
              <Button large icon={icons.picture} label={t("ribbon.picture")} onClick={() => run({ kind: "picture" })} />
            </Group>
            <Group label={t("group.illustrations")}>
              <ShapeGallery
                icons={shapeIcons}
                active={status.drawTool}
                onPick={(tool) => run({ kind: "drawTool", tool })}
                button={(open, toggle) => <Button large active={open || !!status.drawTool} icon={icons.shapes} label={t("ribbon.shapes")} onClick={toggle} />}
              />
            </Group>
            <Group label={t("group.text")}>
              <Button large icon={icons.textbox} label={t("ribbon.textBox")} onClick={() => run({ kind: "textBox" })} />
              <div className="rb-col">
                <HeaderFooterDialog
                  load={loadHeaderFooter}
                  onApply={(c) => run({ kind: "headerFooter", ...c })}
                  button={(open, toggle) => <Button icon={icons.headerFooter} label={t("ribbon.headerFooter")} active={open} onClick={toggle} />}
                />
                <SymbolPicker onPick={(text) => run({ kind: "insertSymbol", text })} button={(open, toggle) => <Button icon={icons.symbol} label={t("ribbon.symbol")} active={open} onClick={toggle} />} />
              </div>
            </Group>
            <Group label={t("group.links")}>
              <LinkPicker
                current={status.link}
                disabled={!text}
                request={linkRequest}
                onApply={(url) => run({ kind: "link", url })}
                button={(open, toggle) => <Button large active={open} disabled={!text} icon={icons.link} label={t("ribbon.link")} onClick={toggle} />}
              />
            </Group>
          </>
        )}
        {tab === "圖形格式" && (
          <>
            <Group label={t("group.shapeStyles")}>
              <Button icon={icons.formatPane} label={t("ctx.formatShape")} disabled={!shapes} onClick={() => run({ kind: "formatPane" })} />
              <div className="rb-col">
                <ColorPicker icon={icons.fill} label={t("ribbon.shapeFill")} value={status.fill} disabled={!shapes} noneLabel={t("ribbon.noFill")} onPick={(color) => run({ kind: "fill", color })} />
                <ColorPicker icon={icons.outline} label={t("ribbon.shapeOutline")} value={status.lineColor} disabled={!shapes} noneLabel={t("ribbon.noOutline")} onPick={(color) => run({ kind: "lineColor", color })} />
              </div>
              <div className="rb-col">
                <label className="rb-field">
                  {t("ribbon.outlineWeight")}
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
            <Group label={t("ribbon.shapes")}>
              <ShapeGallery
                icons={shapeIcons}
                lines={false}
                onPick={(preset) => run({ kind: "changeShape", preset })}
                button={(open, toggle) => (
                  <button className={open ? "rb-btn active" : "rb-btn"} disabled={!status.canChangeShape} onMouseDown={keepFocus} onClick={toggle} title={t("ribbon.changeShape")}>
                    {icons.changeShape}
                    <span>{t("ribbon.changeShapeMenu")}</span>
                  </button>
                )}
              />
              <ShadowMenu status={status} disabled={!shapes || status.shadow === undefined} run={run} />
            </Group>
            <Group label={t("group.arrange")}>
              <div className="rb-col">
                <Button icon={icons.front} label={t("ribbon.bringToFront")} disabled={!shapes} onClick={() => run({ kind: "reorder", order: "front" })} />
                <Button icon={icons.back} label={t("ribbon.sendToBack")} disabled={!shapes} onClick={() => run({ kind: "reorder", order: "back" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.front} label={t("ribbon.bringForward")} disabled={!shapes} onClick={() => run({ kind: "reorder", order: "forward" })} />
                <Button icon={icons.back} label={t("ribbon.sendBackward")} disabled={!shapes} onClick={() => run({ kind: "reorder", order: "backward" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.group} label={t("ribbon.group")} disabled={!status.canGroup} onClick={() => run({ kind: "group", on: true })} />
                <Button icon={icons.ungroup} label={t("ribbon.ungroup")} disabled={!status.canUngroup} onClick={() => run({ kind: "group", on: false })} />
              </div>
              <div className="rb-col">
                <RotateMenu disabled={!status.frame} run={run} />
              </div>
            </Group>
            {accessGroup}
            <SizeGroup status={status} run={run} />
            <Group label={t("group.align")}>
              {(
                [
                  ["left", t("ribbon.shapesLeft")],
                  ["center", t("ribbon.shapesCenter")],
                  ["right", t("ribbon.shapesRight")],
                  ["top", t("ribbon.shapesTop")],
                  ["middle", t("ribbon.shapesMiddle")],
                  ["bottom", t("ribbon.shapesBottom")],
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
            <Group label={t("group.createGraphic")}>
              <Button large icon={icons.addShapeAfter} label={t("ribbon.addShapeAfter")} onClick={() => run({ kind: "smartArt", command: "addAfter" })} />
              <div className="rb-col">
                <Button icon={icons.addShapeBefore} label={t("ribbon.addShapeBefore")} onClick={() => run({ kind: "smartArt", command: "addBefore" })} />
                <Button icon={icons.addChild} label={t("ribbon.addShapeBelow")} onClick={() => run({ kind: "smartArt", command: "addChild" })} />
                <Button icon={icons.textPane} label={t("ribbon.textPane")} active={textPaneOpen} onClick={() => run({ kind: "textPane" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.outdent} label={t("ribbon.promote")} onClick={() => run({ kind: "smartArt", command: "promote" })} />
                <Button icon={icons.indent} label={t("ribbon.demote")} onClick={() => run({ kind: "smartArt", command: "demote" })} />
                <Button icon={icons.delete} label={t("ribbon.deleteShape")} onClick={() => run({ kind: "smartArt", command: "delete" })} />
              </div>
              <div className="rb-col">
                <Button icon={icons.moveUp} label={t("ribbon.moveUp")} onClick={() => run({ kind: "smartArt", command: "moveUp" })} />
                <Button icon={icons.moveDown} label={t("ribbon.moveDown")} onClick={() => run({ kind: "smartArt", command: "moveDown" })} />
              </div>
            </Group>
            {accessGroup}
          </>
        )}
        {tab === "投影片放映" && (
          <>
            <Group label={t("group.startSlideShow")}>
              <Button icon={icons.playStart} label={t("ribbon.fromBeginning")} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: true })} />
              <Button icon={icons.playCurrent} label={t("ribbon.fromCurrent")} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false })} />
              <Button icon={icons.presenter} label={t("ribbon.presenterView")} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false, presenter: true })} />
            </Group>
            <Group label={t("group.record")}>
              <Button icon={icons.record} label={t("ribbon.recordFromBeginning")} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: true, record: true })} />
              <Button icon={icons.record} label={t("ribbon.recordFromCurrent")} disabled={status.slideCount === 0} onClick={() => run({ kind: "slideShow", fromBeginning: false, record: true })} />
              <Button icon={icons.clearNarration} label={t("ribbon.clearNarration")} disabled={!hasNarration} onClick={() => run({ kind: "clearNarration" })} />
            </Group>
            <Group label={t("group.monitors")}>
              <label className="rb-check" title={t("ribbon.usePresenterTip")}>
                <input type="checkbox" checked={usePresenter} onChange={(e) => run({ kind: "usePresenter", on: e.target.checked })} />
                {t("ribbon.usePresenter")}
              </label>
            </Group>
            <Group label={t("group.export")}>
              <Button large icon={icons.exportVideo} label={t("ribbon.exportVideo")} disabled={status.slideCount === 0} onClick={() => run({ kind: "exportVideo" })} />
            </Group>
          </>
        )}
        {tab === "校閱" && (
          <>
            <Group label={t("group.proofing")}>
              <Button large icon={icons.spelling} label={t("ribbon.spelling")} active={spellOn} onClick={() => run({ kind: "spellCheck", on: !spellOn })} />
            </Group>
            <Group label={t("group.accessibility")}>
              <Button large icon={icons.checkAccess} label={t("ribbon.checkAccessibility")} active={checkerOpen} onClick={() => run({ kind: "checker" })} />
              <AltTextDialog
                load={loadAltText}
                run={run}
                button={(open, toggle) => <Button large icon={icons.altText} label={t("ribbon.altText")} active={open} disabled={!status.frame || status.selectionCount !== 1} onClick={toggle} />}
              />
            </Group>
          </>
        )}
        {tab === "檢視" && (
          <>
            <Group label={t("group.presentationViews")}>
              <Button large icon={icons.normalView} label={t("ribbon.normal")} active={viewMode === "normal"} onClick={() => run({ kind: "viewMode", mode: "normal" })} />
              <Button large icon={icons.sorterView} label={t("ribbon.sorter")} active={viewMode === "sorter"} onClick={() => run({ kind: "viewMode", mode: "sorter" })} />
              <Button large icon={icons.readingView} label={t("ribbon.reading")} onClick={() => run({ kind: "slideShow", fromBeginning: false, reading: true })} />
            </Group>
            <Group label={t("group.show")}>
              <div className="rb-checks">
                <label className="rb-check">
                  <input type="checkbox" checked={rulerOn} onChange={() => run({ kind: "toggleRuler" })} />
                  {t("ribbon.ruler")}
                </label>
                <label className="rb-check">
                  <input type="checkbox" checked={!!status.showGrid} onChange={(e) => run({ kind: "showGrid", on: e.target.checked })} />
                  {t("ribbon.gridlines")}
                </label>
                <label className="rb-check">
                  <input type="checkbox" checked={!!status.showGuides} onChange={(e) => run({ kind: "showGuides", on: e.target.checked })} />
                  {t("ribbon.guides")}
                </label>
              </div>
              <GridMenu status={status} run={run} />
              <Button icon={icons.notes} label={t("ribbon.notes")} active={notesOpen} onClick={() => run({ kind: "toggleNotes" })} />
            </Group>
            <Group label={t("group.zoom")}>
              <Button icon={icons.zoomIn} label={t("ribbon.zoomIn")} onClick={() => run({ kind: "zoom", factor: 1.25 })} />
              <Button icon={icons.zoomOut} label={t("ribbon.zoomOut")} onClick={() => run({ kind: "zoom", factor: 0.8 })} />
              <Button icon={icons.fit} label={t("ribbon.fit")} onClick={() => run({ kind: "zoom", factor: "fit" })} />
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
        <button className={open ? "rb-btn active" : "rb-btn"} title={t("grid.title")} onMouseDown={keepFocus} onClick={toggle}>
          {icons.grid}
          <span>{t("grid.settingsMenu")}</span>
        </button>
      )}
    >
      {(close) => (
        <div className="hf-dialog grid-menu" role="dialog" aria-label={t("grid.title")}>
          <div className="hf-title">{t("grid.snapTo")}</div>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.snapToGrid} onChange={(e) => run({ kind: "snapToGrid", on: e.target.checked })} />
            {t("grid.snapToGrid")}
          </label>
          <div className="hf-title sep">{t("grid.gridSettings")}</div>
          <label className="hf-row">
            {t("grid.spacing")}
            <select value={GRID_SPACINGS.find((g) => Math.abs(g - spacing) < 0.005) ?? ""} onChange={(e) => run({ kind: "gridSpacing", emu: Number(e.target.value) * EMU_PER_CM })}>
              {!GRID_SPACINGS.some((g) => Math.abs(g - spacing) < 0.005) && <option value="">{t("ribbon.cm", { n: spacing.toFixed(2) })}</option>}
              {GRID_SPACINGS.map((g) => (
                <option key={g} value={g}>
                  {t("ribbon.cm", { n: g })}
                </option>
              ))}
            </select>
          </label>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.showGrid} onChange={(e) => run({ kind: "showGrid", on: e.target.checked })} />
            {t("grid.showGrid")}
          </label>
          <div className="hf-title sep">{t("grid.guideSettings")}</div>
          <label className="hf-row">
            <input type="checkbox" checked={!!status.showGuides} onChange={(e) => run({ kind: "showGuides", on: e.target.checked })} />
            {t("grid.showGuides")}
          </label>
          <div className="hf-buttons">
            <button onClick={() => run({ kind: "addGuide", vertical: true })}>{t("grid.addVertical")}</button>
            <button onClick={() => run({ kind: "addGuide", vertical: false })}>{t("grid.addHorizontal")}</button>
          </div>
          <div className="hf-note">{t("grid.dragHint")}</div>
          <div className="hf-buttons">
            <button className="primary" onClick={close}>
              {t("common.done")}
            </button>
          </div>
        </div>
      )}
    </Popover>
  );
}
