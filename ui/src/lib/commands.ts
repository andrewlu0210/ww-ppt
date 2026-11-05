
// Every user command, dispatched by App.tsx to the engine.
export type Command =
  | { kind: "open" }
  | { kind: "new" }
  | { kind: "save" }
  | { kind: "saveAs" }
  | { kind: "exportPdf" }
  | { kind: "exportVideo" }
  | { kind: "undo" }
  | { kind: "redo" }
  | { kind: "cut" }
  | { kind: "copy" }
  | { kind: "paste" }
  | { kind: "delete" }
  | { kind: "duplicate" }
  | { kind: "textBox" }
  | { kind: "shape"; preset: string }
  | { kind: "picture" }
  | { kind: "insertTable"; rows: number; cols: number }
  | { kind: "insertSmartArt"; layout: string }
  | { kind: "group"; on: boolean }
  /** Lock or unlock the selection against moving, resizing and rotating. */
  | { kind: "lock"; on: boolean }
  | { kind: "link"; url: string }
  | { kind: "tableMerge" }
  | { kind: "tableSplit"; rows: number; cols: number }
  | { kind: "cellFill"; color: string }
  | { kind: "cellBorders"; which: string; color: string; width: number }
  | { kind: "cellAnchor"; anchor: "top" | "middle" | "bottom" }
  | { kind: "tableStyle"; id: string }
  | { kind: "tableFlag"; flag: string; on: boolean }
  | { kind: "table"; command: "rowAbove" | "rowBelow" | "colLeft" | "colRight" | "deleteRow" | "deleteCol" }
  | { kind: "newSlide"; layout?: number }
  | { kind: "duplicateSlide"; index?: number }
  /** Hide a slide from the slide show (default: the current one), or show it again. */
  | { kind: "hideSlide"; index?: number; on: boolean }
  /** The slide clipboard: copy or cut slide `index`, or paste after it (null: at the end). */
  | { kind: "slideClip"; op: "copy" | "cut"; index: number }
  | { kind: "slideClip"; op: "paste"; after: number | null }
  | { kind: "deleteSlide"; index?: number }
  | { kind: "moveSlide"; from: number; to: number }
  | { kind: "toggle"; which: "bold" | "italic" | "underline" | "strike" | "superscript" | "subscript" }
  | { kind: "charSpacing"; pt: number }
  /** The Font dialog: what the spec names is set in one undo step (`App::setFontSpec`). */
  | { kind: "fontSpec"; spec: FontSpec }
  /** The Paragraph dialog (`App::setParagraphSpec`). */
  | { kind: "paragraphSpec"; spec: ParagraphSpec }
  | { kind: "clearFormatting" }
  | { kind: "lineSpacing"; multiple: number }
  | { kind: "textAnchor"; anchor: "top" | "middle" | "bottom" }
  | { kind: "applyLayout"; layout: number }
  | { kind: "resetSlide" }
  | { kind: "find"; replace: boolean }
  | { kind: "selectAll" }
  | { kind: "setFrame"; x: number; y: number; w: number; h: number; rot: number }
  | { kind: "rotate"; degrees: number }
  | { kind: "flip"; horizontal: boolean }
  | { kind: "changeShape"; preset: string }
  | { kind: "shadow"; preset: string }
  | { kind: "cropMode"; on: boolean }
  | { kind: "crop"; left: number; top: number; right: number; bottom: number }
  | { kind: "resetPicture" }
  | { kind: "drawTool"; tool: string }
  | { kind: "headerFooter"; date: "none" | "auto" | "fixed"; dateText: string; slideNumber: boolean; footer: boolean; footerText: string; skipTitle: boolean; all: boolean }
  | { kind: "insertSymbol"; text: string }
  | { kind: "background"; fill: "solid"; color: string }
  | { kind: "background"; fill: "gradient"; start: string; end: string; radial: boolean; angle: number }
  | { kind: "backgroundPicture" }
  /** The slide background, set in full (`App::setBackgroundFill`). */
  | { kind: "backgroundFill"; spec: import("../components/FormatPane").FillSpec }
  | { kind: "resetBackground" }
  | { kind: "hideBackgroundGraphics"; on: boolean }
  | { kind: "backgroundToAll" }
  | { kind: "colorScheme"; name: string }
  | { kind: "fontScheme"; name: string }
  | { kind: "slideSize"; width: number; height: number; fit: "maximize" | "fit" }
  | { kind: "fontSize"; size: number }
  | { kind: "fontStep"; up: boolean }
  | { kind: "fontFamily"; family: string }
  | { kind: "textColor"; color: string }
  | { kind: "align"; align: "left" | "center" | "right" | "justify" }
  | { kind: "bullets" }
  | { kind: "numbering"; scheme: string }
  | { kind: "level"; delta: number }
  | { kind: "fill"; color: string }
  | { kind: "lineColor"; color: string }
  | { kind: "lineWidth"; width: number }
  | { kind: "reorder"; order: "front" | "forward" | "backward" | "back" }
  | { kind: "alignShapes"; to: "left" | "center" | "right" | "top" | "middle" | "bottom" }
  | { kind: "zoom"; factor: number | "fit" }
  /** Format painter: "once" and "sticky" copy the current format and wait for a target; "off" cancels. */
  | { kind: "formatPainter"; mode: "once" | "sticky" | "off" }
  | { kind: "slideShow"; fromBeginning: boolean; presenter?: boolean; forceAudience?: boolean; /** Reading view: play inside the window, not full screen. */ reading?: boolean; /** Record narration while it plays. */ record?: boolean }
  | { kind: "usePresenter"; on: boolean }
  | { kind: "setTransition"; id: string; option: string; durationMs: number; onClick: boolean; afterMs: number; all: boolean }
  | { kind: "previewTransition" }
  | { kind: "previewAnimations" }
  | { kind: "addAnimation"; class: string; id: number; subtype?: number }
  | { kind: "changeAnimation"; index: number; class: string; id: number; subtype?: number }
  | { kind: "removeAnimation"; index: number }
  | { kind: "moveAnimation"; from: number; to: number }
  | { kind: "setAnimationTrigger"; index: number; trigger: string }
  | { kind: "setAnimationTiming"; index: number; delayMs?: number; durMs?: number }
  | { kind: "selectAnimation"; index: number }
  | { kind: "toggleNotes" }
  | { kind: "viewMode"; mode: "normal" | "sorter" }
  /** The selection's fill, set in full (`App::setFillSpec`). */
  | { kind: "fillSpec"; spec: import("../components/FormatPane").FillSpec }
  /** Changes to the selection's outline (`App::setLineSpec`). */
  | { kind: "lineSpec"; spec: import("../components/FormatPane").LineSpec }
  /** Changes to the selection's drop shadow (`App::setShadowSpec`). */
  | { kind: "shadowSpec"; spec: import("../components/FormatPane").ShadowSpec }
  /** Outline and shadow of the selected text. */
  | { kind: "textEffectSpec"; spec: import("../components/FormatPane").TextEffectSpec }
  /** Changes to the selection's text box settings (`App::setTextBoxSpec`). */
  | { kind: "textBoxSpec"; spec: import("../components/FormatPane").TextBoxSpec }
  /** "Lock aspect ratio" for the selection. */
  | { kind: "lockAspect"; on: boolean }
  /** Open, close or toggle the Format Shape pane. */
  | { kind: "formatPane"; open?: boolean }
  /** Remove the current slide's recorded narration. */
  | { kind: "clearNarration" }
  | { kind: "toggleRuler" }
  | { kind: "selectionPane"; open?: boolean }
  | { kind: "textPane"; open?: boolean }
  | { kind: "smartArt"; command: "addAfter" | "addBefore" | "addChild" | "delete" | "promote" | "demote" | "moveUp" | "moveDown" }
  | { kind: "checker"; open?: boolean }
  | { kind: "altText"; text: string; decorative: boolean }
  | { kind: "showGrid"; on: boolean }
  | { kind: "showGuides"; on: boolean }
  | { kind: "snapToGrid"; on: boolean }
  | { kind: "gridSpacing"; emu: number }
  | { kind: "addGuide"; vertical: boolean }
  | { kind: "spellCheck"; on: boolean };

export interface FontSpec {
  family?: string;
  eaFamily?: string;
  size?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  baseline?: "normal" | "super" | "sub";
  /** Extra space between characters, in points. */
  spacing?: number;
  color?: string;
}

export interface ParagraphSpec {
  align?: "left" | "center" | "right" | "justify";
  /** Points. */
  indentLeft?: number;
  special?: "none" | "first" | "hanging";
  by?: number;
  spaceBefore?: number;
  spaceAfter?: number;
  lineSpacing?: number;
}

/** A gallery table style with the colors its preview needs (`App::tableStyles()`). */
export interface TableStyleInfo {
  id: string;
  name: string;
  header: string;
  band: string;
  body: string;
  line: string;
}

/** Put on the system clipboard when shapes are copied, so paste knows to use the internal clipboard. */
export const SHAPES_MARKER = "\u2063ww-ppt-shapes";

export const FONT_SIZES = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 24, 28, 32, 36, 40, 44, 48, 54, 60, 66, 72, 80, 96];
