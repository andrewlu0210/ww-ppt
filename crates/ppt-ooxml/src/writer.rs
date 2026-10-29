//! PPTX export.
//!
//! Saving starts from the package the deck was loaded from (or the built-in
//! template): untouched parts are copied byte for byte, untouched slides keep
//! their XML, and inside edited slides only the changed shapes are rewritten,
//! and of those only the changed elements (`spPr`, paragraphs, ids).

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::ops::Range;

use ppt_model::{
    Align, Anchor, Bullet, BulletKind, Color, Crop, Dash, EndKind, Geometry, GradientKind, Line, LineEnd, Paint, ParaDefaults, Paragraph, PathCmd, Presentation, RunStyle, Shape, ShapeKind,
    ShapeSource, Slide, SlideSource, TextBody, Xfrm, ContainerSource, Link,
};
use roxmltree::{Document, Node};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Error;
use crate::transition::{transition_edit, transition_xml};
use crate::notes::{CT_NOTES_MASTER, CT_NOTES_SLIDE, CT_THEME, notes_master_xml, notes_slide_xml, notes_text, with_notes_text};
use crate::table_writer::{TABLE_URI, patch_table, style_xml as table_style_xml, table_xml};
use crate::package::{REL_NS, resolve};

const NS_DECLS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const REL_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";

pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

fn rels_part_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Target of `target` relative to the directory of `from`.
fn relative_target(from: &str, target: &str) -> String {
    let from_dir: Vec<&str> = from.split('/').collect::<Vec<_>>().split_last().map(|(_, d)| d.to_vec()).unwrap_or_default();
    let to: Vec<&str> = target.split('/').collect();
    let common = from_dir.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_string(); from_dir.len() - common];
    parts.extend(to[common..].iter().map(|s| s.to_string()));
    parts.join("/")
}

#[derive(Clone, Debug, PartialEq)]
struct RelEntry {
    id: String,
    kind: String,
    /// Absolute part name, or the URL for external relationships.
    target: String,
    external: bool,
}

fn parse_rels(xml: &str, owner: &str) -> Vec<RelEntry> {
    let dir = owner.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let Ok(doc) = Document::parse(xml) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|n| n.tag_name().name() == "Relationship")
        .filter_map(|n| {
            let external = n.attribute("TargetMode") == Some("External");
            let target = n.attribute("Target")?;
            Some(RelEntry {
                id: n.attribute("Id")?.to_string(),
                kind: n.attribute("Type").unwrap_or_default().to_string(),
                target: if external { target.to_string() } else { resolve(dir, target) },
                external,
            })
        })
        .collect()
}

fn rels_xml(entries: &[RelEntry], owner: &str) -> String {
    let mut out = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#);
    out.push_str(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for r in entries {
        let target = if r.external { r.target.clone() } else { relative_target(owner, &r.target) };
        let mode = if r.external { r#" TargetMode="External""# } else { "" };
        out.push_str(&format!(r#"<Relationship Id="{}" Type="{}" Target="{}"{mode}/>"#, esc(&r.id), esc(&r.kind), esc(&target)));
    }
    out.push_str("</Relationships>");
    out
}

/// Applies non-overlapping `(range, replacement)` edits to `s`.
pub(crate) fn apply_edits(s: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = s.to_string();
    for (r, text) in edits {
        out.replace_range(r, &text);
    }
    out
}

pub(crate) fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// Start tag of `n` without its closing `>` or `/>`, e.g. `<p:spPr bwMode="auto"`.
pub(crate) fn open_tag(src: &str, n: Node) -> String {
    let tag = &src[n.range()];
    let end = tag.find('>').unwrap_or(tag.len());
    tag[..end].trim_end_matches('/').trim_end().to_string()
}

// --- Generated XML fragments ----------------------------------------------

fn xfrm_xml(tag: &str, x: &Xfrm) -> String {
    let mut attrs = String::new();
    if x.rot != 0.0 {
        attrs.push_str(&format!(r#" rot="{}""#, (x.rot * 60_000.0).round() as i64));
    }
    if x.flip_h {
        attrs.push_str(r#" flipH="1""#);
    }
    if x.flip_v {
        attrs.push_str(r#" flipV="1""#);
    }
    format!(r#"<{tag}{attrs}><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></{tag}>"#, x.x, x.y, x.w.max(0), x.h.max(0))
}

pub(crate) fn color_xml(c: Color) -> String {
    let alpha = if c.a < 255 { format!(r#"<a:alpha val="{}"/>"#, (c.a as f64 / 255.0 * 100_000.0).round() as i64) } else { String::new() };
    if alpha.is_empty() {
        format!(r#"<a:srgbClr val="{:02X}{:02X}{:02X}"/>"#, c.r, c.g, c.b)
    } else {
        format!(r#"<a:srgbClr val="{:02X}{:02X}{:02X}">{alpha}</a:srgbClr>"#, c.r, c.g, c.b)
    }
}

fn src_rect_xml(crop: &Crop) -> String {
    if crop.is_none() {
        return String::new();
    }
    let v = |f: f64| (f * 100_000.0).round() as i64;
    format!(r#"<a:srcRect l="{}" t="{}" r="{}" b="{}"/>"#, v(crop.left), v(crop.top), v(crop.right), v(crop.bottom))
}

fn fill_xml(fill: &Option<Paint>) -> String {
    match fill {
        Some(Paint::Solid(c)) => format!("<a:solidFill>{}</a:solidFill>", color_xml(*c)),
        Some(Paint::Gradient(g)) => {
            let stops: String = g.stops.iter().map(|(pos, c)| format!(r#"<a:gs pos="{}">{}</a:gs>"#, (pos * 100_000.0).round() as i64, color_xml(*c))).collect();
            let shade = match g.kind {
                GradientKind::Linear { angle, scaled } => format!(r#"<a:lin ang="{}" scaled="{}"/>"#, (angle * 60_000.0).round() as i64, u8::from(scaled)),
                GradientKind::Path { circle, focus } => {
                    let [l, t, r, b] = focus.map(|f| (f * 100_000.0).round() as i64);
                    format!(r#"<a:path path="{}"><a:fillToRect l="{l}" t="{t}" r="{r}" b="{b}"/></a:path>"#, if circle { "circle" } else { "rect" })
                }
            };
            format!(r#"<a:gradFill rotWithShape="{}"><a:gsLst>{stops}</a:gsLst>{shade}</a:gradFill>"#, u8::from(g.rotate_with_shape))
        }
        None => "<a:noFill/>".to_string(),
    }
}

fn line_xml(line: Option<Line>) -> String {
    let Some(l) = line else { return "<a:ln><a:noFill/></a:ln>".to_string() };
    let dash = if l.dash == Dash::Solid { String::new() } else { format!(r#"<a:prstDash val="{}"/>"#, l.dash.name()) };
    let end = |tag: &str, e: LineEnd| {
        if e.kind == EndKind::None {
            return String::new();
        }
        let size = |v: u8| ["sm", "med", "lg"][v.min(2) as usize];
        format!(r#"<a:{tag} type="{}" w="{}" len="{}"/>"#, e.kind.name(), size(e.width), size(e.length))
    };
    format!(r#"<a:ln w="{}"><a:solidFill>{}</a:solidFill>{dash}{}{}</a:ln>"#, l.width, color_xml(l.color), end("headEnd", l.head), end("tailEnd", l.tail))
}

fn geometry_xml(g: &Geometry) -> String {
    match g {
        Geometry::Preset { name, adjust } => {
            let gds: String = adjust.iter().map(|(k, v)| format!(r#"<a:gd name="{}" fmla="val {}"/>"#, esc(k), v.round() as i64)).collect();
            format!(r#"<a:prstGeom prst="{}"><a:avLst>{gds}</a:avLst></a:prstGeom>"#, esc(name))
        }
        Geometry::Custom(paths) => {
            let mut out = String::from(r#"<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst>"#);
            for p in paths {
                let fill = if p.fill { "" } else { r#" fill="none""# };
                let stroke = if p.stroke { "" } else { r#" stroke="0""# };
                out.push_str(&format!(r#"<a:path w="{}" h="{}"{fill}{stroke}>"#, p.w.round() as i64, p.h.round() as i64));
                let pt = |x: f64, y: f64| format!(r#"<a:pt x="{}" y="{}"/>"#, x.round() as i64, y.round() as i64);
                for c in &p.cmds {
                    out.push_str(&match *c {
                        PathCmd::MoveTo(x, y) => format!("<a:moveTo>{}</a:moveTo>", pt(x, y)),
                        PathCmd::LineTo(x, y) => format!("<a:lnTo>{}</a:lnTo>", pt(x, y)),
                        PathCmd::QuadTo(a, b, x, y) => format!("<a:quadBezTo>{}{}</a:quadBezTo>", pt(a, b), pt(x, y)),
                        PathCmd::CubicTo(a, b, c2, d, x, y) => format!("<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(a, b), pt(c2, d), pt(x, y)),
                        PathCmd::Close => "<a:close/>".to_string(),
                    });
                }
                out.push_str("</a:path>");
            }
            out.push_str("</a:pathLst></a:custGeom>");
            out
        }
    }
}

fn fallback_defaults() -> ParaDefaults {
    ParaDefaults {
        style: RunStyle::default(),
        align: Align::Left,
        bullet: None,
        line_spacing: 1.0,
        mar_l: 0,
        indent: 0,
        space_before: 0.0,
        space_after: 0.0,
    }
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{F900}'..='\u{FAFF}').contains(&c) || ('\u{FF00}'..='\u{FFEF}').contains(&c))
}

/// `a:rPr`-style element with only the properties that differ from `base`.
fn rpr_xml(tag: &str, s: &RunStyle, base: &RunStyle, text: &str, links: &mut Links) -> String {
    let lang = if has_cjk(text) { "zh-TW" } else { "en-US" };
    let mut attrs = format!(r#" lang="{lang}" altLang="en-US""#);
    if (s.size_pt - base.size_pt).abs() > 0.01 {
        attrs.push_str(&format!(r#" sz="{}""#, (s.size_pt * 100.0).round() as i64));
    }
    if s.bold != base.bold {
        attrs.push_str(if s.bold { r#" b="1""# } else { r#" b="0""# });
    }
    if s.italic != base.italic {
        attrs.push_str(if s.italic { r#" i="1""# } else { r#" i="0""# });
    }
    if s.underline != base.underline {
        attrs.push_str(if s.underline { r#" u="sng""# } else { r#" u="none""# });
    }
    attrs.push_str(r#" dirty="0""#);
    let mut kids = String::new();
    if s.color != base.color {
        kids.push_str(&format!("<a:solidFill>{}</a:solidFill>", color_xml(s.color)));
    }
    if s.latin_font != base.latin_font && !s.latin_font.is_empty() {
        kids.push_str(&format!(r#"<a:latin typeface="{}"/>"#, esc(&s.latin_font)));
    }
    if s.ea_font != base.ea_font && !s.ea_font.is_empty() {
        kids.push_str(&format!(r#"<a:ea typeface="{}"/>"#, esc(&s.ea_font)));
    }
    if let Some(l) = &s.link {
        let rid = if l.target.is_empty() { String::new() } else { links(l) };
        let action = l.action.as_ref().map(|a| format!(r#" action="{}""#, esc(a))).unwrap_or_default();
        let tooltip = l.tooltip.as_ref().map(|t| format!(r#" tooltip="{}""#, esc(t))).unwrap_or_default();
        kids.push_str(&format!(r#"<a:hlinkClick r:id="{rid}"{action}{tooltip}/>"#));
    }
    if kids.is_empty() { format!("<{tag}{attrs}/>") } else { format!("<{tag}{attrs}>{kids}</{tag}>") }
}

/// Bullet elements of `a:pPr`, in schema order (color, size, font, kind).
fn bullet_xml(b: Option<&Bullet>) -> String {
    let Some(b) = b else { return "<a:buNone/>".to_string() };
    let mut out = String::new();
    if let Some(c) = b.color {
        out.push_str(&format!("<a:buClr>{}</a:buClr>", color_xml(c)));
    }
    if let Some(s) = b.size {
        out.push_str(&format!(r#"<a:buSzPct val="{}"/>"#, (s * 100_000.0).round() as i64));
    }
    if let Some(f) = &b.font {
        out.push_str(&format!(r#"<a:buFont typeface="{}"/>"#, esc(f)));
    }
    match &b.kind {
        BulletKind::Char(c) => out.push_str(&format!(r#"<a:buChar char="{}"/>"#, esc(&c.to_string()))),
        BulletKind::AutoNum { scheme, start } => {
            let start = if *start > 1 { format!(r#" startAt="{start}""#) } else { String::new() };
            out.push_str(&format!(r#"<a:buAutoNum type="{}"{start}/>"#, esc(scheme)));
        }
    }
    out
}

fn paragraph_xml(p: &Paragraph, d: &ParaDefaults, links: &mut Links) -> String {
    let mut attrs = String::new();
    if p.level > 0 {
        attrs.push_str(&format!(r#" lvl="{}""#, p.level));
    }
    if p.mar_l != d.mar_l {
        attrs.push_str(&format!(r#" marL="{}""#, p.mar_l));
    }
    if p.indent != d.indent {
        attrs.push_str(&format!(r#" indent="{}""#, p.indent));
    }
    if p.align != d.align {
        let a = match p.align {
            Align::Left => "l",
            Align::Center => "ctr",
            Align::Right => "r",
            Align::Justify => "just",
        };
        attrs.push_str(&format!(r#" algn="{a}""#));
    }
    let mut kids = String::new();
    if (p.line_spacing - d.line_spacing).abs() > 0.001 {
        kids.push_str(&format!(r#"<a:lnSpc><a:spcPct val="{}"/></a:lnSpc>"#, (p.line_spacing * 100_000.0).round() as i64));
    }
    if (p.space_before - d.space_before).abs() > 0.01 {
        kids.push_str(&format!(r#"<a:spcBef><a:spcPts val="{}"/></a:spcBef>"#, (p.space_before * 100.0).round() as i64));
    }
    if (p.space_after - d.space_after).abs() > 0.01 {
        kids.push_str(&format!(r#"<a:spcAft><a:spcPts val="{}"/></a:spcAft>"#, (p.space_after * 100.0).round() as i64));
    }
    if p.bullet != d.bullet {
        kids.push_str(&bullet_xml(p.bullet.as_ref()));
    }
    let mut out = String::from("<a:p>");
    if !attrs.is_empty() || !kids.is_empty() {
        out.push_str(&if kids.is_empty() { format!("<a:pPr{attrs}/>") } else { format!("<a:pPr{attrs}>{kids}</a:pPr>") });
    }
    for run in &p.runs {
        for (i, piece) in run.text.split('\u{2028}').enumerate() {
            if i > 0 {
                out.push_str(&format!("<a:br>{}</a:br>", rpr_xml("a:rPr", &run.style, &d.style, piece, links)));
            }
            if !piece.is_empty() {
                out.push_str(&format!("<a:r>{}<a:t>{}</a:t></a:r>", rpr_xml("a:rPr", &run.style, &d.style, piece, links), esc(piece)));
            }
        }
    }
    let text = p.text();
    // The end-of-paragraph mark never carries a link.
    let end = RunStyle { link: None, ..p.end_style.clone() };
    out.push_str(&rpr_xml("a:endParaRPr", &end, &d.style, &text, links));
    out.push_str("</a:p>");
    out
}

/// Turns a hyperlink into a relationship id of the part being written.
pub(crate) type Links<'a> = dyn FnMut(&Link) -> String + 'a;

pub(crate) fn paragraphs_xml(body: &TextBody, links: &mut Links) -> String {
    let fallback = fallback_defaults();
    body.paragraphs.iter().map(|p| paragraph_xml(p, body.defaults_for(p.level).unwrap_or(&fallback), links)).collect()
}

/// Relationship id for a hyperlink on the slide `out` writes.
fn link_rel(out: &mut SlideOut, l: &Link) -> String {
    out.rel_for(&l.kind, &l.target, l.external)
}

fn body_pr_xml(body: &TextBody) -> String {
    let anchor = match body.anchor {
        Anchor::Top => "t",
        Anchor::Middle => "ctr",
        Anchor::Bottom => "b",
    };
    let [l, t, r, b] = body.insets;
    let wrap = if body.wrap { "square" } else { "none" };
    let fit = if body.auto_grow { "<a:spAutoFit/>" } else { "<a:noAutofit/>" };
    format!(r#"<a:bodyPr wrap="{wrap}" lIns="{l}" tIns="{t}" rIns="{r}" bIns="{b}" rtlCol="0" anchor="{anchor}">{fit}</a:bodyPr>"#)
}

fn new_tx_body_xml(body: &TextBody, out: &mut SlideOut) -> String {
    format!("<p:txBody>{}<a:lstStyle/>{}</p:txBody>", body_pr_xml(body), paragraphs_xml(body, &mut |l| link_rel(out, l)))
}

// --- Per-slide output -------------------------------------------------------

/// Relationship types whose target belongs to one reference only: PowerPoint
/// rejects a chart or diagram part shared by two graphic frames.
fn owned_part(kind: &str) -> bool {
    ["/chart", "/diagramData", "/diagramLayout", "/diagramQuickStyle", "/diagramColors", "/diagramDrawing", "/oleObject", "/package"].iter().any(|k| kind.ends_with(k))
}

/// Parts created during this save: media keyed by `Presentation::media` key,
/// and copies of owned parts (charts, diagrams) for shapes that duplicate them.
#[derive(Default)]
struct MediaOut {
    parts: HashMap<String, String>,
    new_files: Vec<(String, Vec<u8>)>,
    existing: HashSet<String>,
    /// Owned part → (slide part that references it, whether that reference was written).
    owned: HashMap<String, (String, bool)>,
    /// Every part name in the package, including the ones created so far.
    taken: HashSet<String>,
    /// Content types of the original parts (`Override` entries), and the ones for copies.
    content_types: HashMap<String, String>,
    new_overrides: Vec<(String, String)>,
}

impl MediaOut {
    /// The target a reference from `slide` to owned part `target` should use:
    /// the part itself for its first (or original) reference, a fresh copy otherwise.
    fn claim(&mut self, pres: &Presentation, target: &str, slide: &str) -> String {
        match self.owned.get_mut(target) {
            Some((owner, written)) if owner == slide && !*written => {
                *written = true;
                target.to_string()
            }
            Some(_) => {
                let copy = self.copy_part(pres, target);
                self.owned.insert(copy.clone(), (slide.to_string(), true));
                copy
            }
            None => {
                self.owned.insert(target.to_string(), (slide.to_string(), true));
                target.to_string()
            }
        }
    }

    /// Whether every owned part `src` refers to is still unwritten by this slide.
    fn owned_free(&self, src: &ShapeSource, slide: &str) -> bool {
        src.rels.iter().filter(|(_, kind, _)| owned_part(kind)).all(|(_, _, t)| self.owned.get(t).is_none_or(|(owner, written)| owner == slide && !written))
    }

    /// Copies `part` and the parts it relates to (except shared media) under new names.
    fn copy_part(&mut self, pres: &Presentation, part: &str) -> String {
        let Some(pkg) = pres.package.as_ref() else { return part.to_string() };
        let Ok(mut zip) = ZipArchive::new(Cursor::new(pkg.as_slice())) else { return part.to_string() };
        let mut bytes = Vec::new();
        if zip.by_name(part).map(|mut f| f.read_to_end(&mut bytes)).is_err() {
            return part.to_string();
        }
        let (stem, ext) = part.rsplit_once('.').map_or((part, ""), |(s, e)| (s, e));
        let prefix = stem.trim_end_matches(|c: char| c.is_ascii_digit());
        let mut n = 1;
        let copy = loop {
            let name = format!("{prefix}{n}.{ext}");
            if self.taken.insert(name.clone()) {
                break name;
            }
            n += 1;
        };
        let rels = parse_rels(&read_part(&mut zip, &rels_part_of(part)).unwrap_or_default(), part);
        if !rels.is_empty() {
            let rels: Vec<RelEntry> = rels
                .into_iter()
                .map(|mut r| {
                    if !r.external && !r.target.starts_with("ppt/media/") {
                        r.target = self.copy_part(pres, &r.target);
                    }
                    r
                })
                .collect();
            self.new_files.push((rels_part_of(&copy), rels_xml(&rels, &copy).into_bytes()));
        }
        if let Some(ct) = self.content_types.get(part).cloned() {
            self.new_overrides.push((copy.clone(), ct));
        }
        self.new_files.push((copy.clone(), bytes));
        copy
    }

    fn part_for(&mut self, pres: &Presentation, key: &str) -> Option<String> {
        if self.existing.contains(key) {
            return Some(key.to_string());
        }
        if let Some(p) = self.parts.get(key) {
            return Some(p.clone());
        }
        let bytes = pres.media.get(key)?;
        let ext = key.rsplit('.').next().filter(|e| e.len() <= 4).unwrap_or("png").to_lowercase();
        let part = format!("ppt/media/ww-image{}.{ext}", self.parts.len() + 1);
        self.parts.insert(key.to_string(), part.clone());
        self.new_files.push((part.clone(), bytes.to_vec()));
        Some(part)
    }
}

struct SlideOut<'a> {
    part: String,
    rels: Vec<RelEntry>,
    used_ids: HashSet<u32>,
    next_id: u32,
    pres: &'a Presentation,
    media: &'a mut MediaOut,
    /// Namespace declarations raw shapes need on the root element.
    namespaces: Vec<(String, String)>,
}

impl SlideOut<'_> {
    fn rel_for(&mut self, kind: &str, target: &str, external: bool) -> String {
        if let Some(r) = self.rels.iter().find(|r| r.kind == kind && r.target == target && r.external == external) {
            return r.id.clone();
        }
        let mut n = self.rels.len() + 1;
        while self.rels.iter().any(|r| r.id == format!("rId{n}")) {
            n += 1;
        }
        let id = format!("rId{n}");
        self.rels.push(RelEntry { id: id.clone(), kind: kind.to_string(), target: target.to_string(), external });
        id
    }

    /// Returns `want` if it is still free on this slide, else a fresh id.
    fn take_id(&mut self, want: u32) -> u32 {
        let id = if want != 0 && !self.used_ids.contains(&want) {
            want
        } else {
            while self.used_ids.contains(&self.next_id) {
                self.next_id += 1;
            }
            self.next_id
        };
        self.used_ids.insert(id);
        id
    }

    fn need_namespaces(&mut self, ns: &[(String, String)]) {
        for (prefix, uri) in ns {
            if !prefix.is_empty() && !self.namespaces.iter().any(|(p, _)| p == prefix) {
                self.namespaces.push((prefix.clone(), uri.clone()));
            }
        }
    }
}

fn shape_unchanged(shape: &Shape, src: &ShapeSource) -> bool {
    let p = &src.parsed;
    shape.xfrm == p.xfrm && shape.fill == p.fill && shape.line == p.line && shape.text == p.text && shape.kind == p.kind && shape.group == p.group
}

/// Parses a shape's XML out of context by wrapping it with its namespace declarations.
fn wrap(xml: &str, namespaces: &[(String, String)]) -> (String, usize) {
    let decls: String = namespaces
        .iter()
        .map(|(p, u)| if p.is_empty() { format!(r#" xmlns="{}""#, esc(u)) } else { format!(r#" xmlns:{p}="{}""#, esc(u)) })
        .collect();
    let head = format!("<w{decls}>");
    let offset = head.len();
    (format!("{head}{xml}</w>"), offset)
}

/// Rewrites a loaded shape's XML for `out`, touching only what changed.
fn patch_shape(shape: &Shape, src: &ShapeSource, force_xfrm: bool, out: &mut SlideOut) -> String {
    out.need_namespaces(&src.namespaces);
    let same_part = src.part == out.part;

    // Pass 1: attribute values (shape id, relationship ids).
    let (wrapped, offset) = wrap(src.xml(), &src.namespaces);
    let Ok(doc) = Document::parse(&wrapped) else { return src.xml().to_string() };
    let Some(el) = doc.root_element().first_element_child() else { return src.xml().to_string() };
    let mut edits = Vec::new();
    if let Some(c_nv_pr) = el.descendants().find(|n| n.tag_name().name() == "cNvPr")
        && let Some(attr) = c_nv_pr.attribute_node("id")
    {
        let want: u32 = attr.value().parse().unwrap_or(0);
        let id = out.take_id(want);
        if id != want {
            edits.push((attr.range_value(), id.to_string()));
        }
    }
    for n in el.descendants().filter(|n| n.is_element()) {
        for a in n.attributes().filter(|a| a.namespace() == Some(REL_NS)) {
            let Some((_, kind, target)) = src.rels.iter().find(|(id, _, _)| id == a.value()) else { continue };
            let (target, external) = match target.strip_prefix("external:") {
                Some(url) => (url.to_string(), true),
                None => (target.clone(), false),
            };
            let owned = !external && owned_part(kind);
            if same_part && !owned {
                continue;
            }
            let new_target = if owned { out.media.claim(out.pres, &target, &out.part) } else { target.clone() };
            if same_part && new_target == target {
                continue;
            }
            let id = out.rel_for(kind, &new_target, external);
            if id != a.value() {
                edits.push((a.range_value(), id));
            }
        }
    }
    let wrapped = apply_edits(&wrapped, edits);

    // Pass 2: structural changes.
    let Ok(doc) = Document::parse(&wrapped) else { return wrapped[offset..wrapped.len() - 4].to_string() };
    let el = doc.root_element().first_element_child().expect("element survives pass 1");
    let p = &src.parsed;
    let xfrm_changed = force_xfrm || shape.xfrm != p.xfrm;
    let fill_changed = shape.fill != p.fill;
    let line_changed = shape.line != p.line;
    let text_changed = shape.text != p.text;
    let mut edits = Vec::new();
    match el.tag_name().name() {
        "sp" | "cxnSp" | "pic" => {
            if xfrm_changed || fill_changed || line_changed {
                let sp_pr = child(el, "spPr");
                let new = rebuild_sp_pr(&wrapped, sp_pr, shape, xfrm_changed, fill_changed, line_changed);
                match sp_pr {
                    Some(n) => edits.push((n.range(), new)),
                    None => {
                        // spPr follows the non-visual properties.
                        let after = el.children().find(|c| c.is_element() && c.tag_name().name().starts_with("nv")).map(|n| n.range().end);
                        if let Some(at) = after {
                            edits.push((at..at, new));
                        }
                    }
                }
            }
            if text_changed && el.tag_name().name() == "sp" {
                match (child(el, "txBody"), &shape.text) {
                    (Some(tx), Some(body)) => {
                        let mut kept = String::new();
                        for c in tx.children().filter(|c| c.is_element() && c.tag_name().name() != "p") {
                            kept.push_str(&wrapped[c.range()]);
                        }
                        let new = format!("{}>{kept}{}</p:txBody>", open_tag(&wrapped, tx), paragraphs_xml(body, &mut |l| link_rel(out, l)));
                        edits.push((tx.range(), new));
                    }
                    (None, Some(body)) => {
                        let at = child(el, "extLst").map(|e| e.range().start).unwrap_or_else(|| close_tag_start(&wrapped, el));
                        edits.push((at..at, new_tx_body_xml(body, out)));
                    }
                    _ => {}
                }
            }
        }
        "graphicFrame" => {
            if xfrm_changed && let Some(x) = child(el, "xfrm") {
                edits.push((x.range(), xfrm_xml("p:xfrm", &shape.xfrm)));
            }
            if let (ShapeKind::Table(new), ShapeKind::Table(old)) = (&shape.kind, &p.kind)
                && new != old
                && let Some(tbl) = el.descendants().find(|n| n.is_element() && n.tag_name().name() == "tbl")
            {
                edits.push((tbl.range(), patch_table(&wrapped, tbl, old, new, &mut |l| link_rel(out, l))));
            }
        }
        _ => {}
    }
    let result = apply_edits(&wrapped, edits);
    result[offset..result.len() - "</w>".len()].to_string()
}

fn close_tag_start(src: &str, n: Node) -> usize {
    let r = n.range();
    src[r.clone()].rfind("</").map(|i| r.start + i).unwrap_or(r.end)
}

/// Schema order of `spPr` children, used to insert new elements in place.
fn sp_pr_rank(name: &str) -> u8 {
    match name {
        "xfrm" => 0,
        "custGeom" | "prstGeom" => 1,
        "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "grpFill" => 2,
        "ln" => 3,
        "effectLst" | "effectDag" => 4,
        "scene3d" => 5,
        "sp3d" => 6,
        _ => 7,
    }
}

fn rebuild_sp_pr(src: &str, sp_pr: Option<Node>, shape: &Shape, xfrm: bool, fill: bool, line: bool) -> String {
    let mut kids: Vec<(u8, String)> = sp_pr
        .map(|n| n.children().filter(|c| c.is_element()).map(|c| (sp_pr_rank(c.tag_name().name()), src[c.range()].to_string())).collect())
        .unwrap_or_default();
    let mut set = |rank: u8, xml: String| {
        kids.retain(|(r, _)| *r != rank);
        let at = kids.iter().position(|(r, _)| *r > rank).unwrap_or(kids.len());
        kids.insert(at, (rank, xml));
    };
    if xfrm {
        set(0, xfrm_xml("a:xfrm", &shape.xfrm));
    }
    if fill {
        set(2, fill_xml(&shape.fill));
    }
    if line {
        set(3, line_xml(shape.line));
    }
    let open = sp_pr.map(|n| open_tag(src, n)).unwrap_or_else(|| "<p:spPr".to_string());
    format!("{open}>{}</p:spPr>", kids.into_iter().map(|(_, x)| x).collect::<String>())
}

/// XML for a shape created in the editor.
fn generate_shape(shape: &Shape, out: &mut SlideOut) -> Option<String> {
    let id = out.take_id(0);
    let name = esc(&shape.name);
    let ph = shape.placeholder.as_ref().map(|ph| {
        let ty = if ph.kind == "obj" { String::new() } else { format!(r#" type="{}""#, esc(&ph.kind)) };
        let idx = if ph.idx != 0 { format!(r#" idx="{}""#, ph.idx) } else { String::new() };
        format!("<p:ph{ty}{idx}/>")
    });
    let nv_pr = ph.map(|p| format!("<p:nvPr>{p}</p:nvPr>")).unwrap_or_else(|| "<p:nvPr/>".to_string());
    match &shape.kind {
        ShapeKind::Shape => {
            let text_box = shape.placeholder.is_none() && shape.fill.is_none() && shape.line.is_none() && shape.text.is_some();
            let mut sp_pr = format!("<p:spPr>{}{}", xfrm_xml("a:xfrm", &shape.xfrm), geometry_xml(&shape.geometry));
            if shape.fill.is_some() || text_box {
                sp_pr.push_str(&fill_xml(&shape.fill));
            }
            if shape.line.is_some() {
                sp_pr.push_str(&line_xml(shape.line));
            }
            sp_pr.push_str("</p:spPr>");
            let c_nv_sp_pr = if text_box { r#"<p:cNvSpPr txBox="1"/>"# } else { "<p:cNvSpPr/>" };
            let tx = shape.text.as_ref().map(|t| new_tx_body_xml(t, out)).unwrap_or_default();
            Some(format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/>{c_nv_sp_pr}{nv_pr}</p:nvSpPr>{sp_pr}{tx}</p:sp>"#))
        }
        ShapeKind::Table(table) => Some(format!(
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{name}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>{}<a:graphic><a:graphicData uri="{TABLE_URI}">{}</a:graphicData></a:graphic></p:graphicFrame>"#,
            xfrm_xml("p:xfrm", &shape.xfrm),
            table_xml(table, None, &mut |l| link_rel(out, l))
        )),
        ShapeKind::Picture { media: Some(key), crop } => {
            let part = out.media.part_for(out.pres, key)?;
            let rid = out.rel_for(&format!("{REL_BASE}image"), &part, false);
            Some(format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{name}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr>{nv_pr}</p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/>{}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                src_rect_xml(crop),
                xfrm_xml("a:xfrm", &shape.xfrm)
            ))
        }
        _ => None,
    }
}

/// Emits the slide's shapes. Consecutive members of a group are written as
/// one `p:grpSp`: verbatim when the original group is untouched, else rebuilt.
fn shapes_xml(slide: &Slide, out: &mut SlideOut) -> String {
    let mut xml = String::new();
    let shapes = &slide.shapes;
    let mut i = 0;
    while i < shapes.len() {
        let run = match shapes[i].group {
            Some(g) => shapes[i..].iter().take_while(|s| s.group == Some(g)).count(),
            None => 1,
        };
        if run > 1 {
            xml.push_str(&group_xml(&shapes[i..i + run], out));
        } else {
            xml.push_str(&shape_xml(&shapes[i], out));
        }
        i += run;
    }
    xml
}

/// One shape, reusing its XML when nothing changed.
fn shape_xml(shape: &Shape, out: &mut SlideOut) -> String {
    let Some(src) = &shape.source else { return generate_shape(shape, out).unwrap_or_default() };
    // XML from inside a group has group-relative coordinates: always rewrite the frame.
    let grouped = src.container.is_some();
    // The original reference keeps an owned part (chart, diagram) unless an earlier copy took it.
    let keeps_owned = |out: &mut SlideOut| {
        src.rels.iter().filter(|(_, kind, target)| owned_part(kind) && !target.starts_with("external:")).all(|(_, _, target)| out.media.claim(out.pres, target, &out.part) == *target)
    };
    if !grouped && src.part == out.part && shape_unchanged(shape, src) && !out.used_ids.contains(&raw_id(src)) && out.media.owned_free(src, &out.part) && keeps_owned(out) {
        out.need_namespaces(&src.namespaces);
        out.used_ids.insert(raw_id(src));
        src.xml().to_string()
    } else {
        patch_shape(shape, src, grouped, out)
    }
}

/// Axis-aligned bounds (x0, y0, x1, y1) of possibly rotated shapes.
fn bounds_of(shapes: &[Shape]) -> (i64, i64, i64, i64) {
    let mut b = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for s in shapes {
        let x = &s.xfrm;
        let (w, h) = (x.w as f64, x.h as f64);
        for (px, py) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
            let (sx, sy) = x.to_slide(px, py);
            let (sx, sy) = (sx.round() as i64, sy.round() as i64);
            b = (b.0.min(sx), b.1.min(sy), b.2.max(sx), b.3.max(sy));
        }
    }
    b
}

/// When an untouched original group was only moved or resized as a whole, its
/// XML with just the outer frame (`a:off`/`a:ext`) updated, keeping nested groups.
fn moved_as_a_whole(members: &[Shape], container: &ContainerSource, out: &SlideOut) -> Option<String> {
    if members.len() != container.children || container.part != out.part {
        return None;
    }
    let srcs: Vec<&ShapeSource> = members.iter().map(|s| s.source.as_deref()).collect::<Option<_>>()?;
    let same_otherwise = members.iter().zip(&srcs).all(|(s, src)| {
        let p = &src.parsed;
        src.container.as_deref() == Some(container) && s.fill == p.fill && s.line == p.line && s.text == p.text && s.kind == p.kind && s.group == p.group
    });
    if !same_otherwise {
        return None;
    }
    let frame = |xs: &mut dyn Iterator<Item = Xfrm>| {
        let shapes: Vec<Shape> = xs.map(|x| Shape::new(ppt_model::ShapeId(0), "", x)).collect();
        let (x0, y0, x1, y1) = bounds_of(&shapes);
        Xfrm::new(x0, y0, x1 - x0, y1 - y0)
    };
    let old = frame(&mut srcs.iter().map(|s| s.parsed.xfrm));
    let new = frame(&mut members.iter().map(|s| s.xfrm));
    let (sx, sy) = (new.w as f64 / old.w.max(1) as f64, new.h as f64 / old.h.max(1) as f64);
    let map = |x: &Xfrm| Xfrm {
        x: new.x + ((x.x - old.x) as f64 * sx).round() as i64,
        y: new.y + ((x.y - old.y) as f64 * sy).round() as i64,
        w: (x.w as f64 * sx).round() as i64,
        h: (x.h as f64 * sy).round() as i64,
        ..*x
    };
    // Every member must sit where the whole-group transform puts it (within rounding).
    let close = |a: &Xfrm, b: &Xfrm| (a.x - b.x).abs() <= 2 && (a.y - b.y).abs() <= 2 && (a.w - b.w).abs() <= 2 && (a.h - b.h).abs() <= 2 && a.rot == b.rot;
    if !members.iter().zip(&srcs).all(|(s, src)| close(&s.xfrm, &map(&src.parsed.xfrm))) {
        return None;
    }
    let src = srcs[0];
    let group_xml = &src.doc[container.range.clone()];
    let (wrapped, offset) = wrap(group_xml, &src.namespaces);
    let doc = Document::parse(&wrapped).ok()?;
    let el = doc.root_element().first_element_child()?;
    let xfrm = child(el, "grpSpPr").and_then(|p| child(p, "xfrm"))?;
    let (off, ext) = (child(xfrm, "off")?, child(xfrm, "ext")?);
    let num = |n: Node, a: &str| n.attribute(a).and_then(|v| v.parse::<i64>().ok());
    let g = Xfrm::new(num(off, "x")?, num(off, "y")?, num(ext, "cx")?, num(ext, "cy")?);
    let g2 = map(&g);
    let edits = vec![
        (off.attribute_node("x")?.range_value(), g2.x.to_string()),
        (off.attribute_node("y")?.range_value(), g2.y.to_string()),
        (ext.attribute_node("cx")?.range_value(), g2.w.max(0).to_string()),
        (ext.attribute_node("cy")?.range_value(), g2.h.max(0).to_string()),
    ];
    let patched = apply_edits(&wrapped, edits);
    Some(patched[offset..patched.len() - "</w>".len()].to_string())
}

fn group_xml(members: &[Shape], out: &mut SlideOut) -> String {
    let container = members[0].source.as_ref().and_then(|s| s.container.clone());
    if let Some(container) = &container {
        let intact = members.len() == container.children
            && container.part == out.part
            && members.iter().all(|s| s.source.as_ref().is_some_and(|src| src.container.as_ref() == Some(container) && shape_unchanged(s, src)));
        let src = members[0].source.as_ref().expect("container implies a source");
        let moved = (!intact).then(|| moved_as_a_whole(members, container, out)).flatten();
        if intact || moved.is_some() {
            out.need_namespaces(&src.namespaces);
            let group = &src.doc[container.range.clone()];
            if let Ok(doc) = Document::parse(&wrap(group, &src.namespaces).0) {
                for id in doc.descendants().filter(|n| n.tag_name().name() == "cNvPr").filter_map(|n| n.attribute("id")?.parse().ok()) {
                    out.used_ids.insert(id);
                }
            }
            return moved.unwrap_or_else(|| group.to_string());
        }
    }
    // A new or changed group, in slide coordinates: the child space equals the group's frame.
    let (x0, y0, x1, y1) = bounds_of(members);
    let (w, h) = ((x1 - x0).max(0), (y1 - y0).max(0));
    let id = out.take_id(0);
    let inner: String = members.iter().map(|s| shape_xml(s, out)).collect();
    format!(
        r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="群組 {id}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="{x0}" y="{y0}"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="{x0}" y="{y0}"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></p:grpSpPr>{inner}</p:grpSp>"#
    )
}

fn raw_id(src: &ShapeSource) -> u32 {
    let xml = src.xml();
    let Some(at) = xml.find("cNvPr") else { return 0 };
    let rest = &xml[at..];
    let Some(id_at) = rest.find(" id=\"") else { return 0 };
    rest[id_at + 5..].split('"').next().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Largest shape id used by the loaded XML of `slide`, so fresh ids never collide.
fn max_raw_id(slide: &Slide) -> u32 {
    let mut max = 1;
    for s in slide.shapes.iter().filter_map(|s| s.source.as_ref()) {
        let xml = match &s.container {
            Some(c) => &s.doc[c.range.clone()],
            None => s.xml(),
        };
        let mut rest = xml;
        while let Some(at) = rest.find(" id=\"") {
            rest = &rest[at + 5..];
            if let Some(v) = rest.split('"').next().and_then(|v| v.parse::<u32>().ok()) {
                max = max.max(v);
            }
        }
    }
    max
}

/// Builds the slide XML. `template` provides everything outside the shape tree
/// (background, transitions, timing); without it a minimal slide is written.
fn slide_xml(slide: &Slide, template: Option<&SlideSource>, out: &mut SlideOut) -> String {
    out.next_id = max_raw_id(slide) + 1;
    out.used_ids.insert(1);
    let shapes = shapes_xml(slide, out);
    let Some(t) = template else {
        let extra: String = out.namespaces.iter().filter(|(p, _)| !matches!(p.as_str(), "a" | "r" | "p")).map(|(p, u)| format!(r#" xmlns:{p}="{}""#, esc(u))).collect();
        let transition = slide.transition.as_ref().map(transition_xml).unwrap_or_default();
        return format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {NS_DECLS}{extra}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}</p:sld>"#
        );
    };
    let doc_text = &t.doc;
    let Ok(doc) = Document::parse(doc_text) else { return doc_text.to_string() };
    let root = doc.root_element();
    let Some(tree) = child(root, "cSld").and_then(|c| child(c, "spTree")) else { return doc_text.to_string() };
    let props_end = tree
        .children()
        .filter(|c| c.is_element() && matches!(c.tag_name().name(), "nvGrpSpPr" | "grpSpPr"))
        .map(|c| c.range().end)
        .max()
        .unwrap_or_else(|| tree.range().start + open_tag(doc_text, tree).len() + 1);
    let close = close_tag_start(doc_text, tree);

    let mut edits = vec![(props_end..close, shapes)];
    if slide.transition != t.transition {
        edits.push(transition_edit(doc_text, root, slide.transition.as_ref()));
    }
    // Animations that point at shapes we no longer write would make PowerPoint repair the file.
    if let Some(timing) = child(root, "timing") {
        let dangling = timing.descendants().filter_map(|n| n.attribute("spid")).filter_map(|v| v.parse::<u32>().ok()).any(|id| !out.used_ids.contains(&id));
        if dangling {
            edits.push((timing.range(), String::new()));
        }
    }
    // Declare namespaces used by shapes pasted from other parts.
    let declared: HashSet<String> = root.namespaces().map(|n| n.name().unwrap_or_default().to_string()).collect();
    let extra: String = out.namespaces.iter().filter(|(p, _)| !declared.contains(p)).map(|(p, u)| format!(r#" xmlns:{p}="{}""#, esc(u))).collect();
    if !extra.is_empty() {
        let at = root.range().start + open_tag(doc_text, root).len();
        edits.push((at..at, extra));
    }
    apply_edits(doc_text, edits)
}

fn slide_unchanged(slide: &Slide, src: &SlideSource) -> bool {
    slide.transition == src.transition
        && slide.shapes.len() == src.shape_ids.len()
        && slide.shapes.iter().zip(&src.shape_ids).all(|(s, id)| s.id == *id)
        && slide.shapes.iter().all(|s| s.source.as_ref().is_some_and(|ss| ss.part == src.part && shape_unchanged(s, ss)))
}

// --- Package ----------------------------------------------------------------

struct SlidePlan {
    part: String,
    sld_id: u32,
    rel_id: String,
    /// New XML and rels, or `None` to copy the original parts.
    content: Option<(String, String)>,
}

/// A relationship id not used in `rels`.
fn free_rel_id(rels: &[RelEntry]) -> String {
    let mut n = rels.len() + 1;
    while rels.iter().any(|r| r.id == format!("rId{n}")) {
        n += 1;
    }
    format!("rId{n}")
}

/// The first `{prefix}{n}.xml` not in `taken`, which is updated.
fn free_part(taken: &mut HashSet<String>, prefix: &str) -> String {
    let mut n = 1;
    loop {
        let part = format!("{prefix}{n}.xml");
        if taken.insert(part.clone()) {
            return part;
        }
        n += 1;
    }
}

/// Notes parts written by a save: new notes slides (and a notes master when
/// the deck has none), and rewritten existing notes slides.
#[derive(Default)]
struct NotesOut {
    master: Option<String>,
    /// Relationship id of a notes master created by this save.
    new_master_rel: Option<String>,
    /// New parts with their content types (rels parts have none).
    new_parts: Vec<(String, String, Option<&'static str>)>,
    replaced: Vec<(String, String)>,
}

impl NotesOut {
    /// The notes master, created (with a copy of the deck theme) when missing.
    fn master(&mut self, zip: &mut ZipArchive<Cursor<&[u8]>>, pres: &Presentation, pres_xml: &str, pres_rels: &mut Vec<RelEntry>, taken: &mut HashSet<String>) -> String {
        if let Some(m) = &self.master {
            return m.clone();
        }
        let part = free_part(taken, "ppt/notesMasters/notesMaster");
        let notes_size = Document::parse(pres_xml).ok().and_then(|d| {
            let n = child(d.root_element(), "notesSz")?;
            Some((n.attribute("cx")?.parse::<i64>().ok()?, n.attribute("cy")?.parse::<i64>().ok()?))
        });
        let (nw, nh) = notes_size.unwrap_or((6_858_000, 9_144_000));
        let xml = notes_master_xml(pres.slide_width, pres.slide_height, nw, nh);
        let mut rels = Vec::new();
        let theme_src = pres_rels.iter().find(|r| r.kind.ends_with("/theme")).map(|r| r.target.clone());
        if let Some(theme_xml) = theme_src.and_then(|t| read_part(zip, &t)) {
            let theme = free_part(taken, "ppt/theme/theme");
            rels.push(RelEntry { id: "rId1".into(), kind: format!("{REL_BASE}theme"), target: theme.clone(), external: false });
            self.new_parts.push((theme, theme_xml, Some(CT_THEME)));
        }
        self.new_parts.push((rels_part_of(&part), rels_xml(&rels, &part), None));
        self.new_parts.push((part.clone(), xml, Some(CT_NOTES_MASTER)));
        let id = free_rel_id(pres_rels);
        pres_rels.push(RelEntry { id: id.clone(), kind: format!("{REL_BASE}notesMaster"), target: part.clone(), external: false });
        self.new_master_rel = Some(id);
        self.master = Some(part.clone());
        part
    }
}

fn read_part(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut f = zip.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// Saves `pres` as a `.pptx` package.
pub fn save_pptx(pres: &Presentation) -> Result<Vec<u8>, Error> {
    let package = pres.package.as_ref().ok_or_else(|| Error::Missing("source package".into()))?;
    let mut zip = ZipArchive::new(Cursor::new(package.as_slice())).map_err(|e| Error::Zip(e.to_string()))?;

    let root_rels = parse_rels(&read_part(&mut zip, "_rels/.rels").unwrap_or_default(), "");
    let pres_part = root_rels.iter().find(|r| r.kind.ends_with("/officeDocument")).map(|r| r.target.clone()).unwrap_or_else(|| "ppt/presentation.xml".into());
    let pres_xml = read_part(&mut zip, &pres_part).ok_or_else(|| Error::Missing(pres_part.clone()))?;
    let pres_rels_part = rels_part_of(&pres_part);
    let mut pres_rels = parse_rels(&read_part(&mut zip, &pres_rels_part).unwrap_or_default(), &pres_part);
    let content_types = read_part(&mut zip, "[Content_Types].xml").ok_or_else(|| Error::Missing("[Content_Types].xml".into()))?;

    // Original slides: part → (sldId, rel id).
    let pres_doc = Document::parse(&pres_xml).map_err(|e| Error::Xml(e.to_string()))?;
    let sld_id_lst = child(pres_doc.root_element(), "sldIdLst");
    let mut originals: HashMap<String, (u32, String)> = HashMap::new();
    if let Some(lst) = sld_id_lst {
        for n in lst.children().filter(|n| n.tag_name().name() == "sldId") {
            let (Some(id), Some(rid)) = (n.attribute("id").and_then(|v| v.parse().ok()), n.attribute((REL_NS, "id"))) else { continue };
            if let Some(rel) = pres_rels.iter().find(|r| r.id == rid) {
                originals.insert(rel.target.clone(), (id, rid.to_string()));
            }
        }
    }
    let all_names: Vec<String> = zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect();
    let mut taken_parts: HashSet<String> = all_names.iter().cloned().collect();
    let mut next_sld_id = originals.values().map(|v| v.0).max().unwrap_or(255).max(255) + 1;
    let mut next_slide_no = all_names.iter().filter_map(|n| n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?.parse::<u32>().ok()).max().unwrap_or(0) + 1;

    let mut media = MediaOut {
        existing: all_names.iter().filter(|n| n.starts_with("ppt/media/")).cloned().collect(),
        taken: taken_parts.clone(),
        content_types: Document::parse(&content_types)
            .map(|d| {
                d.root_element()
                    .children()
                    .filter(|n| n.tag_name().name() == "Override")
                    .filter_map(|n| Some((n.attribute("PartName")?.trim_start_matches('/').to_string(), n.attribute("ContentType")?.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    };
    // Owned parts (charts, diagrams) stay with the original slides that keep them;
    // other references to them get copies.
    let mut kept: HashSet<&str> = HashSet::new();
    for slide in &pres.slides {
        if let Some(src) = slide.source.as_ref().filter(|s| originals.contains_key(&s.part))
            && kept.insert(src.part.as_str())
        {
            for r in parse_rels(&read_part(&mut zip, &rels_part_of(&src.part)).unwrap_or_default(), &src.part) {
                if !r.external && owned_part(&r.kind) {
                    media.owned.insert(r.target, (src.part.clone(), false));
                }
            }
        }
    }
    let mut claimed: HashSet<String> = HashSet::new();
    let mut plans: Vec<SlidePlan> = Vec::new();
    let mut notes = NotesOut { master: pres_rels.iter().find(|r| r.kind.ends_with("/notesMaster")).map(|r| r.target.clone()), ..Default::default() };
    // Adds a notes slide for `slide_part` to `rels` when the slide has notes.
    let add_notes = |zip: &mut ZipArchive<Cursor<&[u8]>>, pres_rels: &mut Vec<RelEntry>, taken: &mut HashSet<String>, notes: &mut NotesOut, slide: &Slide, slide_part: &str, rels: &mut Vec<RelEntry>| {
        if slide.notes.trim().is_empty() {
            return false;
        }
        let master = notes.master(zip, pres, &pres_xml, pres_rels, taken);
        // A duplicated slide starts from its original's notes slide (same text format),
        // unless that part references other parts we would not carry over.
        let template = slide.source.as_ref().and_then(|src| {
            let rels = parse_rels(&read_part(zip, &rels_part_of(&src.part))?, &src.part);
            let xml = read_part(zip, &rels.iter().find(|r| r.kind.ends_with("/notesSlide"))?.target)?;
            (!xml.contains(" r:embed=") && !xml.contains(" r:id=") && !xml.contains(" r:link=")).then_some(xml)
        });
        let xml = template.map_or_else(|| notes_slide_xml(&slide.notes), |t| with_notes_text(&t, &slide.notes));
        let part = free_part(taken, "ppt/notesSlides/notesSlide");
        let notes_rels = [
            RelEntry { id: "rId1".into(), kind: format!("{REL_BASE}notesMaster"), target: master, external: false },
            RelEntry { id: "rId2".into(), kind: format!("{REL_BASE}slide"), target: slide_part.to_string(), external: false },
        ];
        notes.new_parts.push((rels_part_of(&part), rels_xml(&notes_rels, &part), None));
        notes.new_parts.push((part.clone(), xml, Some(CT_NOTES_SLIDE)));
        rels.push(RelEntry { id: free_rel_id(rels), kind: format!("{REL_BASE}notesSlide"), target: part, external: false });
        true
    };
    for slide in &pres.slides {
        let original = slide.source.as_ref().filter(|s| originals.contains_key(&s.part) && !claimed.contains(&s.part));
        if let Some(src) = original {
            claimed.insert(src.part.clone());
            let (sld_id, rel_id) = originals[&src.part].clone();
            let rels_part = rels_part_of(&src.part);
            let mut rels = parse_rels(&read_part(&mut zip, &rels_part).unwrap_or_default(), &src.part);
            let mut rels_changed = false;
            match rels.iter().find(|r| r.kind.ends_with("/notesSlide")).map(|r| r.target.clone()) {
                Some(notes_part) => {
                    if let Some(xml) = read_part(&mut zip, &notes_part)
                        && notes_text(&xml) != slide.notes
                    {
                        notes.replaced.push((notes_part, with_notes_text(&xml, &slide.notes)));
                    }
                }
                None => rels_changed = add_notes(&mut zip, &mut pres_rels, &mut taken_parts, &mut notes, slide, &src.part, &mut rels),
            }
            let content = if slide_unchanged(slide, src) {
                // Only the relationships change when a notes slide was added.
                rels_changed.then(|| (src.doc.to_string(), rels_xml(&rels, &src.part)))
            } else {
                let mut out = SlideOut { part: src.part.clone(), rels, used_ids: HashSet::new(), next_id: 2, pres, media: &mut media, namespaces: Vec::new() };
                let xml = slide_xml(slide, Some(src), &mut out);
                Some((xml, rels_xml(&out.rels, &src.part)))
            };
            plans.push(SlidePlan { part: src.part.clone(), sld_id, rel_id, content });
            continue;
        }
        // New slide (added, duplicated or from the template).
        let mut part;
        loop {
            part = format!("ppt/slides/slide{next_slide_no}.xml");
            next_slide_no += 1;
            if !taken_parts.contains(&part) {
                break;
            }
        }
        taken_parts.insert(part.clone());
        let layout_part = slide.layout.and_then(|i| pres.layouts.get(i)).map(|l| l.part.clone()).filter(|p| !p.is_empty()).or_else(|| pres.layouts.first().map(|l| l.part.clone()));
        let mut rels = Vec::new();
        if let Some(lp) = layout_part {
            rels.push(RelEntry { id: "rId1".into(), kind: format!("{REL_BASE}slideLayout"), target: lp, external: false });
        }
        let mut out = SlideOut { part: part.clone(), rels, used_ids: HashSet::new(), next_id: 2, pres, media: &mut media, namespaces: Vec::new() };
        let xml = slide_xml(slide, slide.source.as_deref(), &mut out);
        let mut slide_rels = out.rels;
        add_notes(&mut zip, &mut pres_rels, &mut taken_parts, &mut notes, slide, &part, &mut slide_rels);
        let rels = rels_xml(&slide_rels, &part);
        let rel_id = free_rel_id(&pres_rels);
        pres_rels.push(RelEntry { id: rel_id.clone(), kind: format!("{REL_BASE}slide"), target: part.clone(), external: false });
        plans.push(SlidePlan { part, sld_id: next_sld_id, rel_id, content: Some((xml, rels)) });
        next_sld_id += 1;
    }

    // Table styles used by new or edited tables must be defined in the package.
    let mut needed: Vec<std::sync::Arc<ppt_model::table::TableStyle>> = Vec::new();
    for shape in pres.slides.iter().flat_map(|s| &s.shapes) {
        let ShapeKind::Table(t) = &shape.kind else { continue };
        let edited = shape.source.as_ref().is_none_or(|src| src.parsed.kind != shape.kind);
        if let Some(style) = t.style.as_ref().filter(|_| edited)
            && !needed.iter().any(|n| n.id == style.id)
        {
            needed.push(style.clone());
        }
    }
    let styles_part = pres_rels.iter().find(|r| r.kind.ends_with("/tableStyles")).map(|r| r.target.clone());
    let styles_xml = styles_part.as_ref().and_then(|p| read_part(&mut zip, p));
    let defined: HashSet<String> = styles_xml
        .as_deref()
        .and_then(|x| Document::parse(x).ok().map(|d| d.root_element().children().filter_map(|n| n.attribute("styleId").map(str::to_string)).collect()))
        .unwrap_or_default();
    needed.retain(|s| !defined.contains(&s.id));
    let mut styles_update: Option<(String, String)> = None;
    if !needed.is_empty() {
        let added: String = needed.iter().map(|s| table_style_xml(s)).collect();
        let (part, xml) = match (&styles_part, &styles_xml) {
            (Some(part), Some(xml)) => {
                let doc = Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))?;
                let root = doc.root_element();
                let r = root.range();
                let new_root = if xml[r.clone()].ends_with("/>") {
                    let open = open_tag(xml, root);
                    let tag = open[1..].split_whitespace().next().unwrap_or("a:tblStyleLst").to_string();
                    format!("{open}>{added}</{tag}>")
                } else {
                    let at = close_tag_start(xml, root);
                    format!("{}{added}{}", &xml[r.start..at], &xml[at..r.end])
                };
                (part.clone(), apply_edits(xml, vec![(r, new_root)]))
            }
            _ => {
                let def = pres.table_style.as_ref().map(|s| s.id.clone()).unwrap_or_else(|| needed[0].id.clone());
                let xml = format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" def="{}">{added}</a:tblStyleLst>"#,
                    esc(&def)
                );
                let part = "ppt/tableStyles.xml".to_string();
                let mut n = pres_rels.len() + 1;
                while pres_rels.iter().any(|r| r.id == format!("rId{n}")) {
                    n += 1;
                }
                pres_rels.push(RelEntry { id: format!("rId{n}"), kind: format!("{REL_BASE}tableStyles"), target: part.clone(), external: false });
                (part, xml)
            }
        };
        styles_update = Some((part, xml));
    }

    // Parts of deleted slides (and their notes) are dropped.
    let mut dropped: HashSet<String> = HashSet::new();
    for part in originals.keys().filter(|p| !claimed.contains(*p)) {
        dropped.insert(part.clone());
        let rels_part = rels_part_of(part);
        for r in parse_rels(&read_part(&mut zip, &rels_part).unwrap_or_default(), part) {
            if r.kind.ends_with("/notesSlide") {
                dropped.insert(rels_part_of(&r.target));
                dropped.insert(r.target);
            }
        }
        dropped.insert(rels_part);
    }
    pres_rels.retain(|r| !dropped.contains(&r.target));

    // presentation.xml: new slide list.
    let list: String = plans.iter().map(|p| format!(r#"<p:sldId id="{}" r:id="{}"/>"#, p.sld_id, esc(&p.rel_id))).collect();
    let root = pres_doc.root_element();
    let mut pres_edits = Vec::new();
    // A notes master created by this save is listed right after the slide masters.
    let masters_end = child(root, "sldMasterIdLst").map(|n| n.range().end);
    let mut notes_master_lst = notes
        .new_master_rel
        .as_ref()
        .filter(|_| child(root, "notesMasterIdLst").is_none())
        .map(|id| format!(r#"<p:notesMasterIdLst><p:notesMasterId r:id="{}"/></p:notesMasterIdLst>"#, esc(id)));
    match sld_id_lst {
        Some(lst) => pres_edits.push((lst.range(), format!("<p:sldIdLst>{list}</p:sldIdLst>"))),
        None => {
            let after = ["notesMasterIdLst", "handoutMasterIdLst", "sldMasterIdLst"].iter().find_map(|n| child(root, n)).map(|n| n.range().end);
            let at = after.unwrap_or(root.range().start + open_tag(&pres_xml, root).len() + 1);
            // Both lists go to the same place: keep their schema order.
            let lead = if after == masters_end { notes_master_lst.take().unwrap_or_default() } else { String::new() };
            pres_edits.push((at..at, format!("{lead}<p:sldIdLst>{list}</p:sldIdLst>")));
        }
    }
    if let (Some(lst), Some(at)) = (notes_master_lst, masters_end) {
        pres_edits.push((at..at, lst));
    }
    let new_pres_xml = apply_edits(&pres_xml, pres_edits);

    // [Content_Types].xml
    let ct_doc = Document::parse(&content_types).map_err(|e| Error::Xml(e.to_string()))?;
    let mut defaults: Vec<(String, String)> = Vec::new();
    let mut overrides: Vec<(String, String)> = Vec::new();
    for n in ct_doc.root_element().children().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "Default" => defaults.push((n.attribute("Extension").unwrap_or_default().to_string(), n.attribute("ContentType").unwrap_or_default().to_string())),
            "Override" => overrides.push((n.attribute("PartName").unwrap_or_default().to_string(), n.attribute("ContentType").unwrap_or_default().to_string())),
            _ => {}
        }
    }
    overrides.retain(|(p, _)| !dropped.contains(p.trim_start_matches('/')));
    for plan in &plans {
        let name = format!("/{}", plan.part);
        if !overrides.iter().any(|(p, _)| *p == name) {
            overrides.push((name, CT_SLIDE.to_string()));
        }
    }
    for (part, ct) in &media.new_overrides {
        overrides.push((format!("/{part}"), ct.clone()));
    }
    for (part, _, ct) in &notes.new_parts {
        if let Some(ct) = ct {
            overrides.push((format!("/{part}"), ct.to_string()));
        }
    }
    if let Some((part, _)) = &styles_update {
        let name = format!("/{part}");
        if !overrides.iter().any(|(p, _)| *p == name) {
            overrides.push((name, "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml".to_string()));
        }
    }
    for (part, _) in &media.new_files {
        let ext = part.rsplit('.').next().unwrap_or("png").to_string();
        if !defaults.iter().any(|(e, _)| e.eq_ignore_ascii_case(&ext)) {
            let ct = match ext.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                _ => "image/png",
            };
            defaults.push((ext, ct.to_string()));
        }
    }
    let mut ct = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#);
    for (e, t) in &defaults {
        ct.push_str(&format!(r#"<Default Extension="{}" ContentType="{}"/>"#, esc(e), esc(t)));
    }
    for (p, t) in &overrides {
        ct.push_str(&format!(r#"<Override PartName="{}" ContentType="{}"/>"#, esc(p), esc(t)));
    }
    ct.push_str("</Types>");

    // Write the package.
    let mut replaced: HashMap<String, Vec<u8>> = HashMap::new();
    replaced.insert("[Content_Types].xml".into(), ct.into_bytes());
    replaced.insert(pres_part.clone(), new_pres_xml.into_bytes());
    replaced.insert(pres_rels_part.clone(), rels_xml(&pres_rels, &pres_part).into_bytes());
    let mut new_parts: Vec<(String, Vec<u8>)> = Vec::new();
    for plan in &plans {
        if let Some((xml, rels)) = &plan.content {
            let rels_part = rels_part_of(&plan.part);
            if all_names.contains(&plan.part) {
                replaced.insert(plan.part.clone(), xml.clone().into_bytes());
                if all_names.contains(&rels_part) {
                    replaced.insert(rels_part, rels.clone().into_bytes());
                } else {
                    new_parts.push((rels_part, rels.clone().into_bytes()));
                }
            } else {
                new_parts.push((plan.part.clone(), xml.clone().into_bytes()));
                new_parts.push((rels_part, rels.clone().into_bytes()));
            }
        }
    }
    if let Some((part, xml)) = styles_update {
        if all_names.contains(&part) {
            replaced.insert(part, xml.into_bytes());
        } else {
            new_parts.push((part, xml.into_bytes()));
        }
    }
    for (part, xml) in notes.replaced {
        replaced.insert(part, xml.into_bytes());
    }
    new_parts.extend(notes.new_parts.into_iter().map(|(part, xml, _)| (part, xml.into_bytes())));
    new_parts.extend(media.new_files);

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for i in 0..zip.len() {
        let name = zip.by_index_raw(i).map_err(|e| Error::Zip(e.to_string()))?.name().map_err(|e| Error::Zip(e.to_string()))?.into_owned();
        if dropped.contains(&name) {
            continue;
        }
        if let Some(bytes) = replaced.remove(&name) {
            writer.start_file(name, opts).map_err(|e| Error::Zip(e.to_string()))?;
            writer.write_all(&bytes).map_err(|e| Error::Zip(e.to_string()))?;
        } else {
            let file = zip.by_index_raw(i).map_err(|e| Error::Zip(e.to_string()))?;
            writer.raw_copy_file(file).map_err(|e| Error::Zip(e.to_string()))?;
        }
    }
    for (name, bytes) in replaced.into_iter().chain(new_parts) {
        writer.start_file(name, opts).map_err(|e| Error::Zip(e.to_string()))?;
        writer.write_all(&bytes).map_err(|e| Error::Zip(e.to_string()))?;
    }
    Ok(writer.finish().map_err(|e| Error::Zip(e.to_string()))?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_targets() {
        assert_eq!(relative_target("ppt/slides/slide1.xml", "ppt/slideLayouts/slideLayout1.xml"), "../slideLayouts/slideLayout1.xml");
        assert_eq!(relative_target("ppt/presentation.xml", "ppt/slides/slide2.xml"), "slides/slide2.xml");
        assert_eq!(relative_target("ppt/slides/slide1.xml", "ppt/media/a.png"), "../media/a.png");
    }

    #[test]
    fn edits_apply_back_to_front() {
        assert_eq!(apply_edits("abcdef", vec![(1..2, "X".into()), (4..6, "YZW".into())]), "aXcdYZW");
    }
}
