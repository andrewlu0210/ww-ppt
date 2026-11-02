import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** One row of a context menu: a separator, or an item that runs, toggles or opens a submenu. */
export type MenuItem =
  | { sep: true }
  | {
      sep?: false;
      label: string;
      /** Shown right-aligned, e.g. "⌘X". */
      shortcut?: string;
      disabled?: boolean;
      /** Shows a check mark and exposes the item as a checkbox. */
      checked?: boolean;
      bold?: boolean;
      onSelect?: () => void;
      items?: MenuItem[];
    };

interface Props {
  /** Where the menu opens, in pixels of its positioned parent. */
  x: number;
  y: number;
  items: MenuItem[];
  label: string;
  onClose: () => void;
}

const enabled = (it: MenuItem) => !it.sep && !it.disabled;

/** The first enabled index at or after `from` going in `dir` (wrapping); -1 when none. */
function step(items: MenuItem[], from: number, dir: 1 | -1): number {
  const n = items.length;
  for (let k = 1; k <= n; k++) {
    const i = (((from + dir * k) % n) + n) % n;
    if (enabled(items[i])) return i;
  }
  return -1;
}

/** The list at the end of the focus path, and the items along the way. */
function levels(root: MenuItem[], path: number[]): MenuItem[][] {
  const out = [root];
  for (let d = 0; d < path.length - 1; d++) {
    const it = out[d][path[d]];
    if (!it || it.sep || !it.items) break;
    out.push(it.items);
  }
  return out;
}

/**
 * A right-click menu with separators, shortcuts, check marks and submenus. Arrow keys move,
 * → opens and ← closes a submenu, Enter runs, Esc closes. Keyboard focus moves into the menu
 * while it is open and goes back to where it was when it closes.
 */
export function ContextMenu({ x, y, items, label, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ left: x, top: y });
  /** Focused index per open level; the last entry is the list the keyboard acts on. */
  const [path, setPath] = useState<number[]>(() => [Math.max(0, items.findIndex(enabled))]);
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const pathRef = useRef(path);
  pathRef.current = path;

  // Keep the menu inside its container, and move keyboard focus into it.
  useLayoutEffect(() => {
    const el = ref.current;
    const parent = el?.offsetParent as HTMLElement | null;
    if (!el || !parent) return;
    const left = Math.max(0, Math.min(x, parent.clientWidth - el.offsetWidth - 2));
    const top = Math.max(0, Math.min(y, parent.clientHeight - el.offsetHeight - 2));
    setPos({ left, top });
  }, [x, y, items.length]);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    ref.current?.focus({ preventScroll: true });
    return () => previous?.focus?.({ preventScroll: true });
  }, []);

  const run = (it: MenuItem) => {
    if (it.sep || it.disabled || it.items) return;
    onClose();
    it.onSelect?.();
  };

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      const p = pathRef.current;
      const lv = levels(itemsRef.current, p);
      const depth = lv.length - 1;
      const list = lv[depth];
      const at = p[depth];
      const set = (i: number) => i >= 0 && setPath([...p.slice(0, depth), i]);
      let handled = true;
      switch (e.key) {
        case "Escape":
          if (depth > 0) setPath(p.slice(0, depth));
          else onClose();
          break;
        case "ArrowDown":
          set(step(list, at, 1));
          break;
        case "ArrowUp":
          set(step(list, at, -1));
          break;
        case "ArrowRight": {
          const it = list[at];
          if (it && !it.sep && it.items && !it.disabled) setPath([...p, Math.max(0, it.items.findIndex(enabled))]);
          break;
        }
        case "ArrowLeft":
          if (depth > 0) setPath(p.slice(0, depth));
          break;
        case "Enter":
        case " ": {
          const it = list[at];
          if (!it || it.sep || it.disabled) break;
          if (it.items) setPath([...p, Math.max(0, it.items.findIndex(enabled))]);
          else run(it);
          break;
        }
        case "Tab":
          onClose();
          break;
        default:
          handled = false;
      }
      if (handled) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onClose]);

  const activeId = `ctx-${path.join("-")}`;
  return (
    <div ref={ref} className="ctx-root" tabIndex={-1} style={{ left: pos.left, top: pos.top }} onContextMenu={(e) => e.preventDefault()}>
      <MenuList items={items} path={path} depth={0} label={label} activeId={activeId} setPath={setPath} run={run} />
    </div>
  );
}

interface ListProps {
  items: MenuItem[];
  path: number[];
  depth: number;
  label: string;
  activeId: string;
  setPath: (p: number[]) => void;
  run: (it: MenuItem) => void;
}

function MenuList({ items, path, depth, label, activeId, setPath, run }: ListProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [flip, setFlip] = useState(false);
  // A submenu opens to the left when there is no room on the right.
  useLayoutEffect(() => {
    if (depth === 0) return;
    const el = ref.current;
    const host = el?.closest(".canvas-area") as HTMLElement | null;
    if (!el || !host) return;
    setFlip(el.getBoundingClientRect().right > host.getBoundingClientRect().right);
  }, [depth]);
  const here = path.length - 1 === depth;
  return (
    <div
      ref={ref}
      className={depth === 0 ? "ctx-menu" : flip ? "ctx-menu ctx-sub flip" : "ctx-menu ctx-sub"}
      role="menu"
      aria-label={label}
      aria-activedescendant={here ? activeId : undefined}
    >
      {items.map((it, i) => {
        if (it.sep) return <div key={i} className="ctx-sep" role="separator" />;
        const id = `ctx-${[...path.slice(0, depth), i].join("-")}`;
        const open = !!it.items && path[depth] === i && path.length > depth + 1;
        const focused = path[depth] === i;
        const cls = ["ctx-item", it.disabled && "disabled", focused && here && "focused", focused && !here && "parent", it.bold && "bold"].filter(Boolean).join(" ");
        return (
          <div key={i} className="ctx-row">
            <div
              id={id}
              className={cls}
              role={it.checked !== undefined ? "menuitemcheckbox" : "menuitem"}
              aria-checked={it.checked}
              aria-disabled={it.disabled || undefined}
              aria-haspopup={it.items ? "menu" : undefined}
              aria-expanded={it.items ? open : undefined}
              onMouseEnter={() => {
                if (it.disabled) return;
                const p = path.slice(0, depth);
                setPath(it.items ? [...p, i, Math.max(0, it.items.findIndex(enabled))] : [...p, i]);
              }}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => run(it)}
            >
              <span className="ctx-check" aria-hidden="true">
                {it.checked ? "✓" : ""}
              </span>
              <span className="ctx-label">{it.label}</span>
              {it.shortcut && <span className="ctx-shortcut">{it.shortcut}</span>}
              {it.items && <span className="ctx-arrow" aria-hidden="true">›</span>}
            </div>
            {open && it.items && (
              <MenuList items={it.items} path={path} depth={depth + 1} label={it.label} activeId={activeId} setPath={setPath} run={run} />
            )}
          </div>
        );
      })}
    </div>
  );
}
