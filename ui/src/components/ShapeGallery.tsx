import { t } from "../i18n";
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
  [t("shape.catLines"), ["line", "arrowLine"]],
  [t("shape.catRectangles"), ["rect", "roundRect", "snip1Rect", "snip2SameRect", "snipRoundRect", "round1Rect", "round2SameRect"]],
  [
    t("shape.catBasic"),
    [
      "ellipse", "triangle", "rtTriangle", "parallelogram", "trapezoid", "diamond", "pentagon", "hexagon", "heptagon", "octagon", "decagon", "dodecagon",
      "pie", "chord", "teardrop", "frame", "halfFrame", "corner", "diagStripe", "plus", "plaque", "can", "cube", "bevel", "donut", "noSmoking", "blockArc",
      "foldedCorner", "smileyFace", "heart", "lightningBolt", "sun", "moon", "cloud", "arc", "bracketPair", "bracePair", "leftBracket", "rightBracket", "leftBrace", "rightBrace",
    ],
  ],
  [
    t("shape.catArrows"),
    [
      "rightArrow", "leftArrow", "upArrow", "downArrow", "leftRightArrow", "upDownArrow", "quadArrow", "leftRightUpArrow", "bentArrow", "uturnArrow", "leftUpArrow",
      "bentUpArrow", "curvedRightArrow", "curvedLeftArrow", "curvedUpArrow", "curvedDownArrow", "stripedRightArrow", "notchedRightArrow", "homePlate", "chevron",
      "rightArrowCallout", "downArrowCallout", "leftArrowCallout", "upArrowCallout", "leftRightArrowCallout", "quadArrowCallout", "circularArrow",
    ],
  ],
  [t("shape.catEquation"), ["mathPlus", "mathMinus", "mathMultiply", "mathDivide", "mathEqual", "mathNotEqual"]],
  [
    t("shape.catFlowchart"),
    [
      "flowChartProcess", "flowChartAlternateProcess", "flowChartDecision", "flowChartInputOutput", "flowChartPredefinedProcess", "flowChartInternalStorage",
      "flowChartDocument", "flowChartMultidocument", "flowChartTerminator", "flowChartPreparation", "flowChartManualInput", "flowChartManualOperation",
      "flowChartConnector", "flowChartOffpageConnector", "flowChartPunchedCard", "flowChartPunchedTape", "flowChartSummingJunction", "flowChartOr",
      "flowChartCollate", "flowChartSort", "flowChartExtract", "flowChartMerge", "flowChartOnlineStorage", "flowChartDelay", "flowChartMagneticTape",
      "flowChartMagneticDisk", "flowChartMagneticDrum", "flowChartDisplay",
    ],
  ],
  [
    t("shape.catStars"),
    [
      "irregularSeal1", "irregularSeal2", "star4", "star5", "star6", "star7", "star8", "star10", "star12", "star16", "star24", "star32",
      "ribbon2", "ribbon", "ellipseRibbon2", "ellipseRibbon", "verticalScroll", "horizontalScroll", "wave", "doubleWave",
    ],
  ],
  [
    t("shape.catCallouts"),
    [
      "wedgeRectCallout", "wedgeRoundRectCallout", "wedgeEllipseCallout", "cloudCallout", "borderCallout1", "borderCallout2", "borderCallout3",
      "accentCallout1", "accentCallout2", "accentCallout3", "callout1", "callout2", "callout3", "accentBorderCallout1", "accentBorderCallout2", "accentBorderCallout3",
    ],
  ],
];

/** Names of the common shapes (others show their preset id). */
const NAMES: Record<string, string> = {
  line: t("shape.line"), arrowLine: t("shape.arrowLine"), rect: t("shape.rect"), roundRect: t("shape.roundRect"), snip1Rect: t("shape.snip1Rect"), snip2SameRect: t("shape.snip2SameRect"),
  snipRoundRect: t("shape.snipRoundRect"), round1Rect: t("shape.round1Rect"), round2SameRect: t("shape.round2SameRect"), ellipse: t("shape.ellipse"), triangle: t("shape.triangle"),
  rtTriangle: t("shape.rtTriangle"), parallelogram: t("shape.parallelogram"), trapezoid: t("shape.trapezoid"), diamond: t("shape.diamond"), pentagon: t("shape.pentagon"), hexagon: t("shape.hexagon"), heptagon: t("shape.heptagon"),
  octagon: t("shape.octagon"), decagon: t("shape.decagon"), dodecagon: t("shape.dodecagon"), pie: t("shape.pie"), chord: t("shape.chord"), teardrop: t("shape.teardrop"), frame: t("shape.frame"), halfFrame: t("shape.halfFrame"),
  corner: t("shape.corner"), diagStripe: t("shape.diagStripe"), plus: t("shape.plus"), plaque: t("shape.plaque"), can: t("shape.can"), cube: t("shape.cube"), bevel: t("shape.bevel"), donut: t("shape.donut"), noSmoking: t("shape.noSmoking"),
  blockArc: t("shape.blockArc"), foldedCorner: t("shape.foldedCorner"), smileyFace: t("shape.smileyFace"), heart: t("shape.heart"), lightningBolt: t("shape.lightningBolt"), sun: t("shape.sun"), moon: t("shape.moon"), cloud: t("shape.cloud"), arc: t("shape.arc"),
  bracketPair: t("shape.bracketPair"), bracePair: t("shape.bracePair"), leftBracket: t("shape.leftBracket"), rightBracket: t("shape.rightBracket"), leftBrace: t("shape.leftBrace"), rightBrace: t("shape.rightBrace"),
  rightArrow: t("shape.rightArrow"), leftArrow: t("shape.leftArrow"), upArrow: t("shape.upArrow"), downArrow: t("shape.downArrow"), leftRightArrow: t("shape.leftRightArrow"), upDownArrow: t("shape.upDownArrow"),
  quadArrow: t("shape.quadArrow"), bentArrow: t("shape.bentArrow"), uturnArrow: t("shape.uturnArrow"), chevron: t("shape.chevron"), homePlate: t("shape.homePlate"), notchedRightArrow: t("shape.notchedRightArrow"),
  stripedRightArrow: t("shape.stripedRightArrow"), circularArrow: t("shape.circularArrow"), mathPlus: t("shape.mathPlus"), mathMinus: t("shape.mathMinus"), mathMultiply: t("shape.mathMultiply"), mathDivide: t("shape.mathDivide"),
  mathEqual: t("shape.mathEqual"), mathNotEqual: t("shape.mathNotEqual"), flowChartProcess: t("shape.flowChartProcess"), flowChartDecision: t("shape.flowChartDecision"), flowChartTerminator: t("shape.flowChartTerminator"),
  flowChartDocument: t("shape.flowChartDocument"), flowChartInputOutput: t("shape.flowChartInputOutput"), flowChartConnector: t("shape.flowChartConnector"), star4: t("shape.star4"), star5: t("shape.star5"),
  star6: t("shape.star6"), star8: t("shape.star8"), star10: t("shape.star10"), star12: t("shape.star12"), irregularSeal1: t("shape.irregularSeal1"), irregularSeal2: t("shape.irregularSeal2"),
  wave: t("shape.wave"), doubleWave: t("shape.doubleWave"), verticalScroll: t("shape.verticalScroll"), horizontalScroll: t("shape.horizontalScroll"), wedgeRectCallout: t("shape.wedgeRectCallout"),
  wedgeRoundRectCallout: t("shape.wedgeRoundRectCallout"), wedgeEllipseCallout: t("shape.wedgeEllipseCallout"), cloudCallout: t("shape.cloudCallout"),
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
          {SHAPE_CATEGORIES.filter(([title]) => lines || title !== t("shape.catLines")).map(([title, names]) => (
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
