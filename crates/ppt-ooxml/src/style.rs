//! Theme, color and text-style resolution.

use std::collections::HashMap;

use ppt_model::{Align, Anchor, Bullet, BulletKind, Color, Emu};
use roxmltree::Node;

pub(crate) fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

pub(crate) fn children<'a, 'i: 'a>(n: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

pub(crate) fn path<'a, 'i>(n: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(n, |n, name| child(n, name))
}

pub(crate) fn attr_f64(n: Node, name: &str) -> Option<f64> {
    n.attribute(name)?.parse().ok()
}

pub(crate) fn attr_i64(n: Node, name: &str) -> Option<i64> {
    n.attribute(name)?.parse().ok()
}

pub(crate) fn attr_bool(n: Node, name: &str) -> Option<bool> {
    n.attribute(name).map(|v| v == "1" || v == "true")
}

#[derive(Clone, Debug, Default)]
pub struct Theme {
    pub colors: HashMap<String, Color>,
    pub major: (String, String),
    pub minor: (String, String),
    /// Widths of `a:fmtScheme/a:lnStyleLst` entries, referenced by `a:lnRef@idx`.
    pub line_widths: Vec<Emu>,
    /// `a:fillStyleLst` entries (XML, `a:` namespace declared on a wrapper),
    /// referenced by `a:fillRef@idx` 1..; `phClr` is resolved per use.
    pub fill_styles: Vec<String>,
    /// `a:bgFillStyleLst` entries, referenced by `a:bgRef@idx` 1001...
    pub bg_fill_styles: Vec<String>,
}

impl Theme {
    pub fn parse(xml: &str) -> Theme {
        let mut theme = Theme::default();
        let Ok(doc) = roxmltree::Document::parse(xml) else { return theme };
        let elements = doc.root_element();
        if let Some(scheme) = path(elements, &["themeElements", "clrScheme"]) {
            for entry in scheme.children().filter(|c| c.is_element()) {
                let color = entry.children().filter(|c| c.is_element()).find_map(|c| match c.tag_name().name() {
                    "srgbClr" => c.attribute("val").and_then(Color::from_hex),
                    "sysClr" => c.attribute("lastClr").and_then(Color::from_hex),
                    _ => None,
                });
                if let Some(color) = color {
                    theme.colors.insert(entry.tag_name().name().to_string(), color);
                }
            }
        }
        let ns = "http://schemas.openxmlformats.org/drawingml/2006/main";
        let styles = |name: &str| -> Vec<String> {
            path(elements, &["themeElements", "fmtScheme", name])
                .map(|list| list.children().filter(|c| c.is_element()).map(|c| format!(r#"<w xmlns:a="{ns}">{}</w>"#, &xml[c.range()])).collect())
                .unwrap_or_default()
        };
        theme.fill_styles = styles("fillStyleLst");
        theme.bg_fill_styles = styles("bgFillStyleLst");
        if let Some(fonts) = path(elements, &["themeElements", "fontScheme"]) {
            let read = |name: &str| {
                let Some(f) = child(fonts, name) else { return (String::new(), String::new()) };
                let latin = child(f, "latin").and_then(|n| n.attribute("typeface")).unwrap_or_default().to_string();
                let mut ea = child(f, "ea").and_then(|n| n.attribute("typeface")).unwrap_or_default().to_string();
                if ea.is_empty() {
                    // Prefer the Traditional Chinese script font, then other CJK scripts.
                    for script in ["Hant", "Hans", "Jpan"] {
                        if let Some(face) = children(f, "font").find(|n| n.attribute("script") == Some(script)) {
                            ea = face.attribute("typeface").unwrap_or_default().to_string();
                            break;
                        }
                    }
                }
                (latin, ea)
            };
            theme.major = read("majorFont");
            theme.minor = read("minorFont");
        }
        if let Some(lst) = path(elements, &["themeElements", "fmtScheme", "lnStyleLst"]) {
            theme.line_widths = children(lst, "ln").map(|n| attr_i64(n, "w").unwrap_or(9525)).collect();
        }
        theme
    }

    pub fn resolve_typeface(&self, face: &str) -> String {
        match face {
            "+mj-lt" => self.major.0.clone(),
            "+mn-lt" => self.minor.0.clone(),
            "+mj-ea" => self.major.1.clone(),
            "+mn-ea" => self.minor.1.clone(),
            other => other.to_string(),
        }
    }
}

/// Everything needed to turn a DrawingML color element into an RGB value.
#[derive(Clone, Debug)]
pub struct ColorCtx {
    pub theme: Theme,
    /// Master `p:clrMap`, e.g. `bg1 -> lt1`, `tx1 -> dk1`.
    pub clr_map: HashMap<String, String>,
}

impl ColorCtx {
    pub fn scheme(&self, name: &str) -> Option<Color> {
        let mapped = self.clr_map.get(name).map(String::as_str).unwrap_or(match name {
            "bg1" => "lt1",
            "tx1" => "dk1",
            "bg2" => "lt2",
            "tx2" => "dk2",
            other => other,
        });
        self.theme.colors.get(mapped).copied()
    }

    /// Reads the first color element (`a:srgbClr`, `a:schemeClr`, ...) under `n`.
    /// `ph` substitutes `a:schemeClr val="phClr"` inside style references.
    pub fn color_in(&self, n: Node, ph: Option<Color>) -> Option<Color> {
        n.children().filter(|c| c.is_element()).find_map(|c| self.color_elem(c, ph))
    }

    fn color_elem(&self, c: Node, ph: Option<Color>) -> Option<Color> {
        let base = match c.tag_name().name() {
            "srgbClr" => c.attribute("val").and_then(Color::from_hex),
            "sysClr" => c.attribute("lastClr").and_then(Color::from_hex),
            "schemeClr" => match c.attribute("val")? {
                "phClr" => ph,
                v => self.scheme(v),
            },
            "prstClr" => match c.attribute("val")? {
                "black" => Some(Color::BLACK),
                "white" => Some(Color::WHITE),
                "red" => Some(Color::rgb(255, 0, 0)),
                "green" => Some(Color::rgb(0, 128, 0)),
                "blue" => Some(Color::rgb(0, 0, 255)),
                "yellow" => Some(Color::rgb(255, 255, 0)),
                "gray" => Some(Color::rgb(128, 128, 128)),
                _ => None,
            },
            _ => return None,
        }?;
        Some(apply_mods(base, c))
    }
}

/// Applies `a:lumMod`, `a:lumOff`, `a:tint`, `a:shade` and `a:alpha`.
fn apply_mods(color: Color, n: Node) -> Color {
    let (mut h, mut s, mut l) = rgb_to_hsl(color);
    let mut rgb = color;
    let mut alpha = color.a;
    let mut hsl_dirty = false;
    for m in n.children().filter(|c| c.is_element()) {
        let v = attr_f64(m, "val").unwrap_or(0.0) / 100_000.0;
        match m.tag_name().name() {
            "lumMod" => {
                l *= v;
                hsl_dirty = true;
            }
            "lumOff" => {
                l += v;
                hsl_dirty = true;
            }
            "satMod" => {
                s *= v;
                hsl_dirty = true;
            }
            "tint" | "shade" => {
                if hsl_dirty {
                    rgb = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
                    hsl_dirty = false;
                }
                let f = |c: u8| {
                    let c = c as f64 / 255.0;
                    let out = if m.tag_name().name() == "tint" { 1.0 - (1.0 - c) * v } else { c * v };
                    (out.clamp(0.0, 1.0) * 255.0).round() as u8
                };
                rgb = Color { r: f(rgb.r), g: f(rgb.g), b: f(rgb.b), a: rgb.a };
                (h, s, l) = rgb_to_hsl(rgb);
            }
            "alpha" => alpha = (v.clamp(0.0, 1.0) * 255.0).round() as u8,
            _ => {}
        }
    }
    if hsl_dirty {
        rgb = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    }
    Color { a: alpha, ..rgb }
}

fn rgb_to_hsl(c: Color) -> (f64, f64, f64) {
    let (r, g, b) = (c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-12 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Color {
    let to = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    if s == 0.0 {
        return Color::rgb(to(l), to(l), to(l));
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    Color::rgb(to(hue(h + 1.0 / 3.0)), to(hue(h)), to(hue(h - 1.0 / 3.0)))
}

/// Partially specified paragraph/run properties for one list level.
/// Resolved by layering: slide → layout → master → defaults.
#[derive(Clone, Debug, Default)]
pub struct LevelStyle {
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub baseline: Option<f32>,
    pub spacing: Option<f32>,
    pub color: Option<Color>,
    pub latin: Option<String>,
    pub ea: Option<String>,
    pub align: Option<Align>,
    pub line_spacing: Option<f32>,
    /// Bullet parts inherit independently; `Some(None)` is an explicit
    /// `a:buNone` / `a:buFontTx` / `a:buClrTx` / `a:buSzTx`.
    pub bu_kind: Option<Option<BulletKind>>,
    pub bu_font: Option<Option<String>>,
    pub bu_color: Option<Option<Color>>,
    pub bu_size: Option<Option<f32>>,
    pub mar_l: Option<Emu>,
    pub indent: Option<Emu>,
    pub space_before: Option<Spacing>,
    pub space_after: Option<Spacing>,
}

/// `a:spcBef` / `a:spcAft`: absolute points or a percentage of the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spacing {
    Points(f32),
    Percent(f32),
}

impl Spacing {
    fn parse(n: Node) -> Option<Spacing> {
        if let Some(p) = child(n, "spcPts") {
            return attr_f64(p, "val").map(|v| Spacing::Points((v / 100.0) as f32));
        }
        child(n, "spcPct").and_then(|p| attr_f64(p, "val")).map(|v| Spacing::Percent((v / 100_000.0) as f32))
    }

    /// Resolves to points for a paragraph whose first character is `size_pt`.
    pub fn to_points(self, size_pt: f32) -> f32 {
        match self {
            Spacing::Points(p) => p,
            Spacing::Percent(f) => f * size_pt * 1.2,
        }
    }
}

impl LevelStyle {
    /// Fills unset fields from `fallback`.
    pub fn or(mut self, fallback: &LevelStyle) -> LevelStyle {
        macro_rules! take {
            ($($f:ident),*) => { $( if self.$f.is_none() { self.$f = fallback.$f.clone(); } )* };
        }
        take!(size, bold, italic, underline, strike, baseline, spacing, color, latin, ea, align, line_spacing, bu_kind, bu_font, bu_color, bu_size, mar_l, indent, space_before, space_after);
        self
    }

    /// The resolved bullet, if the level has one.
    pub fn bullet(&self) -> Option<Bullet> {
        let kind = self.bu_kind.clone().flatten()?;
        Some(Bullet { kind, font: self.bu_font.clone().flatten(), color: self.bu_color.flatten(), size: self.bu_size.flatten() })
    }

    /// Parses run properties (`a:rPr`, `a:defRPr`, `a:endParaRPr`).
    pub fn parse_rpr(n: Node, ctx: &ColorCtx) -> LevelStyle {
        LevelStyle {
            size: attr_f64(n, "sz").map(|v| (v / 100.0) as f32),
            bold: attr_bool(n, "b"),
            italic: attr_bool(n, "i"),
            underline: n.attribute("u").map(|u| u != "none"),
            strike: n.attribute("strike").map(|s| s != "noStrike"),
            baseline: attr_f64(n, "baseline").map(|v| (v / 100_000.0) as f32),
            spacing: attr_f64(n, "spc").map(|v| (v / 100.0) as f32),
            // Gradient text is approximated by its first stop for now.
            color: child(n, "solidFill")
                .and_then(|f| ctx.color_in(f, None))
                .or_else(|| path(n, &["gradFill", "gsLst", "gs"]).and_then(|gs| ctx.color_in(gs, None))),
            latin: child(n, "latin").and_then(|f| f.attribute("typeface")).map(str::to_string),
            ea: child(n, "ea").and_then(|f| f.attribute("typeface")).map(str::to_string),
            ..Default::default()
        }
    }

    /// Parses paragraph properties (`a:pPr`, `a:lvlNpPr`) including the nested `a:defRPr`.
    pub fn parse_ppr(n: Node, ctx: &ColorCtx) -> LevelStyle {
        let mut style = child(n, "defRPr").map(|r| LevelStyle::parse_rpr(r, ctx)).unwrap_or_default();
        style.align = n.attribute("algn").map(|a| match a {
            "ctr" => Align::Center,
            "r" => Align::Right,
            "just" | "dist" => Align::Justify,
            _ => Align::Left,
        });
        style.line_spacing = path(n, &["lnSpc", "spcPct"]).and_then(|p| attr_f64(p, "val")).map(|v| (v / 100_000.0) as f32);
        style.mar_l = attr_i64(n, "marL");
        style.indent = attr_i64(n, "indent");
        style.space_before = child(n, "spcBef").and_then(Spacing::parse);
        style.space_after = child(n, "spcAft").and_then(Spacing::parse);
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "buNone" => style.bu_kind = Some(None),
                "buChar" => style.bu_kind = Some(c.attribute("char").and_then(|ch| ch.chars().next()).map(BulletKind::Char)),
                "buAutoNum" => {
                    let scheme = c.attribute("type").unwrap_or("arabicPeriod").to_string();
                    let start = attr_i64(c, "startAt").unwrap_or(1).max(1) as u32;
                    style.bu_kind = Some(Some(BulletKind::AutoNum { scheme, start }));
                }
                // Picture bullets show as a plain dot for now.
                "buBlip" => style.bu_kind = Some(Some(BulletKind::Char('•'))),
                "buFontTx" => style.bu_font = Some(None),
                "buFont" => style.bu_font = Some(c.attribute("typeface").map(str::to_string)),
                "buClrTx" => style.bu_color = Some(None),
                "buClr" => style.bu_color = Some(ctx.color_in(c, None)),
                "buSzTx" => style.bu_size = Some(None),
                "buSzPct" => style.bu_size = Some(attr_f64(c, "val").map(|v| (v / 100_000.0) as f32)),
                _ => {}
            }
        }
        style
    }
}

/// Nine list levels (`a:lvl1pPr` .. `a:lvl9pPr`).
#[derive(Clone, Debug, Default)]
pub struct ListStyle(pub [LevelStyle; 9]);

impl ListStyle {
    pub fn parse(n: Option<Node>, ctx: &ColorCtx) -> ListStyle {
        let mut out = ListStyle::default();
        let Some(n) = n else { return out };
        for (i, level) in out.0.iter_mut().enumerate() {
            if let Some(p) = child(n, &format!("lvl{}pPr", i + 1)) {
                *level = LevelStyle::parse_ppr(p, ctx);
            }
        }
        out
    }

    pub fn level(&self, lvl: u8) -> &LevelStyle {
        &self.0[(lvl as usize).min(8)]
    }
}

/// Partially specified `a:bodyPr`.
#[derive(Clone, Debug, Default)]
pub struct BodyProps {
    pub anchor: Option<Anchor>,
    pub insets: [Option<Emu>; 4],
    pub wrap: Option<bool>,
    pub font_scale: Option<f32>,
    pub line_spacing_reduction: Option<f32>,
    pub auto_grow: Option<bool>,
}

impl BodyProps {
    pub fn parse(n: Option<Node>) -> BodyProps {
        let Some(n) = n else { return BodyProps::default() };
        let autofit = child(n, "normAutofit");
        BodyProps {
            anchor: n.attribute("anchor").map(|a| match a {
                "ctr" => Anchor::Middle,
                "b" => Anchor::Bottom,
                _ => Anchor::Top,
            }),
            insets: ["lIns", "tIns", "rIns", "bIns"].map(|k| attr_i64(n, k)),
            wrap: n.attribute("wrap").map(|w| w != "none"),
            font_scale: autofit.and_then(|a| attr_f64(a, "fontScale")).map(|v| (v / 100_000.0) as f32),
            line_spacing_reduction: autofit.and_then(|a| attr_f64(a, "lnSpcReduction")).map(|v| (v / 100_000.0) as f32),
            auto_grow: if child(n, "spAutoFit").is_some() {
                Some(true)
            } else if child(n, "noAutofit").is_some() || autofit.is_some() {
                Some(false)
            } else {
                None
            },
        }
    }

    pub fn or(mut self, fallback: &BodyProps) -> BodyProps {
        self.anchor = self.anchor.or(fallback.anchor);
        for i in 0..4 {
            self.insets[i] = self.insets[i].or(fallback.insets[i]);
        }
        self.wrap = self.wrap.or(fallback.wrap);
        self.font_scale = self.font_scale.or(fallback.font_scale);
        self.line_spacing_reduction = self.line_spacing_reduction.or(fallback.line_spacing_reduction);
        self.auto_grow = self.auto_grow.or(fallback.auto_grow);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ColorCtx {
        let mut theme = Theme::default();
        theme.colors.insert("accent1".into(), Color::from_hex("4472C4").unwrap());
        theme.colors.insert("dk1".into(), Color::BLACK);
        ColorCtx { theme, clr_map: HashMap::new() }
    }

    fn color(xml: &str) -> Option<Color> {
        let doc = roxmltree::Document::parse(xml).unwrap();
        ctx().color_in(doc.root_element(), None)
    }

    #[test]
    fn scheme_colors_and_modifiers() {
        assert_eq!(color(r#"<f><schemeClr val="accent1"/></f>"#), Color::from_hex("4472C4"));
        assert_eq!(color(r#"<f><schemeClr val="tx1"/></f>"#), Some(Color::BLACK));
        // PowerPoint's "Accent 1, lighter 80%" is lumMod 20000 + lumOff 80000 → DAE3F3.
        let light = color(r#"<f><schemeClr val="accent1"><lumMod val="20000"/><lumOff val="80000"/></schemeClr></f>"#).unwrap();
        let expect = Color::from_hex("DAE3F3").unwrap();
        assert!((light.r as i32 - expect.r as i32).abs() <= 2 && (light.b as i32 - expect.b as i32).abs() <= 2, "{light:?}");
        let half = color(r#"<f><srgbClr val="FF0000"><alpha val="50000"/></srgbClr></f>"#).unwrap();
        assert_eq!(half.a, 128);
    }
}
