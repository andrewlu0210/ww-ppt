//! Deck-wide design changes made on the package: theme color and font
//! variants, and the slide size. The deck is saved, the parts are rewritten
//! and the result is parsed again, so everything resolved from the theme or
//! inherited from layouts and masters picks up the change.

use std::io::{Cursor, Read, Write};
use std::ops::Range;

use ppt_model::{Color, Emu, Presentation};
use roxmltree::{Document, Node};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::package::Package;
use crate::style::Theme;
use crate::writer::{apply_edits, esc};
use crate::{Error, parse_pptx_owned, save_pptx};

/// A built-in theme color variant: `dk2`, `lt2`, `accent1`..`accent6`,
/// `hlink`, `folHlink` (`dk1`/`lt1` stay the system text and window colors).
pub struct ColorScheme {
    pub name: &'static str,
    /// Display names in Traditional and Simplified Chinese; English uses `name`.
    labels: [&'static str; 2],
    pub colors: [u32; 10],
}

impl ColorScheme {
    /// The name shown in the interface language, as PowerPoint shows it.
    pub fn label(&self) -> &'static str {
        ppt_model::lang().pick(self.labels[0], self.labels[1], self.name)
    }
}

/// A built-in theme font variant (Latin heading and body fonts; East Asian
/// and script fonts of the theme are kept).
pub struct FontScheme {
    pub name: &'static str,
    pub major: &'static str,
    pub minor: &'static str,
}

const fn scheme(name: &'static str, labels: [&'static str; 2], colors: [u32; 10]) -> ColorScheme {
    ColorScheme { name, labels, colors }
}

pub const COLOR_SCHEMES: &[ColorScheme] = &[
    scheme("Office", ["Office", "Office"], [0x0E2841, 0xE8E8E8, 0x156082, 0xE97132, 0x196B24, 0x0F9ED5, 0xA02B93, 0x4EA72E, 0x467886, 0x96607D]),
    scheme("Office 2013 - 2022", ["Office 2013 - 2022", "Office 2013 - 2022"], [0x44546A, 0xE7E6E6, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47, 0x0563C1, 0x954F72]),
    scheme("Office 2007 - 2010", ["Office 2007 - 2010", "Office 2007 - 2010"], [0x1F497D, 0xEEECE1, 0x4F81BD, 0xC0504D, 0x9BBB59, 0x8064A2, 0x4BACC6, 0xF79646, 0x0000FF, 0x800080]),
    scheme("Grayscale", ["灰階", "灰度"], [0x000000, 0xF8F8F8, 0xDDDDDD, 0xB2B2B2, 0x969696, 0x808080, 0x5F5F5F, 0x4D4D4D, 0x5F5F5F, 0x919191]),
    scheme("Blue Warm", ["暖藍色", "蓝色暖调"], [0x242852, 0xACCBF9, 0x4A66AC, 0x629DD1, 0x297FD5, 0x7F8FA9, 0x5AA2AE, 0x9D90A0, 0x9454C3, 0x3EBBF0]),
    scheme("Blue", ["藍色", "蓝色"], [0x17406D, 0xDBEFF9, 0x0F6FC6, 0x009DD9, 0x0BD0D9, 0x10CF9B, 0x7CCA62, 0xA5C249, 0xF49100, 0x85DFD0]),
    scheme("Blue II", ["藍色 II", "蓝色 II"], [0x335B74, 0xDFE3E5, 0x1CADE4, 0x2683C6, 0x27CED7, 0x42BA97, 0x3E8853, 0x62A39F, 0x6B9F25, 0xB26B02]),
    scheme("Blue Green", ["藍綠色", "蓝绿色"], [0x373545, 0xCEDBE6, 0x3494BA, 0x58B6C0, 0x75BDA7, 0x7A8C8E, 0x84ACB6, 0x2683C6, 0x6B9F25, 0x9F6715]),
    scheme("Green", ["綠色", "绿色"], [0x455F51, 0xE3DED1, 0x549E39, 0x8AB833, 0xC0CF3A, 0x029676, 0x4AB5C4, 0x0989B1, 0x6B9F25, 0xBA6906]),
    scheme("Yellow", ["黃色", "黄色"], [0x39302A, 0xE5DEDB, 0xFFCA08, 0xF8931D, 0xCE8D3E, 0xEC7016, 0xE64823, 0x9C6A6A, 0x2998E3, 0x7F723D]),
    scheme("Orange", ["橙色", "橙色"], [0x637052, 0xCCDDEA, 0xE48312, 0xBD582C, 0x865640, 0x9B8357, 0xC2BC80, 0x94A088, 0x2998E3, 0x8C8C8C]),
    scheme("Red Orange", ["紅橙色", "红橙色"], [0x505046, 0xEEECE1, 0xE84C22, 0xFFBD47, 0xB64926, 0xFF8427, 0xCC9900, 0xB22600, 0xCC9900, 0x666699]),
    scheme("Red", ["紅色", "红色"], [0x323232, 0xE5C243, 0xA5300F, 0xD55816, 0xE19825, 0xB19C7D, 0x7F5F52, 0xB27D49, 0x6B9F25, 0xB26B02]),
    scheme("Violet", ["紫羅蘭色", "紫罗兰色"], [0x373545, 0xDCD8DC, 0xAD84C6, 0x8784C7, 0x5D739A, 0x6997AF, 0x84ACB6, 0x6F8183, 0x69A020, 0x8C8C8C]),
    scheme("Paper", ["紙張", "纸张"], [0x444D26, 0xFEFAC9, 0xA5B592, 0xF3A447, 0xE7BC29, 0xD092A7, 0x9C85C0, 0x809EC2, 0x8E58B6, 0x7F6F6F]),
];

const fn fonts(name: &'static str, major: &'static str, minor: &'static str) -> FontScheme {
    FontScheme { name, major, minor }
}

pub const FONT_SCHEMES: &[FontScheme] = &[
    fonts("Office", "Aptos Display", "Aptos"),
    fonts("Office 2013 - 2022", "Calibri Light", "Calibri"),
    fonts("Office 2007 - 2010", "Cambria", "Calibri"),
    fonts("Calibri", "Calibri", "Calibri"),
    fonts("Arial", "Arial", "Arial"),
    fonts("Corbel", "Corbel", "Corbel"),
    fonts("Candara", "Candara", "Candara"),
    fonts("Franklin Gothic", "Franklin Gothic Medium", "Franklin Gothic Book"),
    fonts("Century Gothic", "Century Gothic", "Century Gothic"),
    fonts("Tw Cen MT", "Tw Cen MT", "Tw Cen MT"),
    fonts("Cambria", "Cambria", "Cambria"),
    fonts("Garamond", "Garamond", "Garamond"),
    fonts("Georgia", "Georgia", "Georgia"),
    fonts("Gill Sans MT", "Gill Sans MT", "Gill Sans MT"),
    fonts("Times New Roman", "Times New Roman", "Times New Roman"),
    fonts("Trebuchet MS", "Trebuchet MS", "Trebuchet MS"),
    fonts("Verdana", "Verdana", "Verdana"),
];

/// Theme scheme slots in display order.
pub const SCHEME_SLOTS: [&str; 12] = ["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

/// The theme of the deck's first slide master.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeInfo {
    pub color_scheme: String,
    /// In [`SCHEME_SLOTS`] order.
    pub colors: Vec<Color>,
    pub font_scheme: String,
    pub major: String,
    pub minor: String,
}

/// Theme parts used by the slide masters.
fn master_themes(pkg: &mut Package, names: &[String]) -> Vec<String> {
    let mut themes: Vec<String> = Vec::new();
    for master in names.iter().filter(|n| is_part(n, "ppt/slideMasters/slideMaster")) {
        if let Some(t) = pkg.rel_of_type(master, "theme")
            && !themes.contains(&t)
        {
            themes.push(t);
        }
    }
    themes
}

fn is_part(name: &str, prefix: &str) -> bool {
    name.starts_with(prefix) && name.ends_with(".xml")
}

fn part_names(bytes: &[u8]) -> Result<Vec<String>, Error> {
    let zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
    let mut names: Vec<String> = zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect();
    // Masters in numeric order, so the first one is `slideMaster1`.
    names.sort_by_key(|n| (n.len(), n.clone()));
    Ok(names)
}

pub fn theme_info(pres: &Presentation) -> Option<ThemeInfo> {
    let bytes = pres.package.as_ref()?;
    let names = part_names(bytes).ok()?;
    let mut pkg = Package::open(bytes).ok()?;
    let part = master_themes(&mut pkg, &names).into_iter().next()?;
    let xml = pkg.read_string(&part).ok()?;
    let theme = Theme::parse(&xml);
    let doc = Document::parse(&xml).ok()?;
    let elements = crate::style::path(doc.root_element(), &["themeElements"])?;
    let name_of = |tag: &str| crate::style::child(elements, tag).and_then(|n| n.attribute("name")).unwrap_or_default().to_string();
    Some(ThemeInfo {
        color_scheme: name_of("clrScheme"),
        colors: SCHEME_SLOTS.iter().map(|s| theme.colors.get(*s).copied().unwrap_or(Color::WHITE)).collect(),
        font_scheme: name_of("fontScheme"),
        major: theme.major.0,
        minor: theme.minor.0,
    })
}

/// Saves `pres`, passes each XML part through `f` (which returns its new text,
/// or `None` to keep it) and parses the result.
fn rewrite(pres: &Presentation, mut f: impl FnMut(&str, &str) -> Option<String>) -> Result<Presentation, Error> {
    let bytes = save_pptx(pres)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes.as_slice())).map_err(|e| Error::Zip(e.to_string()))?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for i in 0..zip.len() {
        let name = zip.by_index_raw(i).map_err(|e| Error::Zip(e.to_string()))?.name().map_err(|e| Error::Zip(e.to_string()))?.into_owned();
        let changed = if name.ends_with(".xml") {
            let mut xml = String::new();
            zip.by_index(i).map_err(|e| Error::Zip(e.to_string()))?.read_to_string(&mut xml).map_err(|e| Error::Zip(e.to_string()))?;
            f(&name, &xml)
        } else {
            None
        };
        match changed {
            Some(xml) => {
                writer.start_file(name, opts).map_err(|e| Error::Zip(e.to_string()))?;
                writer.write_all(xml.as_bytes()).map_err(|e| Error::Zip(e.to_string()))?;
            }
            None => writer.raw_copy_file(zip.by_index_raw(i).map_err(|e| Error::Zip(e.to_string()))?).map_err(|e| Error::Zip(e.to_string()))?,
        }
    }
    let out = writer.finish().map_err(|e| Error::Zip(e.to_string()))?.into_inner();
    parse_pptx_owned(out)
}

/// Theme parts of `pres`'s slide masters.
fn themes_of(pres: &Presentation) -> Result<Vec<String>, Error> {
    let bytes = pres.package.as_ref().ok_or_else(|| Error::Missing("source package".into()))?;
    let names = part_names(bytes)?;
    Ok(master_themes(&mut Package::open(bytes)?, &names))
}

/// The namespace prefix (with its colon) of element `n` as written in `src`.
fn prefix_of(src: &str, n: Node) -> String {
    let tag = &src[n.range().start + 1..];
    let local = n.tag_name().name();
    tag.find(local).map(|i| tag[..i].to_string()).unwrap_or_default()
}

fn attr_edit(n: Node, name: &str, value: &str) -> Option<(Range<usize>, String)> {
    n.attributes().find(|a| a.name() == name && a.namespace().is_none()).map(|a| (a.range_value(), esc(value)))
}

fn theme_edit(xml: &str, f: impl FnOnce(&str, Node) -> Vec<(Range<usize>, String)>) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    let elements = crate::style::path(doc.root_element(), &["themeElements"])?;
    let edits = f(xml, elements);
    (!edits.is_empty()).then(|| apply_edits(xml, edits))
}

/// Replaces the color scheme of the masters' themes.
pub fn set_color_scheme(pres: &Presentation, scheme: &ColorScheme) -> Result<Presentation, Error> {
    let themes = themes_of(pres)?;
    rewrite(pres, |part, xml| {
        if !themes.iter().any(|t| t == part) {
            return None;
        }
        theme_edit(xml, |src, elements| {
            let Some(old) = crate::style::child(elements, "clrScheme") else { return Vec::new() };
            let p = prefix_of(src, old);
            let mut out = format!(
                r#"<{p}clrScheme name="{}"><{p}dk1><{p}sysClr val="windowText" lastClr="000000"/></{p}dk1><{p}lt1><{p}sysClr val="window" lastClr="FFFFFF"/></{p}lt1>"#,
                esc(scheme.name)
            );
            for (slot, rgb) in SCHEME_SLOTS[2..].iter().zip(scheme.colors) {
                out.push_str(&format!(r#"<{p}{slot}><{p}srgbClr val="{rgb:06X}"/></{p}{slot}>"#));
            }
            out.push_str(&format!("</{p}clrScheme>"));
            vec![(old.range(), out)]
        })
    })
}

/// Replaces the Latin heading and body fonts of the masters' themes.
pub fn set_font_scheme(pres: &Presentation, scheme: &FontScheme) -> Result<Presentation, Error> {
    let themes = themes_of(pres)?;
    rewrite(pres, |part, xml| {
        if !themes.iter().any(|t| t == part) {
            return None;
        }
        theme_edit(xml, |_, elements| {
            let Some(fs) = crate::style::child(elements, "fontScheme") else { return Vec::new() };
            let mut edits: Vec<_> = attr_edit(fs, "name", scheme.name).into_iter().collect();
            for (tag, face) in [("majorFont", scheme.major), ("minorFont", scheme.minor)] {
                if let Some(latin) = crate::style::child(fs, tag).and_then(|f| crate::style::child(f, "latin")) {
                    edits.extend(attr_edit(latin, "typeface", face));
                }
            }
            edits
        })
    })
}

/// How content is fitted when the slide's shape changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// Content keeps as large as possible and may run off the slide.
    Maximize,
    /// Content shrinks to stay on the slide.
    EnsureFit,
}

/// Maps old slide coordinates to the new size: uniform `s` plus centering,
/// except that shapes spanning the whole slide in one direction stretch to
/// keep spanning it (backgrounds, banners).
#[derive(Clone, Copy)]
struct Resize {
    old: (f64, f64),
    new: (f64, f64),
    s: f64,
    d: (f64, f64),
}

impl Resize {
    fn new(old: (Emu, Emu), new: (Emu, Emu), fit: Fit) -> Resize {
        let (ow, oh) = (old.0.max(1) as f64, old.1.max(1) as f64);
        let (nw, nh) = (new.0 as f64, new.1 as f64);
        let (sx, sy) = (nw / ow, nh / oh);
        let s = match fit {
            Fit::Maximize => sx.max(sy),
            Fit::EnsureFit => sx.min(sy),
        };
        Resize { old: (ow, oh), new: (nw, nh), s, d: ((nw - ow * s) / 2.0, (nh - oh * s) / 2.0) }
    }

    /// New position and extent along one axis (0: x, 1: y).
    fn axis(&self, axis: usize, pos: f64, ext: f64) -> (i64, i64) {
        let (old, new, d) = if axis == 0 { (self.old.0, self.new.0, self.d.0) } else { (self.old.1, self.new.1, self.d.1) };
        let tol = old * 0.01;
        if pos <= tol && pos + ext >= old - tol {
            let k = new / old;
            return ((pos * k).round() as i64, (ext * k).round() as i64);
        }
        ((pos * self.s + d).round() as i64, (ext * self.s).round() as i64)
    }

    fn scaled(&self, v: f64) -> i64 {
        (v * self.s).round() as i64
    }
}

/// Whether `n` is a shape directly in the slide's shape tree (possibly
/// wrapped in `mc:AlternateContent`).
fn top_level(n: Node) -> bool {
    match n.parent_element() {
        Some(p) if p.tag_name().name() == "spTree" => true,
        Some(p) if matches!(p.tag_name().name(), "Choice" | "Fallback") => p.parent_element().and_then(|a| a.parent_element()).is_some_and(|t| t.tag_name().name() == "spTree"),
        _ => false,
    }
}

/// The shape owning `a:xfrm`/`p:xfrm` element `x`.
fn xfrm_owner<'a, 'i>(x: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    let p = x.parent_element()?;
    match p.tag_name().name() {
        "graphicFrame" => Some(p),
        "spPr" | "grpSpPr" => p.parent_element(),
        _ => None,
    }
}

fn num(n: Node, name: &str) -> Option<f64> {
    n.attribute(name)?.parse().ok()
}

fn resize_part(xml: &str, r: &Resize) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    let mut edits = Vec::new();
    let uniform = (r.s - 1.0).abs() > 1e-9;
    for n in doc.root_element().descendants().filter(|n| n.is_element()) {
        match n.tag_name().name() {
            "off" | "ext" if n.parent_element().is_some_and(|x| x.tag_name().name() == "xfrm" && xfrm_owner(x).is_some_and(top_level)) => {
                let x = n.parent_element()?;
                let (off, ext) = (crate::style::child(x, "off"), crate::style::child(x, "ext"));
                let (Some(off), Some(ext)) = (off, ext) else { continue };
                let (px, py, cx, cy) = (num(off, "x").unwrap_or(0.0), num(off, "y").unwrap_or(0.0), num(ext, "cx").unwrap_or(0.0), num(ext, "cy").unwrap_or(0.0));
                let ((nx, ncx), (ny, ncy)) = (r.axis(0, px, cx), r.axis(1, py, cy));
                let pairs: [(&str, i64); 2] = if n.tag_name().name() == "off" { [("x", nx), ("y", ny)] } else { [("cx", ncx), ("cy", ncy)] };
                for (name, v) in pairs {
                    edits.extend(attr_edit(n, name, &v.to_string()));
                }
            }
            "rPr" | "defRPr" | "endParaRPr" if uniform => {
                if let Some(sz) = num(n, "sz") {
                    // Kept to tenths of a point.
                    let v = ((sz * r.s / 10.0).round() as i64 * 10).clamp(100, 400_000);
                    edits.extend(attr_edit(n, "sz", &v.to_string()));
                }
            }
            "ln" if uniform => {
                if let Some(w) = num(n, "w") {
                    edits.extend(attr_edit(n, "w", &r.scaled(w).max(0).to_string()));
                }
            }
            "spcPts" if uniform => {
                if let Some(v) = num(n, "val") {
                    edits.extend(attr_edit(n, "val", &r.scaled(v).max(0).to_string()));
                }
            }
            // Table grids of top-level tables follow their frame.
            "gridCol" | "tr" if uniform && n.ancestors().find(|a| a.tag_name().name() == "graphicFrame").is_some_and(top_level) => {
                let name = if n.tag_name().name() == "gridCol" { "w" } else { "h" };
                if let Some(v) = num(n, name) {
                    edits.extend(attr_edit(n, name, &r.scaled(v).max(0).to_string()));
                }
            }
            _ => {}
        }
    }
    (!edits.is_empty()).then(|| apply_edits(xml, edits))
}

/// `p:sldSz` for the new size, plus the default text style scaled.
fn resize_presentation(xml: &str, size: (Emu, Emu), r: &Resize) -> Option<String> {
    let doc = Document::parse(xml).ok()?;
    let root = doc.root_element();
    let sz = crate::style::child(root, "sldSz")?;
    let p = prefix_of(xml, sz);
    let kind = match size {
        (9_144_000, 6_858_000) => r#" type="screen4x3""#,
        (9_144_000, 5_143_500) => r#" type="screen16x9""#,
        (9_144_000, 5_715_000) => r#" type="screen16x10""#,
        _ => "",
    };
    let mut edits = vec![(sz.range(), format!(r#"<{p}sldSz cx="{}" cy="{}"{kind}/>"#, size.0, size.1))];
    if (r.s - 1.0).abs() > 1e-9
        && let Some(defaults) = crate::style::child(root, "defaultTextStyle")
    {
        for n in defaults.descendants().filter(|n| n.tag_name().name() == "defRPr") {
            if let Some(v) = num(n, "sz") {
                edits.extend(attr_edit(n, "sz", &(((v * r.s / 10.0).round() as i64 * 10).clamp(100, 400_000)).to_string()));
            }
        }
    }
    Some(apply_edits(xml, edits))
}

/// Changes the slide size, scaling slides, layouts and masters.
pub fn set_slide_size(pres: &Presentation, width: Emu, height: Emu, fit: Fit) -> Result<Presentation, Error> {
    let r = Resize::new((pres.slide_width, pres.slide_height), (width, height), fit);
    let bytes = pres.package.as_ref().ok_or_else(|| Error::Missing("source package".into()))?;
    let pres_part = Package::open(bytes)?.rel_of_type("", "officeDocument").unwrap_or_else(|| "ppt/presentation.xml".to_string());
    rewrite(pres, |part, xml| {
        if part == pres_part {
            return resize_presentation(xml, (width, height), &r);
        }
        if part.ends_with("/viewProps.xml") {
            let mut view = crate::view::parse(xml);
            for g in &mut view.guides {
                let d = if g.vertical { r.d.0 } else { r.d.1 };
                g.pos = (g.pos as f64 * r.s + d).round() as Emu;
            }
            return crate::view::patch(xml, &view);
        }
        let drawn = ["ppt/slides/slide", "ppt/slideLayouts/slideLayout", "ppt/slideMasters/slideMaster"].iter().any(|p| is_part(part, p));
        if drawn { resize_part(xml, &r) } else { None }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_shapes_stretch_others_scale_and_center() {
        // 16:9 → 4:3, ensure fit: s = 0.75, content centered vertically.
        let r = Resize::new((12_192_000, 6_858_000), (9_144_000, 6_858_000), Fit::EnsureFit);
        assert!((r.s - 0.75).abs() < 1e-9);
        assert_eq!(r.axis(0, 0.0, 12_192_000.0), (0, 9_144_000));
        assert_eq!(r.axis(0, 1_000_000.0, 2_000_000.0), (750_000, 1_500_000));
        assert_eq!(r.axis(1, 1_000_000.0, 2_000_000.0), (750_000 + 857_250, 1_500_000));
    }

    #[test]
    fn resizing_scales_top_level_frames_and_fonts_only() {
        let xml = r#"<p:sld xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree><p:grpSpPr/><p:sp><p:spPr><a:xfrm><a:off x="1000" y="2000"/><a:ext cx="4000" cy="2000"/></a:xfrm></p:spPr><p:txBody><a:p><a:r><a:rPr sz="2000"/></a:r></a:p></p:txBody></p:sp><p:grpSp><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/><a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/></a:xfrm></p:grpSpPr><p:sp><p:spPr><a:xfrm><a:off x="10" y="10"/><a:ext cx="50" cy="50"/></a:xfrm></p:spPr></p:sp></p:grpSp></p:spTree></p:cSld></p:sld>"#;
        let r = Resize { old: (100_000.0, 100_000.0), new: (50_000.0, 50_000.0), s: 0.5, d: (0.0, 0.0) };
        let out = resize_part(xml, &r).unwrap();
        assert!(out.contains(r#"<a:off x="500" y="1000"/><a:ext cx="2000" cy="1000"/>"#), "{out}");
        assert!(out.contains(r#"sz="1000""#));
        // Group children keep their child coordinates; chOff/chExt are untouched.
        assert!(out.contains(r#"<a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/>"#));
        assert!(out.contains(r#"<a:off x="10" y="10"/><a:ext cx="50" cy="50"/>"#));
    }
}
