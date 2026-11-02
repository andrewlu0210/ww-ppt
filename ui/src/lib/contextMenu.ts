import { t } from "../i18n";
import type { MenuItem } from "../components/ContextMenu";
import { NUMBERINGS } from "../components/Ribbon";
import type { Command } from "./commands";
import type { Status } from "./status";

/** What the right click landed on (`App::contextTarget`). */
export type ContextKind = "object" | "text" | "blank";

export interface ContextEnv {
  status: Status;
  run: (c: Command) => void;
  layoutNames: string[];
  /** Go to a ribbon tab (by its internal name). */
  openTab: (tab: string) => void;
  openLink: () => void;
  openAltText: () => void;
  endEditing: () => void;
}

const SEP: MenuItem = { sep: true };

/** Items of the right-click menu for what was hit; every item runs an existing command. */
export function buildContextMenu(kind: ContextKind, env: ContextEnv): MenuItem[] {
  const { status, run } = env;
  const hasTarget = status.hasSelection || status.editingText;
  const clipboard: MenuItem[] = [
    { label: t("ribbon.cut"), shortcut: "⌘X", disabled: !hasTarget, onSelect: () => run({ kind: "cut" }) },
    { label: t("ribbon.copy"), shortcut: "⌘C", disabled: !hasTarget, onSelect: () => run({ kind: "copy" }) },
    { label: t("ribbon.paste"), shortcut: "⌘V", disabled: !status.canPaste, onSelect: () => run({ kind: "paste" }) },
  ];
  const textItems: MenuItem[] = [
    { label: t("ctx.font"), onSelect: () => env.openTab("常用") },
    { label: t("ctx.paragraph"), onSelect: () => env.openTab("常用") },
    { label: t("ribbon.bullets"), checked: !!status.bullet, onSelect: () => run({ kind: "bullets" }) },
    {
      label: t("ctx.numberingStyle"),
      items: NUMBERINGS.map(([scheme, sample]) => ({ label: sample, checked: status.numbering === scheme, onSelect: () => run({ kind: "numbering", scheme }) })),
    },
  ];
  const link: MenuItem = { label: t("ctx.hyperlink"), shortcut: "⌘K", onSelect: env.openLink };
  const formatShape: MenuItem = { label: t("ctx.formatShape"), onSelect: () => env.openTab("圖形格式") };

  if (kind === "text") {
    return [...clipboard, SEP, { label: t("ctx.endEditing"), onSelect: env.endEditing }, ...textItems, SEP, link, SEP, formatShape];
  }

  if (kind === "object") {
    const order: [string, "front" | "forward" | "backward" | "back"][] = [
      [t("ribbon.bringToFront"), "front"],
      [t("ribbon.sendToBack"), "back"],
      [t("ribbon.bringForward"), "forward"],
      [t("ribbon.sendBackward"), "backward"],
    ];
    const align: ["left" | "center" | "right" | "top" | "middle" | "bottom", string][] = [
      ["left", t("ribbon.shapesLeft")],
      ["center", t("ribbon.shapesCenter")],
      ["right", t("ribbon.shapesRight")],
      ["top", t("ribbon.shapesTop")],
      ["middle", t("ribbon.shapesMiddle")],
      ["bottom", t("ribbon.shapesBottom")],
    ];
    const out: MenuItem[] = [...clipboard, SEP];
    if (status.hasText) out.push(textItems[0], textItems[1], SEP);
    out.push(
      { label: t("ctx.group"), shortcut: "⌘⌥G", disabled: !status.canGroup, onSelect: () => run({ kind: "group", on: true }) },
      { label: t("ctx.ungroup"), shortcut: "⌘⌥⇧G", disabled: !status.canUngroup, onSelect: () => run({ kind: "group", on: false }) },
      { label: t("ctx.order"), items: order.map(([label, o]) => ({ label, onSelect: () => run({ kind: "reorder", order: o }) })) },
      { label: t("group.align"), items: align.map(([to, label]) => ({ label, onSelect: () => run({ kind: "alignShapes", to }) })) },
      SEP,
      formatShape,
      { label: t("ctx.altText"), disabled: !status.frame || status.selectionCount !== 1, onSelect: env.openAltText },
    );
    if (status.hasText) out.push(SEP, link);
    return out;
  }

  // The empty part of the slide.
  return [
    { label: t("ribbon.paste"), shortcut: "⌘V", disabled: !status.canPaste, onSelect: () => run({ kind: "paste" }) },
    SEP,
    {
      label: t("ctx.layout"),
      items: env.layoutNames.map((label, layout) => ({ label, checked: status.slideLayout === layout, onSelect: () => run({ kind: "applyLayout", layout }) })),
    },
    { label: t("ribbon.reset"), onSelect: () => run({ kind: "resetSlide" }) },
    SEP,
    { label: t("ctx.background"), onSelect: () => env.openTab("設計") },
    SEP,
    { label: t("ribbon.gridlines"), checked: !!status.showGrid, onSelect: () => run({ kind: "showGrid", on: !status.showGrid }) },
    { label: t("ribbon.guides"), checked: !!status.showGuides, onSelect: () => run({ kind: "showGuides", on: !status.showGuides }) },
  ];
}
