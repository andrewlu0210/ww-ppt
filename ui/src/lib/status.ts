// Mirror of `App::status()` in crates/ppt-wasm.
export interface Status {
  slideIndex: number;
  slideCount: number;
  selectionCount: number;
  hasSelection: boolean;
  editingText: boolean;
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
  align?: "left" | "center" | "right" | "justify";
  bullet?: boolean;
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
  canUndo: false,
  canRedo: false,
  canPaste: false,
  zoom: 1,
  frameMs: 0,
  revision: 0,
  selectionName: "",
  hasText: false,
};
