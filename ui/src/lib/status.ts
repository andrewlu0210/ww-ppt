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
  frameMs: 0,
  revision: 0,
  selectionName: "",
  hasText: false,
};
