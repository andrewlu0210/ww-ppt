//! Reading and writing `p:transition`, including the `mc:AlternateContent`
//! form PowerPoint 2010+ uses for newer types and exact durations.

use std::ops::Range;

use ppt_model::transition::{Dir, Transition, TransitionKind, speed_ms};
use roxmltree::Node;

use crate::writer::esc;

const P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";

fn parse_one(t: Node) -> Transition {
    let duration_ms = t.attribute((P14, "dur")).and_then(|v| v.parse().ok()).unwrap_or_else(|| speed_ms(t.attribute("spd").unwrap_or("fast")));
    let on_click = !matches!(t.attribute("advClick"), Some("0" | "false"));
    let after_ms = t.attribute("advTm").and_then(|v| v.parse().ok());
    let kind = t
        .children()
        .find(|c| c.is_element() && !matches!(c.tag_name().name(), "sndAc" | "extLst"))
        .map_or(TransitionKind::Cut { through_black: false }, kind_of);
    Transition { kind, duration_ms, on_click, after_ms }
}

fn kind_of(k: Node) -> TransitionKind {
    let name = k.tag_name().name();
    let p_ns = k.tag_name().namespace() == Some("http://schemas.openxmlformats.org/presentationml/2006/main");
    let dir = |default| k.attribute("dir").and_then(Dir::from_code).unwrap_or(default);
    let thru = matches!(k.attribute("thruBlk"), Some("1" | "true"));
    if !p_ns {
        return TransitionKind::Other(name.to_string());
    }
    match name {
        "cut" => TransitionKind::Cut { through_black: thru },
        "fade" => TransitionKind::Fade { through_black: thru },
        "push" => TransitionKind::Push(dir(Dir::Left)),
        "wipe" => TransitionKind::Wipe(dir(Dir::Left)),
        "cover" => TransitionKind::Cover(dir(Dir::Left)),
        "pull" => TransitionKind::Pull(dir(Dir::Left)),
        "split" => TransitionKind::Split { vertical: k.attribute("orient") == Some("vert"), outward: k.attribute("dir") != Some("in") },
        "dissolve" => TransitionKind::Dissolve,
        "randomBar" => TransitionKind::RandomBar { vertical: k.attribute("dir") == Some("vert") },
        "zoom" => TransitionKind::Zoom { inward: k.attribute("dir") == Some("in") },
        "circle" => TransitionKind::Circle,
        other => TransitionKind::Other(other.to_string()),
    }
}

/// The slide-level element holding the transition: `p:transition` itself or its `mc:AlternateContent`.
fn transition_node<'a, 'i>(root: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    root.children().find(|c| match c.tag_name().name() {
        "transition" => true,
        "AlternateContent" => c.descendants().any(|d| d.tag_name().name() == "transition"),
        _ => false,
    })
}

/// The slide's transition. Of the alternatives, the first one whose type we
/// play is used (the newer `mc:Choice` first, then the `mc:Fallback`).
pub(crate) fn parse(root: Node) -> Option<Transition> {
    let node = transition_node(root)?;
    let candidates: Vec<Transition> = node.descendants().filter(|d| d.tag_name().name() == "transition").map(parse_one).collect();
    let first = candidates.first()?.clone();
    Some(candidates.into_iter().find(|t| !matches!(t.kind, TransitionKind::Other(_))).unwrap_or(first))
}

fn kind_xml(kind: &TransitionKind) -> String {
    let dir = |name: &str, d: Dir| if d == Dir::Left { format!("<p:{name}/>") } else { format!(r#"<p:{name} dir="{}"/>"#, d.code()) };
    let thru = |name: &str, on: bool| if on { format!(r#"<p:{name} thruBlk="1"/>"#) } else { format!("<p:{name}/>") };
    match kind {
        TransitionKind::Cut { through_black } => thru("cut", *through_black),
        TransitionKind::Fade { through_black } => thru("fade", *through_black),
        TransitionKind::Push(d) => dir("push", *d),
        TransitionKind::Wipe(d) => dir("wipe", *d),
        TransitionKind::Cover(d) => dir("cover", *d),
        TransitionKind::Pull(d) => dir("pull", *d),
        TransitionKind::Split { vertical, outward } => {
            format!(r#"<p:split orient="{}" dir="{}"/>"#, if *vertical { "vert" } else { "horz" }, if *outward { "out" } else { "in" })
        }
        TransitionKind::Dissolve => "<p:dissolve/>".into(),
        TransitionKind::RandomBar { vertical } => format!(r#"<p:randomBar dir="{}"/>"#, if *vertical { "vert" } else { "horz" }),
        TransitionKind::Zoom { inward } => format!(r#"<p:zoom dir="{}"/>"#, if *inward { "in" } else { "out" }),
        TransitionKind::Circle => "<p:circle/>".into(),
        // Types we don't model are written as what they play as.
        TransitionKind::Other(_) => "<p:fade/>".into(),
    }
}

/// XML for `t`: a plain `p:transition` when the duration is one of the
/// standard speeds, else wrapped so PowerPoint 2010+ reads the exact duration.
pub(crate) fn transition_xml(t: &Transition) -> String {
    let spd = [("fast", 500u32), ("med", 750), ("slow", 1000)].into_iter().min_by_key(|(_, ms)| ms.abs_diff(t.duration_ms)).map_or("fast", |(s, _)| s);
    let mut attrs = format!(r#" spd="{spd}""#);
    if !t.on_click {
        attrs.push_str(r#" advClick="0""#);
    }
    if let Some(ms) = t.after_ms {
        attrs.push_str(&format!(r#" advTm="{ms}""#));
    }
    let kind = kind_xml(&t.kind);
    if speed_ms(spd) == t.duration_ms {
        return format!("<p:transition{attrs}>{kind}</p:transition>");
    }
    format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:p14="{P14}" Requires="p14"><p:transition{attrs} p14:dur="{}">{kind}</p:transition></mc:Choice><mc:Fallback><p:transition{attrs}>{kind}</p:transition></mc:Fallback></mc:AlternateContent>"#,
        esc(&t.duration_ms.to_string())
    )
}

/// The edit that sets the slide's transition to `t` (or removes it).
pub(crate) fn transition_edit(doc: &str, root: Node, t: Option<&Transition>) -> (Range<usize>, String) {
    let xml = t.map(transition_xml).unwrap_or_default();
    if let Some(node) = transition_node(root) {
        return (node.range(), xml);
    }
    // Schema order: cSld, clrMapOvr, transition, timing, extLst.
    let at = root
        .children()
        .filter(|c| matches!(c.tag_name().name(), "cSld" | "clrMapOvr"))
        .map(|c| c.range().end)
        .max()
        .unwrap_or_else(|| doc[..root.range().end].rfind("</").unwrap_or(root.range().end));
    (at..at, xml)
}

#[cfg(test)]
mod tests {
    use super::*;
    use roxmltree::Document;

    const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:p159="http://schemas.microsoft.com/office/powerpoint/2015/09/main""#;

    fn read(body: &str) -> Option<Transition> {
        let xml = format!("<p:sld {NS}><p:cSld/><p:clrMapOvr/>{body}</p:sld>");
        let doc = Document::parse(&xml).unwrap();
        parse(doc.root_element())
    }

    #[test]
    fn reads_plain_and_alternate_content() {
        let t = read(r#"<p:transition spd="slow" advClick="0" advTm="3000"><p:push dir="u"/></p:transition>"#).unwrap();
        assert_eq!(t, Transition { kind: TransitionKind::Push(Dir::Up), duration_ms: 1000, on_click: false, after_ms: Some(3000) });
        // A type we play in the Choice: its exact duration is used.
        let t = read(r#"<mc:AlternateContent><mc:Choice Requires="p14"><p:transition p14:dur="100"><p:cut/></p:transition></mc:Choice><mc:Fallback><p:transition><p:cut/></p:transition></mc:Fallback></mc:AlternateContent>"#).unwrap();
        assert_eq!((t.kind, t.duration_ms), (TransitionKind::Cut { through_black: false }, 100));
        // Morph is not played: the fallback's fade is.
        let t = read(r#"<mc:AlternateContent><mc:Choice Requires="p159"><p:transition spd="slow" p14:dur="2000"><p159:morph option="byObject"/></p:transition></mc:Choice><mc:Fallback><p:transition spd="slow"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"#).unwrap();
        assert_eq!((t.kind, t.duration_ms), (TransitionKind::Fade { through_black: false }, 1000));
        // Without a usable fallback the unknown type is kept.
        let t = read(r#"<p:transition><p14:vortex dir="r"/></p:transition>"#).unwrap();
        assert_eq!(t.kind, TransitionKind::Other("vortex".into()));
        assert_eq!(read(r#"<p:transition/>"#).unwrap().kind, TransitionKind::Cut { through_black: false });
        assert_eq!(read(""), None);
    }

    #[test]
    fn writes_what_it_reads() {
        for t in [
            Transition { kind: TransitionKind::Split { vertical: true, outward: false }, duration_ms: 750, on_click: true, after_ms: None },
            Transition { kind: TransitionKind::Cover(Dir::RightDown), duration_ms: 1600, on_click: false, after_ms: Some(2500) },
            Transition { kind: TransitionKind::Fade { through_black: true }, duration_ms: 500, on_click: true, after_ms: None },
        ] {
            assert_eq!(read(&transition_xml(&t)).as_ref(), Some(&t));
        }
        assert!(!transition_xml(&Transition { duration_ms: 750, ..Default::default() }).contains("AlternateContent"));
    }

    #[test]
    fn edits_replace_insert_and_remove() {
        let apply = |xml: &str, t: Option<&Transition>| {
            let doc = Document::parse(xml).unwrap();
            let (r, s) = transition_edit(xml, doc.root_element(), t);
            format!("{}{s}{}", &xml[..r.start], &xml[r.end..])
        };
        let bare = format!("<p:sld {NS}><p:cSld/><p:clrMapOvr/><p:timing/></p:sld>");
        let t = Transition::default();
        let with = apply(&bare, Some(&t));
        assert!(with.contains("<p:clrMapOvr/><mc:AlternateContent") && with.ends_with("<p:timing/></p:sld>"), "{with}");
        let doc = Document::parse(&with).unwrap();
        assert_eq!(parse(doc.root_element()), Some(t));
        assert_eq!(apply(&with, None), bare);
    }
}
