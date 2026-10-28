// Small line icons for the ribbon, drawn on a 24×24 grid.
import type { ReactNode } from "react";

const Svg = ({ children }: { children: ReactNode }) => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export const icons: Record<string, ReactNode> = {
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
