//! `ppt/viewProps.xml`: drawing guides, snap to grid and grid spacing.

use ppt_model::{DrawGuide, Emu, ViewSettings};
use roxmltree::Document;

use crate::style::{attr_bool, child, path};
use crate::writer::{apply_edits, open_tag};

/// Guide positions are stored in eighths of a point.
const EMU_PER_UNIT: f64 = 12_700.0 / 8.0;

pub fn parse(xml: &str) -> ViewSettings {
    let mut view = ViewSettings::default();
    let Ok(doc) = Document::parse(xml) else { return view };
    let root = doc.root_element();
    if let Some(pr) = path(root, &["slideViewPr", "cSldViewPr"]) {
        // The schema defaults: snapping on, guides hidden.
        view.snap_to_grid = attr_bool(pr, "snapToGrid").unwrap_or(true);
        view.show_guides = attr_bool(pr, "showGuides").unwrap_or(false);
        if let Some(list) = child(pr, "guideLst") {
            view.guides = list
                .children()
                .filter(|n| n.tag_name().name() == "guide")
                .map(|n| DrawGuide {
                    vertical: n.attribute("orient") != Some("horz"),
                    pos: (n.attribute("pos").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) * EMU_PER_UNIT).round() as Emu,
                })
                .collect();
        }
    }
    if let Some(g) = child(root, "gridSpacing").and_then(|g| g.attribute("cx")?.parse::<Emu>().ok()).filter(|v| *v > 0) {
        view.grid_spacing = g;
    }
    view
}

fn guides_xml(guides: &[DrawGuide]) -> String {
    let items: String = guides
        .iter()
        .map(|g| {
            let pos = (g.pos as f64 / EMU_PER_UNIT).round() as i64;
            if g.vertical { format!(r#"<p:guide pos="{pos}"/>"#) } else { format!(r#"<p:guide orient="horz" pos="{pos}"/>"#) }
        })
        .collect();
    format!("<p:guideLst>{items}</p:guideLst>")
}

/// `xml` with `view`'s settings written in, or `None` when they are already there.
pub fn patch(xml: &str, view: &ViewSettings) -> Option<String> {
    if parse(xml) == *view {
        return None;
    }
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    let mut edits = Vec::new();
    let flag = |b: bool| if b { "1" } else { "0" };
    match path(root, &["slideViewPr", "cSldViewPr"]) {
        Some(pr) => {
            let mut tag = open_tag(xml, pr);
            for (name, value) in [("snapToGrid", flag(view.snap_to_grid)), ("showGuides", flag(view.show_guides))] {
                tag = set_attr(&tag, name, value);
            }
            let start = pr.range().start;
            edits.push((start..start + open_tag(xml, pr).len(), tag));
            match child(pr, "guideLst") {
                Some(old) => edits.push((old.range(), guides_xml(&view.guides))),
                None => {
                    let at = child(pr, "cViewPr").map(|c| c.range().end).unwrap_or_else(|| start + open_tag(xml, pr).len() + 1);
                    // A self-closing element has no room inside: rewrite it whole.
                    if xml[pr.range()].ends_with("/>") {
                        edits.pop();
                        edits.push((pr.range(), format!("{}>{}{}</p:cSldViewPr>", set_attr(&set_attr(&open_tag(xml, pr), "snapToGrid", flag(view.snap_to_grid)), "showGuides", flag(view.show_guides)), CVIEW, guides_xml(&view.guides))));
                    } else {
                        edits.push((at..at, guides_xml(&view.guides)));
                    }
                }
            }
        }
        None => {
            let xml_pr = format!(
                r#"<p:slideViewPr><p:cSldViewPr snapToGrid="{}" showGuides="{}">{CVIEW}{}</p:cSldViewPr></p:slideViewPr>"#,
                flag(view.snap_to_grid),
                flag(view.show_guides),
                guides_xml(&view.guides)
            );
            let at = child(root, "normalViewPr").map(|n| n.range().end).unwrap_or_else(|| root.range().start + open_tag(xml, root).len() + 1);
            edits.push((at..at, xml_pr));
        }
    }
    let spacing = format!(r#"<p:gridSpacing cx="{0}" cy="{0}"/>"#, view.grid_spacing);
    match child(root, "gridSpacing") {
        Some(g) => edits.push((g.range(), spacing)),
        None => {
            let at = child(root, "extLst").map(|e| e.range().start).unwrap_or_else(|| xml[..root.range().end].rfind("</").unwrap_or(root.range().end));
            edits.push((at..at, spacing));
        }
    }
    Some(apply_edits(xml, edits))
}

const CVIEW: &str = r#"<p:cViewPr><p:scale><a:sx n="100" d="100"/><a:sy n="100" d="100"/></p:scale><p:origin x="0" y="0"/></p:cViewPr>"#;

/// Sets attribute `name` on start tag text `tag` (without its closing `>`).
fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    match tag.find(&key) {
        Some(i) => {
            let start = i + key.len();
            let end = tag[start..].find('"').map_or(tag.len(), |j| start + j);
            format!("{}{value}{}", &tag[..start], &tag[end..])
        }
        None => format!(r#"{tag} {name}="{value}""#),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<p:viewPr xmlns:a="a" xmlns:p="p"><p:normalViewPr/><p:slideViewPr><p:cSldViewPr snapToGrid="0"><p:cViewPr varScale="1"/><p:guideLst><p:guide orient="horz" pos="2160"/><p:guide pos="3840"/></p:guideLst></p:cSldViewPr></p:slideViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#;

    #[test]
    fn guides_are_read_in_eighths_of_a_point() {
        let v = parse(XML);
        assert_eq!(v.guides, vec![DrawGuide { vertical: false, pos: 3_429_000 }, DrawGuide { vertical: true, pos: 6_096_000 }]);
        assert!(!v.snap_to_grid && !v.show_guides);
        assert_eq!(v.grid_spacing, 72_008);
        assert_eq!(patch(XML, &v), None, "unchanged");
    }

    #[test]
    fn edited_settings_round_trip() {
        let mut v = parse(XML);
        v.guides.push(DrawGuide { vertical: true, pos: 914_400 });
        v.show_guides = true;
        v.snap_to_grid = true;
        v.grid_spacing = 360_000;
        let out = patch(XML, &v).unwrap();
        assert_eq!(parse(&out), v, "{out}");
        // Without a guide list or grid spacing, both are added.
        let bare = r#"<p:viewPr xmlns:a="a" xmlns:p="p"><p:normalViewPr/></p:viewPr>"#;
        assert_eq!(parse(&patch(bare, &v).unwrap()), v);
    }
}
