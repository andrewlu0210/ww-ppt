//! Table export. An edited table keeps its XML and only changed cells,
//! widths and heights are rewritten, unless rows or columns were added or
//! removed, in which case the whole `a:tbl` is regenerated.

use ppt_model::table::{Border, CellFill, TableCell, TableFlags};
use ppt_model::{Anchor, Table};
use roxmltree::Node;

use crate::writer::{apply_edits, child, color_xml, esc, open_tag, paragraphs_xml};

pub(crate) const TABLE_URI: &str = "http://schemas.openxmlformats.org/drawingml/2006/table";
const MARGINS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];

fn flags_attrs(f: &TableFlags) -> String {
    [
        ("firstRow", f.first_row),
        ("lastRow", f.last_row),
        ("firstCol", f.first_col),
        ("lastCol", f.last_col),
        ("bandRow", f.band_row),
        ("bandCol", f.band_col),
    ]
    .iter()
    .filter(|(_, on)| *on)
    .map(|(name, _)| format!(r#" {name}="1""#))
    .collect()
}

fn border_xml(tag: &str, b: Option<Border>) -> String {
    match b {
        None => String::new(),
        Some(None) => format!("<a:{tag}><a:noFill/></a:{tag}>"),
        Some(Some(l)) => format!(r#"<a:{tag} w="{}"><a:solidFill>{}</a:solidFill></a:{tag}>"#, l.width, color_xml(l.color)),
    }
}

fn tc_pr_xml(cell: &TableCell) -> String {
    let mut attrs = String::new();
    for (name, (v, d)) in ["marL", "marT", "marR", "marB"].iter().zip(cell.text.insets.iter().zip(MARGINS)) {
        if *v != d {
            attrs.push_str(&format!(r#" {name}="{v}""#));
        }
    }
    match cell.text.anchor {
        Anchor::Top => {}
        Anchor::Middle => attrs.push_str(r#" anchor="ctr""#),
        Anchor::Bottom => attrs.push_str(r#" anchor="b""#),
    }
    let [l, t, r, b] = cell.borders;
    // Schema order: lnL, lnR, lnT, lnB, ..., fill.
    let mut kids = border_xml("lnL", l) + &border_xml("lnR", r) + &border_xml("lnT", t) + &border_xml("lnB", b);
    match cell.fill {
        CellFill::Inherit => {}
        CellFill::None => kids.push_str("<a:noFill/>"),
        CellFill::Solid(c) => kids.push_str(&format!("<a:solidFill>{}</a:solidFill>", color_xml(c))),
    }
    if kids.is_empty() { format!("<a:tcPr{attrs}/>") } else { format!("<a:tcPr{attrs}>{kids}</a:tcPr>") }
}

fn tc_xml(cell: &TableCell) -> String {
    let mut attrs = String::new();
    if cell.grid_span > 1 {
        attrs.push_str(&format!(r#" gridSpan="{}""#, cell.grid_span));
    }
    if cell.row_span > 1 {
        attrs.push_str(&format!(r#" rowSpan="{}""#, cell.row_span));
    }
    if cell.h_merge {
        attrs.push_str(r#" hMerge="1""#);
    }
    if cell.v_merge {
        attrs.push_str(r#" vMerge="1""#);
    }
    format!("<a:tc{attrs}><a:txBody><a:bodyPr/><a:lstStyle/>{}</a:txBody>{}</a:tc>", paragraphs_xml(&cell.text), tc_pr_xml(cell))
}

/// A complete `a:tbl`. `old_tbl_pr` keeps the original properties' children.
pub(crate) fn table_xml(t: &Table, old_tbl_pr: Option<&str>) -> String {
    let style = t.style.as_ref().map(|s| format!("<a:tableStyleId>{}</a:tableStyleId>", esc(&s.id))).unwrap_or_default();
    let tbl_pr = format!("<a:tblPr{}>{}</a:tblPr>", flags_attrs(&t.flags), old_tbl_pr.unwrap_or(&style));
    let grid: String = t.cols.iter().map(|w| format!(r#"<a:gridCol w="{w}"/>"#)).collect();
    let rows: String = t.rows.iter().map(|r| format!(r#"<a:tr h="{}">{}</a:tr>"#, r.height, r.cells.iter().map(tc_xml).collect::<String>())).collect();
    format!("<a:tbl>{tbl_pr}<a:tblGrid>{grid}</a:tblGrid>{rows}</a:tbl>")
}

fn same_structure(a: &Table, b: &Table) -> bool {
    a.cols.len() == b.cols.len()
        && a.rows.len() == b.rows.len()
        && a.rows.iter().zip(&b.rows).all(|(ra, rb)| {
            ra.cells.len() == rb.cells.len()
                && ra.cells.iter().zip(&rb.cells).all(|(x, y)| (x.grid_span, x.row_span, x.h_merge, x.v_merge) == (y.grid_span, y.row_span, y.h_merge, y.v_merge))
        })
}

fn elements<'a, 'i>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

/// Edits turning the original `tbl` (parsed as `old`) into `new`, in `src` coordinates.
fn table_edits(src: &str, tbl: Node, old: &Table, new: &Table) -> Vec<(std::ops::Range<usize>, String)> {
    let tbl_pr = child(tbl, "tblPr");
    let tbl_pr_kids = tbl_pr.map(|p| p.children().filter(|c| c.is_element()).map(|c| &src[c.range()]).collect::<String>());
    if !same_structure(old, new) {
        let kids = tbl_pr_kids.filter(|_| old.style.as_ref().map(|s| &s.id) == new.style.as_ref().map(|s| &s.id));
        return vec![(tbl.range(), table_xml(new, kids.as_deref()))];
    }
    let mut edits = Vec::new();
    if old.flags != new.flags
        && let Some(p) = tbl_pr
    {
        edits.push((p.range(), format!("<a:tblPr{}>{}</a:tblPr>", flags_attrs(&new.flags), tbl_pr_kids.unwrap_or_default())));
    }
    if old.cols != new.cols
        && let Some(grid) = child(tbl, "tblGrid")
    {
        for ((col, ow), nw) in elements(grid, "gridCol").zip(&old.cols).zip(&new.cols) {
            if ow != nw
                && let Some(a) = col.attribute_node("w")
            {
                edits.push((a.range_value(), nw.to_string()));
            }
        }
    }
    for ((tr, orow), nrow) in elements(tbl, "tr").zip(&old.rows).zip(&new.rows) {
        if orow.height != nrow.height
            && let Some(a) = tr.attribute_node("h")
        {
            edits.push((a.range_value(), nrow.height.to_string()));
        }
        for ((tc, oc), nc) in elements(tr, "tc").zip(&orow.cells).zip(&nrow.cells) {
            if oc.text != nc.text {
                match child(tc, "txBody") {
                    Some(tx) => {
                        let kept: String = tx.children().filter(|c| c.is_element() && c.tag_name().name() != "p").map(|c| &src[c.range()]).collect();
                        edits.push((tx.range(), format!("{}>{kept}{}</a:txBody>", open_tag(src, tx), paragraphs_xml(&nc.text))));
                    }
                    None => {
                        let at = tc.range().start + src[tc.range()].find('>').map_or(0, |i| i + 1);
                        edits.push((at..at, format!("<a:txBody><a:bodyPr/><a:lstStyle/>{}</a:txBody>", paragraphs_xml(&nc.text))));
                    }
                }
            }
            let props = |c: &TableCell| (c.fill, c.borders, c.text.insets, c.text.anchor);
            if props(oc) != props(nc) {
                match child(tc, "tcPr") {
                    Some(pr) => edits.push((pr.range(), tc_pr_xml(nc))),
                    None => {
                        let at = child(tc, "extLst").map_or_else(|| tc.range().end - "</a:tc>".len(), |e| e.range().start);
                        edits.push((at..at, tc_pr_xml(nc)));
                    }
                }
            }
        }
    }
    edits
}

/// Applies `table_edits` to a whole graphic frame's XML (used for copies too).
pub(crate) fn patch_table(src: &str, tbl: Node, old: &Table, new: &Table) -> String {
    let r = tbl.range();
    let edits = table_edits(src, tbl, old, new).into_iter().map(|(e, s)| (e.start - r.start..e.end - r.start, s)).collect();
    apply_edits(&src[r], edits)
}
