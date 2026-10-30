// Every user command, dispatched by App.tsx to the engine.
export type Command =
  | { kind: "open" }
  | { kind: "new" }
  | { kind: "save" }
  | { kind: "saveAs" }
  | { kind: "exportPdf" }
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
  | { kind: "group"; on: boolean }
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
  | { kind: "deleteSlide"; index?: number }
  | { kind: "moveSlide"; from: number; to: number }
  | { kind: "toggle"; which: "bold" | "italic" | "underline" | "strike" | "superscript" | "subscript" }
  | { kind: "charSpacing"; pt: number }
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
  | { kind: "slideShow"; fromBeginning: boolean; presenter?: boolean; forceAudience?: boolean }
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
  | { kind: "toggleRuler" }
  | { kind: "selectionPane"; open?: boolean }
  | { kind: "checker"; open?: boolean }
  | { kind: "altText"; text: string; decorative: boolean }
  | { kind: "showGrid"; on: boolean }
  | { kind: "showGuides"; on: boolean }
  | { kind: "snapToGrid"; on: boolean }
  | { kind: "gridSpacing"; emu: number }
  | { kind: "addGuide"; vertical: boolean }
  | { kind: "spellCheck"; on: boolean };

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
