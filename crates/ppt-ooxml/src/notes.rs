//! Speaker notes (`p:notes` parts): the text of the body placeholder as plain
//! text, one line per paragraph.

use roxmltree::{Document, Node};

use crate::writer::{apply_edits, esc};

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
pub(crate) const CT_NOTES_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";
pub(crate) const CT_NOTES_MASTER: &str = "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml";
pub(crate) const CT_THEME: &str = "application/vnd.openxmlformats-officedocument.theme+xml";

fn is_body_placeholder(sp: Node) -> bool {
    sp.tag_name().name() == "sp"
        && sp.descendants().any(|n| n.tag_name().name() == "ph" && n.parent().is_some_and(|p| p.tag_name().name() == "nvPr") && n.attribute("type") == Some("body"))
}

fn body_tx<'a, 'i>(doc: &'a Document<'i>) -> Option<(Node<'a, 'i>, Option<Node<'a, 'i>>)> {
    let sp = doc.descendants().find(|n| is_body_placeholder(*n))?;
    let tx = sp.children().find(|c| c.tag_name().name() == "txBody");
    Some((sp, tx))
}

/// The notes text of a `p:notes` part ("" when it has none).
pub(crate) fn notes_text(xml: &str) -> String {
    let Ok(doc) = Document::parse(xml) else { return String::new() };
    let Some((_, Some(tx))) = body_tx(&doc) else { return String::new() };
    let paragraphs: Vec<String> = tx
        .children()
        .filter(|p| p.tag_name().name() == "p")
        .map(|p| {
            let mut line = String::new();
            for n in p.children() {
                match n.tag_name().name() {
                    "r" | "fld" => line.extend(n.children().filter(|t| t.tag_name().name() == "t").filter_map(|t| t.text())),
                    "br" => line.push('\n'),
                    _ => {}
                }
            }
            line
        })
        .collect();
    let text = paragraphs.join("\n");
    // A notes body that only holds empty paragraphs means "no notes".
    if text.trim().is_empty() { String::new() } else { text }
}

/// Paragraphs for `text`, with `rpr` (an `a:rPr` element) on every run.
fn paragraphs_xml(text: &str, rpr: &str) -> String {
    let end = rpr.replacen("<a:rPr", "<a:endParaRPr", 1).replace("</a:rPr>", "</a:endParaRPr>");
    text.split('\n')
        .map(|line| if line.is_empty() { format!("<a:p>{end}</a:p>") } else { format!("<a:p><a:r>{rpr}<a:t>{}</a:t></a:r></a:p>", esc(line)) })
        .collect()
}

fn body_sp_xml(id: u32, text: &str, rpr: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Notes Placeholder {id}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>"#,
        paragraphs_xml(text, rpr)
    )
}

const DEFAULT_RPR: &str = r#"<a:rPr lang="zh-TW" altLang="en-US"/>"#;

/// `xml` (a `p:notes` part) with its notes text replaced. The run properties
/// of the first run (or paragraph end) are kept for all the new text.
pub(crate) fn with_notes_text(xml: &str, text: &str) -> String {
    let Ok(doc) = Document::parse(xml) else { return xml.to_string() };
    match body_tx(&doc) {
        Some((_, Some(tx))) => {
            let rpr = tx
                .descendants()
                .find(|n| matches!(n.tag_name().name(), "rPr" | "endParaRPr"))
                .map(|n| {
                    let s = &xml[n.range()];
                    if n.tag_name().name() == "endParaRPr" { s.replacen("<a:endParaRPr", "<a:rPr", 1).replace("</a:endParaRPr>", "</a:rPr>") } else { s.to_string() }
                })
                .unwrap_or_else(|| DEFAULT_RPR.to_string());
            // Keep a:bodyPr and a:lstStyle; replace the paragraphs.
            let paras: Vec<Node> = tx.children().filter(|c| c.tag_name().name() == "p").collect();
            let new = paragraphs_xml(text, &rpr);
            let range = match (paras.first(), paras.last()) {
                (Some(f), Some(l)) => f.range().start..l.range().end,
                _ => {
                    let at = tx.children().filter(|c| c.is_element()).map(|c| c.range().end).max().unwrap_or(tx.range().end);
                    at..at
                }
            };
            apply_edits(xml, vec![(range, new)])
        }
        Some((sp, None)) => {
            let at = sp.range().end - "</p:sp>".len();
            apply_edits(xml, vec![(at..at, format!("<p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody>", paragraphs_xml(text, DEFAULT_RPR)))])
        }
        None => {
            let Some(tree) = doc.descendants().find(|n| n.tag_name().name() == "spTree") else { return xml.to_string() };
            let id = doc.descendants().filter(|n| n.tag_name().name() == "cNvPr").filter_map(|n| n.attribute("id")?.parse::<u32>().ok()).max().unwrap_or(1) + 1;
            let at = tree.range().end - "</p:spTree>".len();
            apply_edits(xml, vec![(at..at, body_sp_xml(id, text, DEFAULT_RPR))])
        }
    }
}

/// A new notes slide holding `text`.
pub(crate) fn notes_slide_xml(text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notes {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
        body_sp_xml(3, text, DEFAULT_RPR)
    )
}

/// A minimal notes master for decks without one: slide image above, notes
/// body below, on a portrait `notes_w` x `notes_h` page.
pub(crate) fn notes_master_xml(slide_w: i64, slide_h: i64, notes_w: i64, notes_h: i64) -> String {
    let margin = notes_w / 10;
    let img_w = notes_w - 2 * margin;
    let img_h = img_w * slide_h / slide_w.max(1);
    let img_y = notes_h / 8;
    let body_y = img_y + img_h + notes_h / 30;
    let body_h = notes_h - body_y - notes_h / 10;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:notesMaster {NS}><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{margin}" y="{img_y}"/><a:ext cx="{img_w}" cy="{img_h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/><a:ln w="12700"><a:solidFill><a:prstClr val="black"/></a:solidFill></a:ln></p:spPr></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" sz="quarter" idx="3"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{margin}" y="{body_y}"/><a:ext cx="{img_w}" cy="{body_h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"/><a:lstStyle/><a:p><a:pPr lvl="0"/><a:r><a:rPr lang="zh-TW" altLang="en-US"/><a:t>Click to edit Master text styles</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:notesStyle><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1200" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:notesStyle></p:notesMaster>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id="2" name="img"/><p:cNvSpPr/><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="body"/><p:cNvSpPr/><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="zh-CN" sz="1400"/><a:t>第一段</a:t></a:r><a:r><a:rPr lang="zh-CN"/><a:t> A&amp;B</a:t></a:r></a:p><a:p><a:endParaRPr lang="zh-CN"/></a:p><a:p><a:r><a:rPr lang="zh-CN"/><a:t>三</a:t></a:r><a:br><a:rPr lang="zh-CN"/></a:br><a:r><a:rPr lang="zh-CN"/><a:t>四</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="4" name="num"/><p:cNvSpPr/><p:nvPr><p:ph type="sldNum" idx="5"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:fld type="slidenum"><a:t>1</a:t></a:fld></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:notes>"#;

    #[test]
    fn reads_body_text_only() {
        assert_eq!(notes_text(SAMPLE), "第一段 A&B\n\n三\n四");
        assert_eq!(notes_text(&with_notes_text(SAMPLE, "  \n")), "");
    }

    #[test]
    fn rewrites_keeping_run_format_and_other_shapes() {
        let xml = with_notes_text(SAMPLE, "新的 <內容>\n\n第三行");
        assert_eq!(notes_text(&xml), "新的 <內容>\n\n第三行");
        assert!(xml.contains(r#"<a:rPr lang="zh-CN" sz="1400"/><a:t>新的 &lt;內容&gt;</a:t>"#), "{xml}");
        assert!(xml.contains(r#"<a:endParaRPr lang="zh-CN" sz="1400"/>"#));
        // The slide number placeholder is untouched.
        assert!(xml.contains(r#"<a:fld type="slidenum"><a:t>1</a:t></a:fld>"#));
    }

    #[test]
    fn adds_a_body_when_missing() {
        let bare = r#"<p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/></p:nvGrpSpPr></p:spTree></p:cSld></p:notes>"#;
        assert_eq!(notes_text(&with_notes_text(bare, "hi")), "hi");
        assert_eq!(notes_text(&notes_slide_xml("a\nb")), "a\nb");
        assert!(Document::parse(&notes_master_xml(12192000, 6858000, 6858000, 9144000)).is_ok());
    }
}
