import type { ReactNode } from "react";
import { Popover } from "./Popover";

/** A preset's gallery icon (`App::presetIcons`). */
export interface PresetIcon {
  name: string;
  viewBox: string;
  paths: { d: string; fill: boolean; stroke: boolean }[];
}

/** The gallery's sections, in PowerPoint's order. `arrowLine` is a line with an arrowhead. */
export const SHAPE_CATEGORIES: [string, string[]][] = [
  ["線條", ["line", "arrowLine"]],
  ["矩形", ["rect", "roundRect", "snip1Rect", "snip2SameRect", "snipRoundRect", "round1Rect", "round2SameRect"]],
  [
    "基本圖案",
    [
      "ellipse", "triangle", "rtTriangle", "parallelogram", "trapezoid", "diamond", "pentagon", "hexagon", "heptagon", "octagon", "decagon", "dodecagon",
      "pie", "chord", "teardrop", "frame", "halfFrame", "corner", "diagStripe", "plus", "plaque", "can", "cube", "bevel", "donut", "noSmoking", "blockArc",
      "foldedCorner", "smileyFace", "heart", "lightningBolt", "sun", "moon", "cloud", "arc", "bracketPair", "bracePair", "leftBracket", "rightBracket", "leftBrace", "rightBrace",
    ],
  ],
  [
    "箭號圖案",
    [
      "rightArrow", "leftArrow", "upArrow", "downArrow", "leftRightArrow", "upDownArrow", "quadArrow", "leftRightUpArrow", "bentArrow", "uturnArrow", "leftUpArrow",
      "bentUpArrow", "curvedRightArrow", "curvedLeftArrow", "curvedUpArrow", "curvedDownArrow", "stripedRightArrow", "notchedRightArrow", "homePlate", "chevron",
      "rightArrowCallout", "downArrowCallout", "leftArrowCallout", "upArrowCallout", "leftRightArrowCallout", "quadArrowCallout", "circularArrow",
    ],
  ],
  ["方程式圖形", ["mathPlus", "mathMinus", "mathMultiply", "mathDivide", "mathEqual", "mathNotEqual"]],
  [
    "流程圖",
    [
      "flowChartProcess", "flowChartAlternateProcess", "flowChartDecision", "flowChartInputOutput", "flowChartPredefinedProcess", "flowChartInternalStorage",
      "flowChartDocument", "flowChartMultidocument", "flowChartTerminator", "flowChartPreparation", "flowChartManualInput", "flowChartManualOperation",
      "flowChartConnector", "flowChartOffpageConnector", "flowChartPunchedCard", "flowChartPunchedTape", "flowChartSummingJunction", "flowChartOr",
      "flowChartCollate", "flowChartSort", "flowChartExtract", "flowChartMerge", "flowChartOnlineStorage", "flowChartDelay", "flowChartMagneticTape",
      "flowChartMagneticDisk", "flowChartMagneticDrum", "flowChartDisplay",
    ],
  ],
  [
    "星星及綵帶",
    [
      "irregularSeal1", "irregularSeal2", "star4", "star5", "star6", "star7", "star8", "star10", "star12", "star16", "star24", "star32",
      "ribbon2", "ribbon", "ellipseRibbon2", "ellipseRibbon", "verticalScroll", "horizontalScroll", "wave", "doubleWave",
    ],
  ],
  [
    "圖說文字",
    [
      "wedgeRectCallout", "wedgeRoundRectCallout", "wedgeEllipseCallout", "cloudCallout", "borderCallout1", "borderCallout2", "borderCallout3",
      "accentCallout1", "accentCallout2", "accentCallout3", "callout1", "callout2", "callout3", "accentBorderCallout1", "accentBorderCallout2", "accentBorderCallout3",
    ],
  ],
];

/** Names of the common shapes (others show their preset id). */
const NAMES: Record<string, string> = {
  line: "直線", arrowLine: "直線箭號", rect: "矩形", roundRect: "圓角矩形", snip1Rect: "剪去單一角落矩形", snip2SameRect: "剪去同側角落矩形",
  snipRoundRect: "剪去和圓角單一角落矩形", round1Rect: "圓角單一角落矩形", round2SameRect: "圓角同側角落矩形", ellipse: "橢圓", triangle: "等腰三角形",
  rtTriangle: "直角三角形", parallelogram: "平行四邊形", trapezoid: "梯形", diamond: "菱形", pentagon: "規則五邊形", hexagon: "六邊形", heptagon: "七邊形",
  octagon: "八邊形", decagon: "十邊形", dodecagon: "十二邊形", pie: "圓形圖", chord: "弦形", teardrop: "淚滴", frame: "框架", halfFrame: "半框架",
  corner: "L 形", diagStripe: "斜紋", plus: "十字形", plaque: "牌匾", can: "圓柱", cube: "立方體", bevel: "斜角", donut: "圓環", noSmoking: "禁止符號",
  blockArc: "空心弧", foldedCorner: "摺角", smileyFace: "笑臉", heart: "心型", lightningBolt: "閃電", sun: "太陽", moon: "月亮", cloud: "雲朵", arc: "弧形",
  bracketPair: "雙括弧", bracePair: "雙大括號", leftBracket: "左中括號", rightBracket: "右中括號", leftBrace: "左大括號", rightBrace: "右大括號",
  rightArrow: "向右箭號", leftArrow: "向左箭號", upArrow: "向上箭號", downArrow: "向下箭號", leftRightArrow: "左右雙向箭號", upDownArrow: "上下雙向箭號",
  quadArrow: "四向箭號", bentArrow: "彎曲箭號", uturnArrow: "迴轉箭號", chevron: "V 形箭號", homePlate: "五邊形箭號", notchedRightArrow: "燕尾形向右箭號",
  stripedRightArrow: "向右條紋箭號", circularArrow: "環形箭號", mathPlus: "加號", mathMinus: "減號", mathMultiply: "乘號", mathDivide: "除號",
  mathEqual: "等號", mathNotEqual: "不等號", flowChartProcess: "流程圖：程序", flowChartDecision: "流程圖：決策", flowChartTerminator: "流程圖：結束點",
  flowChartDocument: "流程圖：文件", flowChartInputOutput: "流程圖：資料", flowChartConnector: "流程圖：接點", star4: "星形：四角", star5: "星形：五角",
  star6: "星形：六角", star8: "星形：八角", star10: "星形：十角", star12: "星形：十二角", irregularSeal1: "爆炸：八點", irregularSeal2: "爆炸：十四點",
  wave: "波浪", doubleWave: "雙波浪", verticalScroll: "垂直捲軸", horizontalScroll: "水平捲軸", wedgeRectCallout: "圖說文字：矩形",
  wedgeRoundRectCallout: "圖說文字：圓角矩形", wedgeEllipseCallout: "圖說文字：橢圓形", cloudCallout: "思考泡泡：雲朵",
};

export const shapeName = (name: string) => NAMES[name] ?? name;

function Icon({ icon }: { icon?: PresetIcon }) {
  if (!icon) return <svg width="22" height="18" />;
  return (
    <svg width="22" height="18" viewBox={icon.viewBox} aria-hidden="true">
      {icon.paths.map((p, i) => (
        <path key={i} d={p.d} fill={p.fill ? "currentColor" : "none"} fillOpacity={p.fill ? 0.18 : 0} stroke={p.stroke ? "currentColor" : "none"} strokeWidth={1} vectorEffect="non-scaling-stroke" />
      ))}
    </svg>
  );
}

/**
 * PowerPoint's shape gallery. `onPick` gets the preset (or `arrowLine`);
 * `lines` includes the line tools (not for changing a shape).
 */
export function ShapeGallery({ icons, onPick, button, lines = true, active }: { icons: Map<string, PresetIcon>; onPick: (name: string) => void; button: (open: boolean, toggle: () => void) => ReactNode; lines?: boolean; active?: string }) {
  return (
    <Popover button={button}>
      {(close) => (
        <div className="shape-gallery" onMouseDown={(e) => e.preventDefault()}>
          {SHAPE_CATEGORIES.filter(([title]) => lines || title !== "線條").map(([title, names]) => (
            <div key={title} className="shape-section">
              <div className="shape-section-title">{title}</div>
              <div className="shape-grid">
                {names.map((n) => (
                  <button
                    key={n}
                    className={n === active ? "shape-item active" : "shape-item"}
                    title={shapeName(n)}
                    aria-label={shapeName(n)}
                    onClick={() => {
                      onPick(n);
                      close();
                    }}
                  >
                    <Icon icon={icons.get(n)} />
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </Popover>
  );
}
