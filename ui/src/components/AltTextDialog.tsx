import { t } from "../i18n";
import { useState, type ReactNode } from "react";
import { Popover } from "./Popover";
import type { Command } from "../lib/commands";

export interface AltTextInfo {
  text: string;
  decorative: boolean;
}

export function AltTextForm({ initial, run, close }: { initial: AltTextInfo; run: (c: Command) => void; close: () => void }) {
  const [text, setText] = useState(initial.text);
  const [decorative, setDecorative] = useState(initial.decorative);
  return (
    <div className="hf-dialog alt-dialog" role="dialog" aria-label={t("ribbon.altText")}>
      <div className="hf-title">{t("ribbon.altText")}</div>
      <div className="hf-note">{t("alt.help")}</div>
      <textarea className="alt-text" rows={4} value={text} disabled={decorative} placeholder={t("alt.example")} onChange={(e) => setText(e.target.value)} />
      <label className="hf-row">
        <input type="checkbox" checked={decorative} onChange={(e) => setDecorative(e.target.checked)} />
        {t("checker.markDecorative")}
      </label>
      <div className="hf-note">{t("alt.decorativeHelp")}</div>
      <div className="hf-buttons">
        <button onClick={close}>{t("save.cancel")}</button>
        <button
          className="primary"
          onClick={() => {
            run({ kind: "altText", text: decorative ? "" : text, decorative });
            close();
          }}
        >
          {t("common.ok")}
        </button>
      </div>
    </div>
  );
}

/** 替代文字: alternative text and the decorative mark of the selected object. */
export function AltTextDialog({ load, run, button }: { load: () => AltTextInfo | null; run: (c: Command) => void; button: (open: boolean, toggle: () => void) => ReactNode }) {
  return <Popover button={button}>{(close) => <AltTextForm initial={load() ?? { text: "", decorative: false }} run={run} close={close} />}</Popover>;
}
