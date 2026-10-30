import { useState, type ReactNode } from "react";
import { Popover } from "./Popover";
import type { Command } from "../lib/commands";

export interface AltTextInfo {
  text: string;
  decorative: boolean;
}

function Form({ initial, run, close }: { initial: AltTextInfo; run: (c: Command) => void; close: () => void }) {
  const [text, setText] = useState(initial.text);
  const [decorative, setDecorative] = useState(initial.decorative);
  return (
    <div className="hf-dialog alt-dialog" role="dialog" aria-label="替代文字">
      <div className="hf-title">替代文字</div>
      <div className="hf-note">用一兩句話描述這個物件和它的內容，讓看不到的人透過螢幕閱讀器了解。</div>
      <textarea className="alt-text" rows={4} value={text} disabled={decorative} placeholder="例如：2024 年各季營收長條圖，第四季最高" onChange={(e) => setText(e.target.value)} />
      <label className="hf-row">
        <input type="checkbox" checked={decorative} onChange={(e) => setDecorative(e.target.checked)} />
        標示為裝飾用
      </label>
      <div className="hf-note">裝飾用的物件（例如框線、背景花紋）會被螢幕閱讀器略過。</div>
      <div className="hf-buttons">
        <button onClick={close}>取消</button>
        <button
          className="primary"
          onClick={() => {
            run({ kind: "altText", text: decorative ? "" : text, decorative });
            close();
          }}
        >
          確定
        </button>
      </div>
    </div>
  );
}

/** 替代文字: alternative text and the decorative mark of the selected object. */
export function AltTextDialog({ load, run, button }: { load: () => AltTextInfo | null; run: (c: Command) => void; button: (open: boolean, toggle: () => void) => ReactNode }) {
  return <Popover button={button}>{(close) => <Form initial={load() ?? { text: "", decorative: false }} run={run} close={close} />}</Popover>;
}
