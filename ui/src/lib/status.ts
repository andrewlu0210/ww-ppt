// Mirror of `App::status()` in crates/ppt-wasm.
export interface Status {
  slideIndex: number;
  slideCount: number;
  selectionCount: number;
  hasSelection: boolean;
  editingText: boolean;
  /** A table or one of its cells is the target (shows the table layout tab). */
  inTable: boolean;
  canGroup?: boolean;
  canMerge?: boolean;
  tableStyle?: string;
  tableFlags?: Record<string, boolean>;
  cellAnchor?: "top" | "middle" | "bottom";
  canUngroup?: boolean;
  canUndo: boolean;
  canRedo: boolean;
  canPaste: boolean;
  zoom: number;
  frameMs: number;
  revision: number;
  selectionName: string;
  hasText: boolean;
  fontFamily?: string;
  fontSize?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  textColor?: string;
  /** URL of the link at the caret / on the selected text, "" when none. */
  link?: string;
  align?: "left" | "center" | "right" | "justify";
  bullet?: boolean;
  /** Numbering scheme of the current paragraph, "" when not numbered. */
  numbering?: string;
  /** Present when shapes are selected; "" means no fill / no outline. */
  fill?: string;
  lineColor?: string;
  lineWidth?: number;
}

export const EMPTY_STATUS: Status = {
  slideIndex: 0,
  slideCount: 0,
  selectionCount: 0,
  hasSelection: false,
  editingText: false,
  inTable: false,
  canUndo: false,
  canRedo: false,
  canPaste: false,
  zoom: 1,
  frameMs: 0,
  revision: 0,
  selectionName: "",
  hasText: false,
};
