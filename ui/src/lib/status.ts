// Mirror of `App::status()` in crates/ppt-wasm.
export interface Status {
  slideIndex: number;
  slideCount: number;
  selectionCount: number;
  hasSelection: boolean;
  editingText: boolean;
  /** A table or one of its cells is the target (shows the table layout tab). */
  inTable: boolean;
  /** An editable SmartArt is selected (shows the SmartArt design tab). */
  smartArt?: boolean;
  /** Bumped when a SmartArt is double-clicked: open its text pane at `diagramFocus`. */
  diagramRequests?: number;
  diagramFocus?: number;
  /** UTF-16 range to select in that row (a find match). */
  diagramRange?: [number, number];
  /** Every selected shape is locked. */
  locked?: boolean;
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
  /** Absolute zoom in percent (100 = one slide point per CSS pixel). */
  zoomPercent: number;
  /** The slide follows the window size (no manual zoom). */
  zoomFit: boolean;
  /** Scroll bars `[x ratio, x position, y ratio, y position]`; empty during a show. */
  scroll: number[];
  frameMs: number;
  revision: number;
  selectionName: string;
  hasText: boolean;
  fontFamily?: string;
  fontSize?: number;
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  strike?: boolean;
  /** Baseline shift as a fraction of the size: > 0 superscript, < 0 subscript. */
  baseline?: number;
  /** Extra space between characters, in points. */
  charSpacing?: number;
  /** Line spacing of the current paragraph, as a multiple of single. */
  lineSpacing?: number;
  /** Vertical text position in the edited or selected shape. */
  textAnchor?: "top" | "middle" | "bottom";
  /** Frame of the single selected shape or selected group (EMU, degrees). */
  frame?: { x: number; y: number; w: number; h: number; rot: number; single: boolean };
  /** Shadow preset of the selected shape: a preset name, "none" or "custom". */
  shadow?: string;
  canChangeShape?: boolean;
  /** The shape the next drag draws, "" when none. */
  drawTool?: string;
  showGrid?: boolean;
  showGuides?: boolean;
  snapToGrid?: boolean;
  /** EMU. */
  gridSpacing?: number;
  /** Cropping of the selected picture (fractions), present only for pictures. */
  crop?: { left: number; top: number; right: number; bottom: number };
  cropMode?: boolean;
  /** The current slide's layout index, -1 when unknown. */
  slideLayout?: number;
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
  /** The current slide's transition, absent when it has none. */
  transition?: TransitionInfo;
  /** Per slide: whether it has a (visible) transition. */
  transitionSlides?: boolean[];
  /** The current slide's animations in playing order. */
  animations?: AnimationInfo[];
  /** Whether the selection can get an animation. */
  canAnimate?: boolean;
}

export interface AnimationInfo {
  /** Clicks before it starts (0: with the slide). */
  click: number;
  /** "clickEffect" | "withEffect" | "afterEffect" */
  trigger: string;
  /** "entr" | "exit" | "emph" | "path" */
  class: string;
  presetId: number | null;
  /** Effect option (direction, ...). */
  subtype: number | null;
  /** Names of the animated shapes. */
  shapes: string[];
  /** Start relative to its click, and length (ms). */
  startMs: number;
  durMs: number;
  /** The effect's own delay (ms). */
  delayMs: number;
  /** Whether its shapes are selected. */
  selected: boolean;
  /** Top-left of its shapes on the canvas (CSS pixels). */
  badge: [number, number] | null;
}

/** An effect the editor can add (`App::animationPresets()`). */
export interface AnimationPreset {
  class: string;
  id: number;
  subtype: number;
  durMs: number;
  directional: boolean;
}

export interface TransitionInfo {
  /** "fade", "push", ... ("none" in commands removes the transition). */
  id: string;
  /** Effect option: a direction ("l", "u", ...), "vertOut", "thruBlk", ... or "". */
  option: string;
  durationMs: number;
  onClick: boolean;
  /** Automatic advance after this long; -1 for none. */
  afterMs: number;
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
  zoomPercent: 100,
  zoomFit: true,
  scroll: [],
  frameMs: 0,
  revision: 0,
  selectionName: "",
  hasText: false,
};
