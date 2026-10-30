import { useState } from "react";
import { Popover } from "./Popover";
import type { ReactNode } from "react";

/** The current slide's footer items (`App::headerFooter`). */
export interface HeaderFooterInfo {
  date: "none" | "auto" | "fixed";
  dateText: string;
  slideNumber: boolean;
  footer: string | null;
}

export interface HeaderFooterChoice {
  date: "none" | "auto" | "fixed";
  dateText: string;
  slideNumber: boolean;
  footer: boolean;
  footerText: string;
  skipTitle: boolean;
  all: boolean;
}

const today = () => new Date().toLocaleDateString("zh-TW", { year: "numeric", month: "2-digit", day: "2-digit" });

function Form({ initial, onApply, close }: { initial: HeaderFooterInfo; onApply: (c: HeaderFooterChoice) => void; close: () => void }) {
  const [dateOn, setDateOn] = useState(initial.date !== "none");
  const [auto, setAuto] = useState(initial.date !== "fixed");
  const [fixed, setFixed] = useState(initial.date === "fixed" ? initial.dateText : today());
  const [number, setNumber] = useState(initial.slideNumber);
  const [footerOn, setFooterOn] = useState(initial.footer !== null);
  const [footer, setFooter] = useState(initial.footer ?? "");
  const [skipTitle, setSkipTitle] = useState(true);
  const apply = (all: boolean) => {
    onApply({ date: dateOn ? (auto ? "auto" : "fixed") : "none", dateText: auto ? today() : fixed, slideNumber: number, footer: footerOn, footerText: footer, skipTitle, all });
    close();
  };
  return (
    <div className="hf-dialog" role="dialog" aria-label="頁首及頁尾">
      <div className="hf-title">投影片包含</div>
      <label className="hf-row">
        <input type="checkbox" checked={dateOn} onChange={(e) => setDateOn(e.target.checked)} />
        日期及時間
      </label>
      <div className="hf-indent">
        <label className="hf-row">
          <input type="radio" disabled={!dateOn} checked={auto} onChange={() => setAuto(true)} />
          自動更新（{today()}）
        </label>
        <label className="hf-row">
          <input type="radio" disabled={!dateOn} checked={!auto} onChange={() => setAuto(false)} />
          固定
          <input className="hf-text" disabled={!dateOn || auto} value={fixed} onChange={(e) => setFixed(e.target.value)} />
        </label>
      </div>
      <label className="hf-row">
        <input type="checkbox" checked={number} onChange={(e) => setNumber(e.target.checked)} />
        投影片編號
      </label>
      <label className="hf-row">
        <input type="checkbox" checked={footerOn} onChange={(e) => setFooterOn(e.target.checked)} />
        頁尾
      </label>
      <div className="hf-indent">
        <input className="hf-text wide" disabled={!footerOn} value={footer} placeholder="頁尾文字" onChange={(e) => setFooter(e.target.value)} />
      </div>
      <label className="hf-row sep">
        <input type="checkbox" checked={skipTitle} onChange={(e) => setSkipTitle(e.target.checked)} />
        標題投影片中不顯示
      </label>
      <div className="hf-buttons">
        <button onClick={close}>取消</button>
        <button onClick={() => apply(false)}>套用</button>
        <button className="primary" onClick={() => apply(true)}>
          全部套用
        </button>
      </div>
    </div>
  );
}

/** 插入 > 頁首及頁尾: date, slide number and footer for this or every slide. */
export function HeaderFooterDialog({ load, onApply, button }: { load: () => HeaderFooterInfo | null; onApply: (c: HeaderFooterChoice) => void; button: (open: boolean, toggle: () => void) => ReactNode }) {
  return <Popover button={button}>{(close) => <Form initial={load() ?? { date: "none", dateText: "", slideNumber: false, footer: null }} onApply={onApply} close={close} />}</Popover>;
}
