// Every user command, dispatched by App.tsx to the engine.
export type Command =
  | { kind: "open" }
  | { kind: "new" }
  | { kind: "save" }
  | { kind: "saveAs" }
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
  | { kind: "toggle"; which: "bold" | "italic" | "underline" }
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
  | { kind: "slideShow"; fromBeginning: boolean };

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
