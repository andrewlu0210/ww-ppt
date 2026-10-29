//! Chart parsing (`c:chartSpace`): series values and categories come from the
//! caches PowerPoint stores in the chart part, formatting from its `c:spPr` / `c:txPr`.

use ppt_model::chart::{
    Axis, AxisPos, Chart, ChartTitle, DataLabels, Grouping, LabelPosition, Legend, LegendPos, ManualLayout, Marker, MarkerSymbol, Plot,
    PlotKind, Series,
};
use ppt_model::{Color, Line, Paint, RunStyle};
use roxmltree::Node;

use crate::style::{ColorCtx, LevelStyle, Theme, attr_i64, child, children, path};
use crate::{FillSpec, fill_spec, parse_line};

fn val<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.attribute("val"))
}

fn val_f64(n: Node, name: &str) -> Option<f64> {
    val(n, name).and_then(|v| v.parse().ok())
}

fn flag(n: Node, name: &str) -> Option<bool> {
    // Chart booleans default to true when the element is present without `val`.
    child(n, name).map(|c| !matches!(c.attribute("val"), Some("0" | "false")))
}

/// Text style from a `c:txPr` (its first paragraph's `a:defRPr`) over `base`.
fn text_style(tx_pr: Option<Node>, base: &RunStyle, ctx: &ColorCtx) -> RunStyle {
    let Some(rpr) = tx_pr.and_then(|t| path(t, &["p", "pPr", "defRPr"])) else { return base.clone() };
    let s = LevelStyle::parse_rpr(rpr, ctx);
    let theme: &Theme = &ctx.theme;
    RunStyle {
        size_pt: s.size.unwrap_or(base.size_pt),
        bold: s.bold.unwrap_or(base.bold),
        italic: s.italic.unwrap_or(base.italic),
        color: s.color.unwrap_or(base.color),
        latin_font: s.latin.as_deref().map(|t| theme.resolve_typeface(t)).unwrap_or_else(|| base.latin_font.clone()),
        ea_font: s.ea.as_deref().map(|t| theme.resolve_typeface(t)).unwrap_or_else(|| base.ea_font.clone()),
        ..base.clone()
    }
}

fn paint(sp_pr: Option<Node>, ctx: &ColorCtx) -> Option<Paint> {
    match sp_pr.map(|p| fill_spec(p, ctx))? {
        FillSpec::Paint(p) => Some(p),
        _ => None,
    }
}

fn manual_layout(n: Option<Node>) -> Option<ManualLayout> {
    let m = n.and_then(|l| child(l, "manualLayout"))?;
    Some(ManualLayout { x: val_f64(m, "x")?, y: val_f64(m, "y")?, w: val_f64(m, "w"), h: val_f64(m, "h") })
}

/// Excel date serial (days since 1899-12-30) as `y/m/d`.
fn excel_date(serial: f64) -> String {
    // Civil-from-days (Howard Hinnant), with day 0 = 1899-12-30 = 1970-01-01 minus 25569 days.
    let z = serial.floor() as i64 - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y}/{m}/{d}")
}

fn is_date_format(f: &str) -> bool {
    let f = f.to_lowercase();
    !f.contains('0') && !f.contains('#') && (f.contains('y') || f.contains('d') || (f.contains('m') && !f.contains("general")))
}

/// Points of a `c:strCache` / `c:numCache` (or literal) as strings / numbers.
fn cache_points(cache: Node) -> (usize, Vec<(usize, String)>, String) {
    let count = val(cache, "ptCount").and_then(|v| v.parse().ok()).unwrap_or(0);
    let format = child(cache, "formatCode").and_then(|f| f.text()).unwrap_or("General").to_string();
    let points = children(cache, "pt")
        .filter_map(|p| Some((attr_i64(p, "idx")? as usize, child(p, "v").and_then(|v| v.text()).unwrap_or_default().to_string())))
        .collect();
    (count, points, format)
}

fn find_cache<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    n.descendants().find(|d| matches!(d.tag_name().name(), "strCache" | "numCache" | "strLit" | "numLit" | "multiLvlStrCache"))
}

fn categories(cat: Option<Node>) -> Vec<String> {
    let Some(cache) = cat.and_then(find_cache) else { return Vec::new() };
    // Multi-level categories: show the innermost level.
    let cache = if cache.tag_name().name() == "multiLvlStrCache" { cache.descendants().find(|d| d.tag_name().name() == "lvl").unwrap_or(cache) } else { cache };
    let (count, points, format) = cache_points(cache);
    let mut out = vec![String::new(); count.max(points.iter().map(|p| p.0 + 1).max().unwrap_or(0))];
    for (i, v) in points {
        out[i] = match v.parse::<f64>() {
            Ok(n) if is_date_format(&format) => excel_date(n),
            Ok(n) if cache.tag_name().name() == "numCache" => ppt_model::chart::format_number(n, &format),
            _ => v,
        };
    }
    out
}

fn values(v: Option<Node>) -> (Vec<Option<f64>>, String) {
    let Some(cache) = v.and_then(find_cache) else { return (Vec::new(), "General".into()) };
    let (count, points, format) = cache_points(cache);
    let mut out = vec![None; count.max(points.iter().map(|p| p.0 + 1).max().unwrap_or(0))];
    for (i, s) in points {
        out[i] = s.parse().ok();
    }
    (out, format)
}

fn labels(n: Option<Node>, base: &RunStyle, ctx: &ColorCtx) -> Option<DataLabels> {
    let n = n?;
    let mut l = DataLabels {
        show_value: flag(n, "showVal").unwrap_or(false),
        show_percent: flag(n, "showPercent").unwrap_or(false),
        show_category: flag(n, "showCatName").unwrap_or(false),
        show_series: flag(n, "showSerName").unwrap_or(false),
        position: val(n, "dLblPos").map(|p| match p {
            "outEnd" => LabelPosition::OutsideEnd,
            "inEnd" => LabelPosition::InsideEnd,
            "ctr" => LabelPosition::Center,
            "inBase" => LabelPosition::InsideBase,
            "t" => LabelPosition::Above,
            "b" => LabelPosition::Below,
            "l" => LabelPosition::Left,
            "r" => LabelPosition::Right,
            _ => LabelPosition::BestFit,
        }),
        format: child(n, "numFmt").and_then(|f| f.attribute("formatCode")).map(str::to_string),
        style: text_style(child(n, "txPr"), base, ctx),
        deleted: Vec::new(),
        offsets: Vec::new(),
        texts: Vec::new(),
    };
    for d in children(n, "dLbl") {
        let Some(idx) = val(d, "idx").and_then(|v| v.parse().ok()) else { continue };
        if flag(d, "delete") == Some(true) {
            l.deleted.push(idx);
            continue;
        }
        if let Some(m) = manual_layout(child(d, "layout")) {
            l.offsets.push((idx, m.x, m.y));
        }
        if let Some(rich) = path(d, &["tx", "rich"]) {
            let text = children(rich, "p").map(|p| p.descendants().filter(|t| t.tag_name().name() == "t").filter_map(|t| t.text()).collect::<String>()).collect::<Vec<_>>().join("\n");
            l.texts.push((idx, text));
        }
    }
    Some(l)
}

fn grouping(n: Node) -> Grouping {
    match val(n, "grouping") {
        Some("stacked") => Grouping::Stacked,
        Some("percentStacked") => Grouping::PercentStacked,
        _ => Grouping::Clustered,
    }
}

fn series(n: Node, ctx: &ColorCtx, base: &RunStyle) -> Series {
    let sp_pr = child(n, "spPr");
    let name = child(n, "tx")
        .and_then(|t| t.descendants().find(|d| d.tag_name().name() == "v").and_then(|v| v.text()))
        .unwrap_or_default()
        .to_string();
    let (values, format) = values(child(n, "val").or_else(|| child(n, "yVal")));
    let marker = child(n, "marker").map(|m| {
        let msp = child(m, "spPr");
        Marker {
            symbol: match val(m, "symbol") {
                Some("none") => MarkerSymbol::None,
                Some("square") => MarkerSymbol::Square,
                Some("diamond") => MarkerSymbol::Diamond,
                Some("triangle") => MarkerSymbol::Triangle,
                _ => MarkerSymbol::Circle,
            },
            size: val_f64(m, "size").unwrap_or(5.0),
            fill: paint(msp, ctx).map(|p| p.color()),
            line: msp.and_then(|p| parse_line(child(p, "ln"), None, ctx)).map(|l| l.color),
        }
    });
    Series {
        index: val(n, "idx").and_then(|v| v.parse().ok()).unwrap_or(0),
        name,
        categories: categories(child(n, "cat").or_else(|| child(n, "xVal"))),
        values,
        format,
        fill: paint(sp_pr, ctx),
        line: sp_pr.and_then(|p| parse_line(child(p, "ln"), None, ctx)),
        no_line: sp_pr.and_then(|p| child(p, "ln")).is_some_and(|ln| child(ln, "noFill").is_some()),
        marker,
        point_fills: children(n, "dPt")
            .filter_map(|d| Some((val(d, "idx")?.parse().ok()?, paint(child(d, "spPr"), ctx)?)))
            .collect(),
        labels: labels(child(n, "dLbls"), base, ctx),
    }
}

fn axis(n: Node, ctx: &ColorCtx, base: &RunStyle) -> Option<Axis> {
    let scaling = child(n, "scaling");
    let sp_pr = child(n, "spPr");
    Some(Axis {
        id: val(n, "axId")?.parse().ok()?,
        category: n.tag_name().name() != "valAx",
        position: match val(n, "axPos") {
            Some("l") => AxisPos::Left,
            Some("r") => AxisPos::Right,
            Some("t") => AxisPos::Top,
            _ => AxisPos::Bottom,
        },
        deleted: flag(n, "delete").unwrap_or(false),
        min: scaling.and_then(|s| val_f64(s, "min")),
        max: scaling.and_then(|s| val_f64(s, "max")),
        major_unit: val_f64(n, "majorUnit"),
        reversed: scaling.and_then(|s| val(s, "orientation")) == Some("maxMin"),
        format: child(n, "numFmt").filter(|f| f.attribute("sourceLinked") != Some("1")).and_then(|f| f.attribute("formatCode")).map(str::to_string),
        gridlines: child(n, "majorGridlines").map(|g| {
            parse_line(child(g, "spPr").and_then(|p| child(p, "ln")), None, ctx).unwrap_or(Line::new(Color::rgb(0xD9, 0xD9, 0xD9), 9_525))
        }),
        line: sp_pr.and_then(|p| parse_line(child(p, "ln"), None, ctx)),
        labels: val(n, "tickLblPos") != Some("none"),
        style: text_style(child(n, "txPr"), base, ctx),
        cross_axis: val(n, "crossAx").and_then(|v| v.parse().ok()).unwrap_or(0),
    })
}

/// Office's automatic chart colors: the six accents, then darker and lighter variants.
fn palette(ctx: &ColorCtx) -> Vec<Color> {
    let accents: Vec<Color> = (1..=6).filter_map(|i| ctx.scheme(&format!("accent{i}"))).collect();
    let shade = |c: Color, f: f64| {
        let ch = |v: u8| if f < 0.0 { (v as f64 * (1.0 + f)).round() as u8 } else { (v as f64 + (255.0 - v as f64) * f).round() as u8 };
        Color { r: ch(c.r), g: ch(c.g), b: ch(c.b), a: c.a }
    };
    let mut out = accents.clone();
    out.extend(accents.iter().map(|c| shade(*c, -0.4)));
    out.extend(accents.iter().map(|c| shade(*c, 0.4)));
    out
}

/// Parses a chart part. Returns `None` when it has no plot we can draw.
pub(crate) fn chart(xml: &str, ctx: &ColorCtx, minor: &(String, String)) -> Option<Chart> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let space = doc.root_element();
    let chart = child(space, "chart")?;
    let plot_area = child(chart, "plotArea")?;
    // Office's default chart text: 10pt dark gray in the theme's body font.
    let default = RunStyle { size_pt: 10.0, color: Color::rgb(0x59, 0x59, 0x59), latin_font: minor.0.clone(), ea_font: minor.1.clone(), ..Default::default() };
    let base = text_style(child(space, "txPr"), &default, ctx);

    let mut plots = Vec::new();
    for p in plot_area.children().filter(|c| c.is_element()) {
        let name = p.tag_name().name();
        let kind = match name {
            "barChart" | "bar3DChart" => PlotKind::Bar {
                horizontal: val(p, "barDir") == Some("bar"),
                grouping: grouping(p),
                gap: val_f64(p, "gapWidth").unwrap_or(150.0),
                overlap: val_f64(p, "overlap").unwrap_or(if grouping(p) == Grouping::Clustered { 0.0 } else { 100.0 }),
            },
            "lineChart" | "line3DChart" => PlotKind::Line { grouping: grouping(p) },
            "areaChart" | "area3DChart" => PlotKind::Area { grouping: grouping(p) },
            "pieChart" | "pie3DChart" | "doughnutChart" | "ofPieChart" => PlotKind::Pie {
                first_angle: val_f64(p, "firstSliceAng").unwrap_or(0.0),
                hole: if name == "doughnutChart" { val_f64(p, "holeSize").unwrap_or(50.0) / 100.0 } else { 0.0 },
                three_d: name == "pie3DChart",
            },
            _ => continue,
        };
        let ax: Vec<u32> = children(p, "axId").filter_map(|a| a.attribute("val")?.parse().ok()).collect();
        let series = children(p, "ser").map(|s| series(s, ctx, &base)).collect();
        plots.push(Plot {
            kind,
            series,
            axes: (ax.len() >= 2).then(|| (ax[0], ax[1])),
            labels: labels(child(p, "dLbls"), &base, ctx),
            vary_colors: flag(p, "varyColors").unwrap_or(matches!(kind, PlotKind::Pie { .. })),
        });
    }
    if plots.is_empty() {
        return None;
    }
    let axes = plot_area.children().filter(|c| matches!(c.tag_name().name(), "catAx" | "valAx" | "dateAx" | "serAx")).filter_map(|a| axis(a, ctx, &base)).collect();

    let title = child(chart, "title").map(|t| {
        let rich = path(t, &["tx", "rich"]);
        let text = rich
            .map(|r| children(r, "p").map(|p| p.descendants().filter(|d| d.tag_name().name() == "t").filter_map(|d| d.text()).collect::<String>()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|| plots.iter().flat_map(|p| &p.series).next().map(|s| s.name.clone()).unwrap_or_default());
        let title_base = RunStyle { size_pt: 14.0, ..base.clone() };
        let mut style = text_style(child(t, "txPr").or(rich), &title_base, ctx);
        if let Some(rpr) = rich.and_then(|r| r.descendants().find(|d| d.tag_name().name() == "rPr")) {
            let s = LevelStyle::parse_rpr(rpr, ctx);
            style.size_pt = s.size.unwrap_or(style.size_pt);
            style.bold = s.bold.unwrap_or(style.bold);
            style.color = s.color.unwrap_or(style.color);
        }
        ChartTitle { text, style, layout: manual_layout(child(t, "layout")), overlay: flag(t, "overlay").unwrap_or(false) }
    });
    let legend = child(chart, "legend").map(|l| Legend {
        position: match val(l, "legendPos") {
            Some("l") => LegendPos::Left,
            Some("t") => LegendPos::Top,
            Some("b") => LegendPos::Bottom,
            Some("tr") => LegendPos::TopRight,
            _ => LegendPos::Right,
        },
        layout: manual_layout(child(l, "layout")),
        overlay: flag(l, "overlay").unwrap_or(false),
        style: text_style(child(l, "txPr"), &base, ctx),
        deleted: children(l, "legendEntry").filter(|e| flag(*e, "delete") == Some(true)).filter_map(|e| val(e, "idx")?.parse().ok()).collect(),
    });
    let layout_node = child(plot_area, "layout");
    let space_sp = child(space, "spPr");
    Some(Chart {
        title,
        plots,
        axes,
        legend,
        plot_layout: manual_layout(layout_node),
        plot_layout_inner: layout_node.and_then(|l| child(l, "manualLayout")).and_then(|m| val(m, "layoutTarget")) == Some("inner"),
        fill: paint(space_sp, ctx),
        border: space_sp.and_then(|p| parse_line(child(p, "ln"), None, ctx)),
        plot_fill: paint(child(plot_area, "spPr"), ctx),
        text: base,
        palette: palette(ctx),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_combo_chart() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:autoTitleDeleted val="1"/><c:plotArea>
            <c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>
              <c:ser><c:idx val="0"/><c:tx><c:v>銷售</c:v></c:tx><c:spPr><a:solidFill><a:srgbClr val="27499B"/></a:solidFill></c:spPr>
                <c:dLbls><c:dLbl><c:idx val="1"/><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>自訂</a:t></a:r></a:p></c:rich></c:tx><c:showVal val="1"/></c:dLbl><c:showVal val="1"/></c:dLbls>
                <c:cat><c:numRef><c:numCache><c:formatCode>yyyy/m/d</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>45000</c:v></c:pt><c:pt idx="1"><c:v>45001</c:v></c:pt></c:numCache></c:numRef></c:cat>
                <c:val><c:numRef><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>
              <c:gapWidth val="80"/><c:axId val="1"/><c:axId val="2"/></c:barChart>
            <c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="1"/><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="1"><c:v>0.5</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="1"/><c:axId val="3"/></c:lineChart>
            <c:catAx><c:axId val="1"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx>
            <c:valAx><c:axId val="2"/><c:axPos val="l"/><c:majorGridlines/><c:crossAx val="1"/></c:valAx>
            <c:valAx><c:axId val="3"/><c:scaling><c:max val="1"/></c:scaling><c:axPos val="r"/><c:numFmt formatCode="0%" sourceLinked="0"/><c:crossAx val="1"/></c:valAx>
          </c:plotArea><c:legend><c:legendPos val="b"/></c:legend></c:chart></c:chartSpace>"#;
        let ctx = ColorCtx { theme: Theme::default(), clr_map: Default::default() };
        let c = chart(xml, &ctx, &("Calibri".into(), String::new())).unwrap();
        assert!(c.title.is_none());
        assert_eq!(c.plots.len(), 2);
        assert!(matches!(c.plots[0].kind, PlotKind::Bar { horizontal: false, gap, .. } if gap == 80.0));
        let s = &c.plots[0].series[0];
        assert_eq!((s.name.as_str(), s.categories.clone(), s.values.clone()), ("銷售", vec!["2023/3/15".to_string(), "2023/3/16".to_string()], vec![Some(10.0), Some(20.0)]));
        assert_eq!(s.fill, Some(Paint::Solid(Color::rgb(0x27, 0x49, 0x9B))));
        assert_eq!(s.labels.as_ref().unwrap().texts, vec![(1, "自訂".to_string())]);
        assert_eq!(c.plots[1].series[0].values, vec![None, Some(0.5)]);
        assert_eq!(c.plots[1].axes, Some((1, 3)));
        let right = c.axis(3).unwrap();
        assert_eq!((right.position, right.max, right.format.as_deref()), (AxisPos::Right, Some(1.0), Some("0%")));
        assert!(c.axis(2).unwrap().gridlines.is_some());
        assert_eq!(c.legend.as_ref().unwrap().position, LegendPos::Bottom);
    }

    #[test]
    fn excel_dates() {
        assert_eq!(excel_date(45_000.0), "2023/3/15");
        assert_eq!(excel_date(1.0), "1899/12/31");
        assert!(is_date_format("yyyy/m/d") && !is_date_format("0.0%") && !is_date_format("General"));
    }
}
