import type { ReactNode } from "react";
import { Popover } from "./Popover";
import { Thumb } from "./Thumb";

interface Props {
  names: string[];
  thumbs: (ImageData | undefined)[];
  onOpen: () => void;
  onPick: (layout: number) => void;
  /** Highlighted layout (the current slide's). */
  current?: number;
  button: (open: boolean, toggle: () => void) => ReactNode;
}

/** Gallery of the deck's layouts, like PowerPoint's "New Slide" menu. */
export function LayoutPicker({ names, thumbs, onOpen, onPick, button, current }: Props) {
  return (
    <Popover
      button={(open, toggle) =>
        button(open, () => {
          if (!open) onOpen();
          toggle();
        })
      }
    >
      {(close) => (
        <div className="layout-panel" onMouseDown={(e) => e.preventDefault()}>
          {names.map((name, i) => (
            <button
              key={i}
              className={i === current ? "layout-item active" : "layout-item"}
              onClick={() => {
                onPick(i);
                close();
              }}
            >
              <Thumb image={thumbs[i]} className="layout-thumb" />
              <span>{name}</span>
            </button>
          ))}
        </div>
      )}
    </Popover>
  );
}
