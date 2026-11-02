import { useEffect, useRef, useState } from "react";

interface Props {
  /** Class of the text field (carries the width). */
  className: string;
  /** The current value shown when the field is not being edited. */
  value: string;
  options: string[];
  disabled?: boolean;
  placeholder?: string;
  /** Called with the chosen or typed value. */
  onCommit: (value: string) => void;
  /** Draws each option in its own typeface (font names). */
  previewFonts?: boolean;
  /** Accepts only values from the list when typing (otherwise any non-empty text). */
  listOnly?: boolean;
  ariaLabel?: string;
}

/** A text field with a drop-down list: type a value or pick one. */
export function Combobox({ className, value, options, disabled, placeholder, onCommit, previewFonts, listOnly, ariaLabel }: Props) {
  const [draft, setDraft] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const root = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  const text = draft ?? value;
  // The option to highlight: the typed text's prefix match, else the current value.
  const match = (s: string) => {
    const q = s.trim().toLowerCase();
    if (!q) return -1;
    const exact = options.findIndex((o) => o.toLowerCase() === q);
    return exact >= 0 ? exact : options.findIndex((o) => o.toLowerCase().startsWith(q));
  };

  const close = () => {
    setOpen(false);
    setDraft(null);
    setActive(-1);
  };
  const commit = (v: string) => {
    close();
    if (v.trim()) onCommit(v.trim());
  };
  const show = () => {
    setOpen(true);
    setActive(match(value));
  };

  // Keep the highlighted option in view.
  useEffect(() => {
    if (!open || active < 0) return;
    (list.current?.children[active] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) close();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);

  return (
    <div className="combo" ref={root}>
      <input
        ref={input}
        className={className}
        role="combobox"
        aria-expanded={open}
        aria-label={ariaLabel}
        value={text}
        disabled={disabled}
        placeholder={placeholder}
        onFocus={(e) => {
          setDraft(e.target.value);
          e.target.select();
        }}
        onChange={(e) => {
          setDraft(e.target.value);
          if (!open) setOpen(true);
          setActive(match(e.target.value));
        }}
        onBlur={() => {
          // A click on a list item keeps focus (mousedown is prevented), so this is a real exit.
          if (!open) setDraft(null);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            if (!open) return show();
            const step = e.key === "ArrowDown" ? 1 : -1;
            setActive((a) => Math.min(options.length - 1, Math.max(0, a + step)));
          } else if (e.key === "Enter") {
            e.preventDefault();
            if (open && active >= 0) commit(options[active]);
            else if (draft && (!listOnly || match(draft) >= 0)) commit(listOnly ? options[match(draft)] : draft);
            else close();
            input.current?.blur();
          } else if (e.key === "Escape") {
            e.preventDefault();
            close();
            input.current?.blur();
          }
        }}
      />
      <button
        type="button"
        className="combo-arrow"
        tabIndex={-1}
        disabled={disabled}
        aria-label={ariaLabel}
        onMouseDown={(e) => {
          // Keep the focus where it is (the canvas or the field).
          e.preventDefault();
          if (open) close();
          else {
            input.current?.focus();
            show();
          }
        }}
      >
        ▾
      </button>
      {open && (
        <div className="combo-list" ref={list} role="listbox" onMouseDown={(e) => e.preventDefault()}>
          {options.map((o, i) => (
            <div
              key={o}
              role="option"
              aria-selected={o === value}
              className={["combo-option", i === active && "active", o === value && "current"].filter(Boolean).join(" ")}
              style={previewFonts ? { fontFamily: `"${o}", sans-serif` } : undefined}
              onMouseEnter={() => setActive(i)}
              onClick={() => commit(o)}
            >
              {o}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
