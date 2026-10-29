//! Table parsing: `a:tbl` and the table styles in `ppt/tableStyles.xml`.

use std::collections::HashMap;
use std::sync::Arc;

use ppt_model::table::{Border, CellFill, StylePart, TableCell, TableFlags, TableRow, TableStyle};
use ppt_model::{Anchor, Line, Table};
use roxmltree::Node;

use crate::style::{ColorCtx, LevelStyle, attr_bool, attr_i64, child, children, path};
use crate::package::Rel;
use crate::{FillSpecColor, Master, fill_spec, text_body};

/// PowerPoint's default table style, "Medium Style 2 - Accent 1".
pub const MEDIUM_STYLE_2: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";

/// Definition of [`MEDIUM_STYLE_2`], for decks that use it without defining it
/// (PowerPoint knows its built-in styles by id) and for the built-in template.
pub const MEDIUM_STYLE_2_XML: &str = r#"<a:tblStyle styleId="{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}" styleName="Medium Style 2 - Accent 1"><a:wholeTbl><a:tcTxStyle><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="dk1"/></a:tcTxStyle><a:tcStyle><a:tcBdr><a:left><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:left><a:right><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:right><a:top><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:top><a:bottom><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:bottom><a:insideH><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:insideH><a:insideV><a:ln w="12700" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:insideV></a:tcBdr><a:fill><a:solidFill><a:schemeClr val="accent1"><a:tint val="20000"/></a:schemeClr></a:solidFill></a:fill></a:tcStyle></a:wholeTbl><a:band1H><a:tcStyle><a:tcBdr/><a:fill><a:solidFill><a:schemeClr val="accent1"><a:tint val="40000"/></a:schemeClr></a:solidFill></a:fill></a:tcStyle></a:band1H><a:band2H><a:tcStyle><a:tcBdr/></a:tcStyle></a:band2H><a:band1V><a:tcStyle><a:tcBdr/><a:fill><a:solidFill><a:schemeClr val="accent1"><a:tint val="40000"/></a:schemeClr></a:solidFill></a:fill></a:tcStyle></a:band1V><a:band2V><a:tcStyle><a:tcBdr/></a:tcStyle></a:band2V><a:lastCol><a:tcTxStyle b="on"><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="lt1"/></a:tcTxStyle><a:tcStyle><a:tcBdr/><a:fill><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:fill></a:tcStyle></a:lastCol><a:firstCol><a:tcTxStyle b="on"><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="lt1"/></a:tcTxStyle><a:tcStyle><a:tcBdr/><a:fill><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:fill></a:tcStyle></a:firstCol><a:lastRow><a:tcTxStyle b="on"><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="lt1"/></a:tcTxStyle><a:tcStyle><a:tcBdr><a:top><a:ln w="38100" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:top></a:tcBdr><a:fill><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:fill></a:tcStyle></a:lastRow><a:firstRow><a:tcTxStyle b="on"><a:fontRef idx="minor"><a:prstClr val="black"/></a:fontRef><a:schemeClr val="lt1"/></a:tcTxStyle><a:tcStyle><a:tcBdr><a:bottom><a:ln w="38100" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"/></a:solidFill></a:ln></a:bottom></a:tcBdr><a:fill><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></a:fill></a:tcStyle></a:firstRow></a:tblStyle>"#;

/// Default cell margins (`a:tcPr`): 0.1" left/right, 0.05" top/bottom.
const MARGIN_LR: i64 = 91_440;
const MARGIN_TB: i64 = 45_720;
/// Width of a border line without `w`.
const DEFAULT_BORDER: i64 = 12_700;

/// A border element (`a:lnL`, `a:left/a:ln`, ...): `Some(None)` when explicitly empty.
fn border(ln: Node, ctx: &ColorCtx) -> Option<Border> {
    match fill_spec(ln, ctx).color() {
        FillSpecColor::Solid(color) => Some(Some(Line::new(color, attr_i64(ln, "w").unwrap_or(DEFAULT_BORDER)))),
        FillSpecColor::None => Some(None),
        FillSpecColor::Unset => None,
    }
}

fn cell_fill(n: Node, ctx: &ColorCtx) -> Option<CellFill> {
    match fill_spec(n, ctx).color() {
        FillSpecColor::Solid(c) => Some(CellFill::Solid(c)),
        FillSpecColor::None => Some(CellFill::None),
        FillSpecColor::Unset => None,
    }
}

fn style_part(n: Option<Node>, ctx: &ColorCtx) -> StylePart {
    let Some(n) = n else { return StylePart::default() };
    let mut part = StylePart::default();
    if let Some(tx) = child(n, "tcTxStyle") {
        let on = |name| tx.attribute(name).map(|v| v == "on");
        part.bold = on("b");
        part.italic = on("i");
        // A direct color child wins over the one inside `fontRef`.
        part.color = ctx.color_in(tx, None).or_else(|| child(tx, "fontRef").and_then(|f| ctx.color_in(f, None)));
    }
    if let Some(tc) = child(n, "tcStyle") {
        part.fill = child(tc, "fill").and_then(|f| cell_fill(f, ctx));
        if let Some(bdr) = child(tc, "tcBdr") {
            let edge = |name: &str| child(bdr, name).and_then(|e| child(e, "ln")).and_then(|ln| border(ln, ctx));
            part.left = edge("left");
            part.right = edge("right");
            part.top = edge("top");
            part.bottom = edge("bottom");
            part.inside_h = edge("insideH");
            part.inside_v = edge("insideV");
        }
    }
    part
}

/// Finds and resolves style `id` in a `tableStyles.xml` document, falling back
/// to the built-in definition of [`MEDIUM_STYLE_2`].
pub(crate) fn table_style(xml: Option<&str>, id: &str, ctx: &ColorCtx) -> Option<TableStyle> {
    let doc = xml.and_then(|x| roxmltree::Document::parse(x).ok());
    let found = doc.as_ref().and_then(|d| children(d.root_element(), "tblStyle").find(|s| s.attribute("styleId") == Some(id)));
    if let Some(n) = found {
        return Some(parse_style(n, id, ctx));
    }
    if id != MEDIUM_STYLE_2 {
        return None;
    }
    let wrapped = format!(r#"<a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{MEDIUM_STYLE_2_XML}</a:tblStyleLst>"#);
    let doc = roxmltree::Document::parse(&wrapped).ok()?;
    let n = doc.root_element().first_element_child()?;
    Some(parse_style(n, id, ctx))
}

/// Default table style id of a `tableStyles.xml` (`a:tblStyleLst@def`).
pub(crate) fn default_style_id(xml: Option<&str>) -> String {
    xml.and_then(|x| roxmltree::Document::parse(x).ok())
        .and_then(|d| d.root_element().attribute("def").map(str::to_string))
        .unwrap_or_else(|| MEDIUM_STYLE_2.to_string())
}

fn parse_style(n: Node, id: &str, ctx: &ColorCtx) -> TableStyle {
    let part = |name| style_part(child(n, name), ctx);
    TableStyle {
        id: id.to_string(),
        whole: part("wholeTbl"),
        band1_h: part("band1H"),
        band2_h: part("band2H"),
        band1_v: part("band1V"),
        band2_v: part("band2V"),
        first_row: part("firstRow"),
        last_row: part("lastRow"),
        first_col: part("firstCol"),
        last_col: part("lastCol"),
    }
}

pub(crate) fn table_flags(tbl: Node) -> TableFlags {
    let flag = |name| path(tbl, &["tblPr"]).and_then(|p| attr_bool(p, name)).unwrap_or(false);
    TableFlags {
        first_row: flag("firstRow"),
        last_row: flag("lastRow"),
        first_col: flag("firstCol"),
        last_col: flag("lastCol"),
        band_row: flag("bandRow"),
        band_col: flag("bandCol"),
    }
}

pub(crate) fn style_id(tbl: Node) -> Option<String> {
    path(tbl, &["tblPr", "tableStyleId"]).and_then(|n| n.text()).map(|s| s.trim().to_string())
}

/// Parses `a:tbl`. Cell text inherits the style's text formatting for its position.
pub(crate) fn table(tbl: Node, master: &Master, style: Option<Arc<TableStyle>>, rels: &HashMap<String, Rel>) -> Table {
    let ctx = &master.ctx;
    let cols: Vec<i64> = path(tbl, &["tblGrid"]).map(|g| children(g, "gridCol").map(|c| attr_i64(c, "w").unwrap_or(0)).collect()).unwrap_or_default();
    let mut table = Table { cols, rows: Vec::new(), flags: table_flags(tbl), style };
    // Shape the rows first so style parts (which depend on the row count) resolve correctly.
    let row_nodes: Vec<Node> = children(tbl, "tr").collect();
    table.rows = row_nodes.iter().map(|tr| TableRow { height: attr_i64(*tr, "h").unwrap_or(0), cells: Vec::new() }).collect();

    for (r, tr) in row_nodes.iter().enumerate() {
        let mut cells = Vec::new();
        for (c, tc) in children(*tr, "tc").enumerate() {
            let pr = child(tc, "tcPr");
            // Text formatting from the table style for this position, lowest priority first.
            let mut text_style = LevelStyle { latin: Some("+mn-lt".into()), ea: Some("+mn-ea".into()), ..Default::default() };
            for part in table.parts_for(r, c) {
                text_style.bold = part.bold.or(text_style.bold);
                text_style.italic = part.italic.or(text_style.italic);
                text_style.color = part.color.or(text_style.color);
            }
            let mut text = child(tc, "txBody").map(|tx| text_body(tx, master, None, &[], Some(text_style), rels)).unwrap_or_default();
            let margin = |name, default| pr.and_then(|p| attr_i64(p, name)).unwrap_or(default);
            text.insets = [margin("marL", MARGIN_LR), margin("marT", MARGIN_TB), margin("marR", MARGIN_LR), margin("marB", MARGIN_TB)];
            text.anchor = match pr.and_then(|p| p.attribute("anchor")) {
                Some("ctr") => Anchor::Middle,
                Some("b") => Anchor::Bottom,
                _ => Anchor::Top,
            };
            text.wrap = true;
            text.auto_grow = false;
            let edge = |name| pr.and_then(|p| child(p, name)).and_then(|ln| border(ln, ctx));
            cells.push(TableCell {
                text,
                grid_span: attr_i64(tc, "gridSpan").unwrap_or(1).max(1) as u32,
                row_span: attr_i64(tc, "rowSpan").unwrap_or(1).max(1) as u32,
                h_merge: attr_bool(tc, "hMerge").unwrap_or(false),
                v_merge: attr_bool(tc, "vMerge").unwrap_or(false),
                fill: pr.and_then(|p| cell_fill(p, ctx)).unwrap_or(CellFill::Inherit),
                borders: [edge("lnL"), edge("lnT"), edge("lnR"), edge("lnB")],
            });
        }
        // Keep rows rectangular even when a file omits trailing cells.
        cells.resize_with(table.cols.len().max(cells.len()), TableCell::default);
        table.rows[r].cells = cells;
    }
    table
}
