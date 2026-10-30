// Small line icons for the ribbon, drawn on a 24×24 grid.
import type { ReactNode } from "react";

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export const icons: Record<string, ReactNode> = {
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
