// Small line icons for the ribbon, drawn on a 24×24 grid.
import type { ReactNode } from "react";

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export const icons: Record<string, ReactNode> = {
  addShapeAfter: (
    <Svg>
      <rect x="2.5" y="8" width="9" height="8" rx="1" />
      <rect x="14" y="8" width="7.5" height="8" rx="1" strokeDasharray="2 1.5" />
      <path d="M17.75 10.5v3M16.25 12h3" />
    </Svg>
  ),
  addShapeBefore: (
    <Svg>
      <rect x="2.5" y="8" width="7.5" height="8" rx="1" strokeDasharray="2 1.5" />
      <rect x="12.5" y="8" width="9" height="8" rx="1" />
      <path d="M6.25 10.5v3M4.75 12h3" />
    </Svg>
  ),
  addChild: (
    <Svg>
      <rect x="3" y="3" width="10" height="7" rx="1" />
      <path d="M8 10v6.5h4" />
      <rect x="12" y="13" width="9" height="7" rx="1" strokeDasharray="2 1.5" />
      <path d="M16.5 15v3M15 16.5h3" />
    </Svg>
  ),
  moveUp: (
    <Svg>
      <rect x="4" y="13" width="16" height="7" rx="1" />
      <path d="M12 10V3M8.5 6.5L12 3l3.5 3.5" />
    </Svg>
  ),
  moveDown: (
    <Svg>
      <rect x="4" y="4" width="16" height="7" rx="1" />
      <path d="M12 14v7M8.5 17.5L12 21l3.5-3.5" />
    </Svg>
  ),
  exportVideo: (
    <Svg>
      <rect x="2.5" y="5.5" width="13" height="13" rx="1.5" />
      <path d="M15.5 10l6-3.5v11l-6-3.5" />
      <path d="M7.5 9.5l4 2.5-4 2.5z" />
    </Svg>
  ),
  textPane: (
    <Svg>
      <rect x="2.5" y="4" width="8" height="16" rx="1" />
      <path d="M4.5 8h4M4.5 11.5h4M6 15h2.5" />
      <rect x="13" y="6" width="8.5" height="5" rx="1" />
      <rect x="13" y="13" width="8.5" height="5" rx="1" />
    </Svg>
  ),
  checkAccess: (
    <Svg>
      <circle cx="12" cy="4.5" r="1.8" />
      <path d="M5 8.5l7 1.5 7-1.5M12 10v4.5l-3 6M12 14.5l3 6" />
    </Svg>
  ),
  altText: (
    <Svg>
      <rect x="3" y="4" width="13" height="10" rx="1" />
      <path d="M3 11.5l3.5-3 3 2.5 2-1.5 4.5 3.5" />
      <path d="M13 19h8M13 16h8" />
    </Svg>
  ),
  selectionPane: (
    <Svg>
      <rect x="3.5" y="4" width="17" height="16" rx="1" />
      <path d="M13 4v16M15.5 8h3M15.5 11.5h3M15.5 15h3" />
    </Svg>
  ),
  normalView: (
    <Svg>
      <rect x="3" y="4.5" width="18" height="15" rx="1" />
      <path d="M8 4.5v15M4.5 7.5h2M4.5 10.5h2M4.5 13.5h2" />
    </Svg>
  ),
  sorterView: (
    <Svg>
      <rect x="4" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" />
    </Svg>
  ),
  grid: (
    <Svg>
      <rect x="3.5" y="3.5" width="17" height="17" rx="1" />
      <path d="M9.2 3.5v17M14.8 3.5v17M3.5 9.2h17M3.5 14.8h17" />
    </Svg>
  ),
  slideSize: (
    <Svg>
      <rect x="3" y="6" width="18" height="12" rx="1" />
      <path d="M6 9v-.5h2.5M18 15v.5h-2.5" />
    </Svg>
  ),
  background: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <path d="M3.5 15.5l5-4.5 4 3.5 3-2.5 5 4" />
      <circle cx="15.5" cy="9" r="1.5" />
    </Svg>
  ),
  colors: (
    <Svg>
      <path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.2 0 1.8-.8 1.8-1.6 0-1.2-1-1.5-1-2.6 0-.9.7-1.6 1.7-1.6H17a3.5 3.5 0 0 0 3.5-3.5c0-4.2-3.8-7.7-8.5-7.7z" />
      <circle cx="7.5" cy="11" r="1" />
      <circle cx="10" cy="7.5" r="1" />
      <circle cx="14.5" cy="7.5" r="1" />
    </Svg>
  ),
  fonts: (
    <Svg>
      <path d="M3 18l4.5-12L12 18M4.7 13.5h5.6" />
      <path d="M19.5 18v-5.5a2.5 2.5 0 0 0-4.5-1.5M19.5 15c-.5 2-4.5 3.5-5 1s3-2.5 5-2.5" />
    </Svg>
  ),
  shapes: (
    <Svg>
      <rect x="3.5" y="11" width="8.5" height="8.5" rx="0.5" />
      <circle cx="16" cy="8" r="4.5" />
      <path d="M14 20.5l3.5-6 3.5 6z" />
    </Svg>
  ),
  headerFooter: (
    <Svg>
      <rect x="4" y="3.5" width="16" height="17" rx="1" />
      <path d="M7 17.5h4M15 17.5h2" />
    </Svg>
  ),
  symbol: (
    <Svg>
      <path d="M7 19h4v-2.5a6 6 0 1 1 2 0V19h4" />
    </Svg>
  ),
  rotateRight: (
    <Svg>
      <path d="M18 9a7 7 0 1 0 1 6" />
      <path d="M19 4v5h-5" />
    </Svg>
  ),
  rotateLeft: (
    <Svg>
      <path d="M6 9a7 7 0 1 1-1 6" />
      <path d="M5 4v5h5" />
    </Svg>
  ),
  flipH: (
    <Svg>
      <path d="M12 3v18" strokeDasharray="2 2" />
      <path d="M9 6L3.5 18H9zM15 6l5.5 12H15z" />
    </Svg>
  ),
  flipV: (
    <Svg>
      <path d="M3 12h18" strokeDasharray="2 2" />
      <path d="M6 9L18 3.5V9zM6 15l12 5.5V15z" />
    </Svg>
  ),
  shadow: (
    <Svg>
      <rect x="7" y="7" width="12" height="12" rx="1" fill="currentColor" fillOpacity="0.25" stroke="none" />
      <rect x="4.5" y="4.5" width="12" height="12" rx="1" />
    </Svg>
  ),
  changeShape: (
    <Svg>
      <rect x="3.5" y="11" width="8" height="8" rx="1" />
      <circle cx="16" cy="8" r="4.5" />
      <path d="M12.5 15.5h4.5v-3" />
    </Svg>
  ),
  crop: (
    <Svg>
      <path d="M7 3v14h14M3 7h14v14" />
    </Svg>
  ),
  resetPicture: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <path d="M7 16l3.5-4 2.5 3 2-2 2 3" />
      <circle cx="15.5" cy="9" r="1.3" />
    </Svg>
  ),
  layout: (
    <Svg>
      <rect x="3.5" y="4.5" width="17" height="15" rx="1" />
      <path d="M3.5 9h17M10 9v10.5" />
    </Svg>
  ),
  resetSlide: (
    <Svg>
      <rect x="3.5" y="4.5" width="17" height="15" rx="1" strokeOpacity="0.45" />
      <path d="M9 10.5a4 4 0 1 1 0 4" />
      <path d="M9 8v2.8h2.8" />
    </Svg>
  ),
  find: (
    <Svg>
      <circle cx="10.5" cy="10.5" r="5.5" />
      <path d="M14.5 14.5l5 5" />
    </Svg>
  ),
  replace: (
    <Svg>
      <path d="M4 7h11l-3-3M20 17H9l3 3" />
    </Svg>
  ),
  selectAll: (
    <Svg>
      <rect x="4" y="4" width="16" height="16" rx="1" strokeDasharray="2.5 2" />
      <path d="M8.5 12.5l2.5 2.5 4.5-5.5" />
    </Svg>
  ),
  clearFormat: (
    <Svg>
      <path d="M5 6.5h11M10.5 6.5L8 18" />
      <path d="M14 14l5 5M19 14l-5 5" stroke="#d93025" />
    </Svg>
  ),
  charSpacing: (
    <Svg>
      <path d="M5 13l3-8 3 8M6 10.5h4M13 5l3 8 3-8" />
      <path d="M4 18h16M6 16l-2 2 2 2M18 16l2 2-2 2" />
    </Svg>
  ),
  lineSpacing: (
    <Svg>
      <path d="M11 6.5h9M11 12h9M11 17.5h9" />
      <path d="M6 4v16M4 6l2-2 2 2M4 18l2 2 2-2" />
    </Svg>
  ),
  anchorTop: (
    <Svg>
      <rect x="4" y="3.5" width="16" height="17" rx="1" strokeOpacity="0.45" />
      <path d="M7.5 7.5h9M7.5 10.5h9" />
    </Svg>
  ),
  anchorMiddle: (
    <Svg>
      <rect x="4" y="3.5" width="16" height="17" rx="1" strokeOpacity="0.45" />
      <path d="M7.5 10.5h9M7.5 13.5h9" />
    </Svg>
  ),
  anchorBottom: (
    <Svg>
      <rect x="4" y="3.5" width="16" height="17" rx="1" strokeOpacity="0.45" />
      <path d="M7.5 13.5h9M7.5 16.5h9" />
    </Svg>
  ),
  spelling: (
    <Svg>
      <path d="M4 14l3.5-9 3.5 9M5.3 11h4.4" />
      <path d="M13 12.5l2.5 2.5 5-6" />
      <path d="M3.5 18.5c1 .9 2 .9 3 0s2-.9 3 0 2 .9 3 0 2-.9 3 0 2 .9 3 0" stroke="#d93025" />
    </Svg>
  ),
  notes: (
    <Svg>
      <rect x="4" y="3.5" width="16" height="17" rx="1" />
      <rect x="7" y="6" width="10" height="5.5" />
      <path d="M7 14.5h10M7 17.5h7" />
    </Svg>
  ),
  playStart: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="12" rx="1" />
      <path d="M10.5 8.5v5l4-2.5z" fill="currentColor" />
      <path d="M8 20.5h8" />
    </Svg>
  ),
  presenter: (
    <Svg>
      <rect x="2.5" y="4" width="12" height="9" rx="1" />
      <rect x="16" y="4" width="5.5" height="4" rx="0.5" />
      <path d="M16 11h5.5M16 13.5h4M2.5 16.5h12M2.5 19h8" />
    </Svg>
  ),
  readingView: (
    <Svg>
      <rect x="3.5" y="5" width="7.5" height="14" rx="1.5" />
      <rect x="13" y="5" width="7.5" height="14" rx="1.5" />
    </Svg>
  ),
  formatPane: (
    <Svg>
      <rect x="3" y="4" width="18" height="16" rx="1.5" />
      <path d="M14.5 4v16" />
      <path d="M17 8h2M17 11h2M17 14h2" />
    </Svg>
  ),
  record: (
    <Svg>
      <circle cx="12" cy="12" r="7" />
      <circle cx="12" cy="12" r="3.5" fill="currentColor" />
    </Svg>
  ),
  clearNarration: (
    <Svg>
      <rect x="9" y="4" width="6" height="10" rx="3" />
      <path d="M6 11a6 6 0 0 0 12 0M12 17v3M4 4l16 16" />
    </Svg>
  ),
  playCurrent: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="12" rx="1" />
      <path d="M10.5 8.5v5l4-2.5z" fill="currentColor" />
      <path d="M6.5 8v6" />
      <path d="M8 20.5h8" />
    </Svg>
  ),
  borders: (
    <Svg>
      <rect x="4" y="4" width="16" height="16" />
      <path d="M12 4v16M4 12h16" strokeDasharray="2 2" />
    </Svg>
  ),
  merge: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <path d="M7 12h10M14.5 9.5 17 12l-2.5 2.5M9.5 9.5 7 12l2.5 2.5" />
    </Svg>
  ),
  split: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <path d="M12 5v14" />
      <path d="M9 12H6M15 12h3" strokeDasharray="1.5 1.5" />
    </Svg>
  ),
  alignTop: (
    <Svg>
      <path d="M5 4.5h14M8 9h8M8 12.5h8" />
    </Svg>
  ),
  alignMiddle: (
    <Svg>
      <path d="M8 10h8M8 13.5h8M5 4.5h14M5 19.5h14" strokeOpacity="1" />
    </Svg>
  ),
  alignBottom: (
    <Svg>
      <path d="M5 19.5h14M8 11h8M8 14.5h8" />
    </Svg>
  ),
  numberList: (
    <Svg>
      <path d="M10 6.5h10M10 12h10M10 17.5h10" />
      <path d="M4 5l1.5-1v5M3.8 13.5c0-1 2.6-1.4 2.6 0 0 1-2.6 1.8-2.6 2.8h2.7" strokeWidth="1.2" />
    </Svg>
  ),
  link: (
    <Svg>
      <path d="M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1" />
      <path d="M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1" />
    </Svg>
  ),
  group: (
    <Svg>
      <rect x="3.5" y="3.5" width="17" height="17" rx="1" strokeDasharray="2 2" />
      <rect x="6.5" y="6.5" width="6" height="6" />
      <circle cx="15" cy="15" r="3" />
    </Svg>
  ),
  ungroup: (
    <Svg>
      <rect x="4" y="4" width="7" height="7" />
      <circle cx="16.5" cy="16.5" r="3.5" />
      <path d="M14 6h5v5M6 14v5h5" strokeDasharray="2 2" />
    </Svg>
  ),
  table: (
    <Svg>
      <rect x="3.5" y="4.5" width="17" height="15" rx="1" />
      <path d="M3.5 9.5h17M3.5 14.5h17M9.5 4.5v15M15 4.5v15" />
    </Svg>
  ),
  smartArt: (
    <Svg>
      <rect x="9" y="3.5" width="6" height="5" rx="1" />
      <rect x="3.5" y="15.5" width="6" height="5" rx="1" />
      <rect x="14.5" y="15.5" width="6" height="5" rx="1" />
      <path d="M12 8.5v3.5M6.5 15.5V12h11v3.5" />
    </Svg>
  ),
  rowAbove: (
    <Svg>
      <rect x="4" y="11" width="16" height="9" rx="1" />
      <path d="M4 15.5h16M12 3v6M9 6h6" />
    </Svg>
  ),
  rowBelow: (
    <Svg>
      <rect x="4" y="4" width="16" height="9" rx="1" />
      <path d="M4 8.5h16M12 15v6M9 18h6" />
    </Svg>
  ),
  colLeft: (
    <Svg>
      <rect x="11" y="4" width="9" height="16" rx="1" />
      <path d="M15.5 4v16M3 12h6M6 9v6" />
    </Svg>
  ),
  colRight: (
    <Svg>
      <rect x="4" y="4" width="9" height="16" rx="1" />
      <path d="M8.5 4v16M15 12h6M18 9v6" />
    </Svg>
  ),
  deleteRow: (
    <Svg>
      <rect x="4" y="8" width="16" height="8" rx="1" />
      <path d="M9 10.5l6 3M15 10.5l-6 3" />
    </Svg>
  ),
  deleteCol: (
    <Svg>
      <rect x="8" y="4" width="8" height="16" rx="1" />
      <path d="M10.5 9l3 6M13.5 9l-3 6" />
    </Svg>
  ),
  save: (
    <Svg>
      <path d="M5 4h11l3 3v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1z" />
      <path d="M8 4v5h7V4M8 20v-6h8v6" />
    </Svg>
  ),
  paste: (
    <Svg>
      <rect x="5" y="5" width="12" height="15" rx="1" />
      <path d="M9 5V3.5h4V5M9 10h8v10" />
    </Svg>
  ),
  cut: (
    <Svg>
      <circle cx="7" cy="17" r="2.5" />
      <circle cx="17" cy="17" r="2.5" />
      <path d="M8.8 15.2 17 4M15.2 15.2 7 4" />
    </Svg>
  ),
  formatPainter: (
    <Svg>
      <path d="M20 4l-7.5 7.5" />
      <path d="M10.5 9.5l4 4c-.8 3.2-3.2 5-8.5 5 1.8-1.7 1.2-3.7 2.2-5.2z" />
    </Svg>
  ),
  copy: (
    <Svg>
      <rect x="8" y="8" width="11" height="12" rx="1" />
      <path d="M5 16V5a1 1 0 0 1 1-1h9" />
    </Svg>
  ),
  newSlide: (
    <Svg>
      <rect x="3.5" y="6" width="14" height="10" rx="1" />
      <path d="M19.5 9v6M16.5 12h6" />
    </Svg>
  ),
  duplicate: (
    <Svg>
      <rect x="7" y="7" width="13" height="9" rx="1" />
      <path d="M4 13V5a1 1 0 0 1 1-1h11" />
    </Svg>
  ),
  bulletList: (
    <Svg>
      <circle cx="5" cy="7" r="1" fill="currentColor" />
      <circle cx="5" cy="12" r="1" fill="currentColor" />
      <circle cx="5" cy="17" r="1" fill="currentColor" />
      <path d="M9 7h11M9 12h11M9 17h11" />
    </Svg>
  ),
  indent: (
    <Svg>
      <path d="M4 5h16M10 10h10M10 14h10M4 19h16M4 9.5 7 12l-3 2.5" />
    </Svg>
  ),
  outdent: (
    <Svg>
      <path d="M4 5h16M10 10h10M10 14h10M4 19h16M7 9.5 4 12l3 2.5" />
    </Svg>
  ),
  alignLeft: (
    <Svg>
      <path d="M4 6h16M4 10h10M4 14h16M4 18h10" />
    </Svg>
  ),
  alignCenter: (
    <Svg>
      <path d="M4 6h16M7 10h10M4 14h16M7 18h10" />
    </Svg>
  ),
  alignRight: (
    <Svg>
      <path d="M4 6h16M10 10h10M4 14h16M10 18h10" />
    </Svg>
  ),
  alignJustify: (
    <Svg>
      <path d="M4 6h16M4 10h16M4 14h16M4 18h16" />
    </Svg>
  ),
  picture: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="14" rx="1" />
      <circle cx="9" cy="10" r="1.5" />
      <path d="m4 17 5-5 4 4 2.5-2.5L20 18" />
    </Svg>
  ),
  front: (
    <Svg>
      <rect x="4" y="4" width="10" height="10" />
      <rect x="10" y="10" width="10" height="10" fill="currentColor" fillOpacity="0.25" />
    </Svg>
  ),
  back: (
    <Svg>
      <rect x="10" y="10" width="10" height="10" />
      <rect x="4" y="4" width="10" height="10" fill="currentColor" fillOpacity="0.25" />
    </Svg>
  ),
  alignShapes: (
    <Svg>
      <path d="M4 4v16" />
      <rect x="7" y="6" width="12" height="4" />
      <rect x="7" y="14" width="7" height="4" />
    </Svg>
  ),
  fill: (
    <Svg>
      <path d="m5 12 7-7 6 6-7 7z" />
      <path d="M19 15s1.5 2 1.5 3a1.5 1.5 0 0 1-3 0c0-1 1.5-3 1.5-3z" />
    </Svg>
  ),
  outline: (
    <Svg>
      <path d="M15 5l4 4L9 19H5v-4z" />
    </Svg>
  ),
  fontColor: (
    <Svg>
      <path d="m7 17 5-12 5 12M9 12.5h6" />
    </Svg>
  ),
  open: (
    <Svg>
      <path d="M3 7.5V18a1.5 1.5 0 0 0 1.5 1.5h15A1.5 1.5 0 0 0 21 18V9a1.5 1.5 0 0 0-1.5-1.5H12L9.8 5H4.5A1.5 1.5 0 0 0 3 6.5z" />
    </Svg>
  ),
  new: (
    <Svg>
      <rect x="3.5" y="5" width="17" height="12" rx="1" />
      <path d="M12 8.5v5M9.5 11h5M9 20h6" />
    </Svg>
  ),
  undo: (
    <Svg>
      <path d="M9 7 4.5 11 9 15" />
      <path d="M5 11h9.5a4.5 4.5 0 0 1 0 9H12" />
    </Svg>
  ),
  redo: (
    <Svg>
      <path d="m15 7 4.5 4L15 15" />
      <path d="M19 11H9.5a4.5 4.5 0 0 0 0 9H12" />
    </Svg>
  ),
  delete: (
    <Svg>
      <path d="M5 7h14M10 7V5h4v2M6.5 7l.8 12a1 1 0 0 0 1 1h7.4a1 1 0 0 0 1-1l.8-12" />
    </Svg>
  ),
  textbox: (
    <Svg>
      <rect x="3.5" y="4.5" width="17" height="15" rx="1" strokeDasharray="2 2" />
      <path d="M8 9h8M12 9v7" />
    </Svg>
  ),
  rect: (
    <Svg>
      <rect x="4" y="6" width="16" height="12" />
    </Svg>
  ),
  roundRect: (
    <Svg>
      <rect x="4" y="6" width="16" height="12" rx="3.5" />
    </Svg>
  ),
  ellipse: (
    <Svg>
      <ellipse cx="12" cy="12" rx="8.5" ry="6.5" />
    </Svg>
  ),
  triangle: (
    <Svg>
      <path d="M12 5 20.5 19h-17z" />
    </Svg>
  ),
  rightArrow: (
    <Svg>
      <path d="M3.5 9h10V5.5L20.5 12l-7 6.5V15h-10z" />
    </Svg>
  ),
  star5: (
    <Svg>
      <path d="m12 4 2.4 5.2 5.6.6-4.2 3.8 1.2 5.6L12 16.4l-5 2.8 1.2-5.6L4 9.8l5.6-.6z" />
    </Svg>
  ),
  line: (
    <Svg>
      <path d="M5 19 19 5" />
    </Svg>
  ),
  zoomIn: (
    <Svg>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="m15 15 5 5M8 10.5h5M10.5 8v5" />
    </Svg>
  ),
  zoomOut: (
    <Svg>
      <circle cx="10.5" cy="10.5" r="6" />
      <path d="m15 15 5 5M8 10.5h5" />
    </Svg>
  ),
  fit: (
    <Svg>
      <path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5" />
    </Svg>
  ),
};
