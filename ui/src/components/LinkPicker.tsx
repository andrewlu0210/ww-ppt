import { useEffect, useRef, useState, type ReactNode } from "react";

/**
 * Hyperlink panel: an address field with apply / remove. Opens from its button
 * or whenever `request` changes (the ⌘K menu item).
 */
export function LinkPicker(props: { current?: string; disabled?: boolean; request: number; button: (open: boolean, toggle: () => void) => ReactNode; onApply: (url: string) => void }) {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const first = useRef(true);

  useEffect(() => {
    // Ignore the initial render; react to later ⌘K requests.
    if (first.current) {
      first.current = false;
      return;
    }
    if (!props.disabled) setOpen(true);
  }, [props.request, props.disabled]);
  useEffect(() => {
    if (!open) return;
    setUrl(props.current ?? "");
    setTimeout(() => input.current?.select(), 0);
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open, props.current]);

  const apply = (value: string) => {
    props.onApply(value);
    setOpen(false);
  };
  return (
    <div className="popover-anchor" ref={ref}>
      {props.button(open, () => !props.disabled && setOpen((o) => !o))}
      {open && (
        <div className="popover link-panel">
          <label className="rb-field">
            網址
            <input
              ref={input}
              className="link-input"
              value={url}
              placeholder="https://"
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") apply(url.trim());
                if (e.key === "Escape") setOpen(false);
              }}
            />
          </label>
          <div className="link-actions">
            {props.current && <button onClick={() => apply("")}>移除連結</button>}
            <button className="primary" disabled={!url.trim()} onClick={() => apply(url.trim())}>
              確定
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
