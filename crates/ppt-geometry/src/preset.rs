//! DrawingML preset geometry: the guide-formula engine behind the 180+
//! shapes of `presetShapeDefinitions.xml` (ECMA-376 Part 1, 20.1.9).
//!
//! A definition has adjust values (`avLst`), guides computed from them
//! (`gdLst`), and paths whose coordinates refer to guides by name.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use kurbo::{Arc, BezPath, Point, Vec2};
use roxmltree::Node;

use crate::Outline;

const DEFINITIONS: &str = include_str!("presets.xml");

/// Angles in DrawingML are 60000ths of a degree.
const ANGLE_UNIT: f64 = 60_000.0;

struct ShapeDef {
    adjust: Vec<(String, String)>,
    guides: Vec<(String, String)>,
    paths: Vec<PathDef>,
    /// Text rectangle (`rect`): l, t, r, b guide references.
    text_rect: Option<[String; 4]>,
}

struct PathDef {
    w: Option<f64>,
    h: Option<f64>,
    fill: Shade,
    stroke: bool,
    cmds: Vec<Cmd>,
}

/// Path fill mode: `norm`, `none`, or one of the shading variants that
/// lighten or darken the shape's fill (for 3D-looking presets).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shade {
    None,
    Normal,
    /// Positive lightens, negative darkens, as a blend factor toward white/black.
    Tint(f32),
}

enum Cmd {
    MoveTo(String, String),
    LineTo(String, String),
    ArcTo { wr: String, hr: String, start: String, swing: String },
    QuadTo([(String, String); 2]),
    CubicTo([(String, String); 3]),
    Close,
}

fn attr(n: Node, name: &str) -> String {
    n.attribute(name).unwrap_or("0").to_string()
}

fn pt(n: Node) -> (String, String) {
    n.children().find(|c| c.tag_name().name() == "pt").map(|p| (attr(p, "x"), attr(p, "y"))).unwrap_or_default()
}

fn pts(n: Node) -> Vec<(String, String)> {
    n.children().filter(|c| c.tag_name().name() == "pt").map(|p| (attr(p, "x"), attr(p, "y"))).collect()
}

fn guides(n: Option<Node>) -> Vec<(String, String)> {
    n.map(|l| l.children().filter(|g| g.tag_name().name() == "gd").map(|g| (attr(g, "name"), g.attribute("fmla").unwrap_or("val 0").to_string())).collect())
        .unwrap_or_default()
}

fn parse_def(n: Node) -> ShapeDef {
    let child = |name: &str| n.children().find(|c| c.tag_name().name() == name);
    let paths = child("pathLst")
        .map(|l| {
            l.children()
                .filter(|p| p.tag_name().name() == "path")
                .map(|p| PathDef {
                    w: p.attribute("w").and_then(|v| v.parse().ok()),
                    h: p.attribute("h").and_then(|v| v.parse().ok()),
                    fill: match p.attribute("fill") {
                        Some("none") => Shade::None,
                        Some("lighten") => Shade::Tint(0.4),
                        Some("lightenLess") => Shade::Tint(0.2),
                        Some("darken") => Shade::Tint(-0.4),
                        Some("darkenLess") => Shade::Tint(-0.2),
                        _ => Shade::Normal,
                    },
                    stroke: !matches!(p.attribute("stroke"), Some("false" | "0")),
                    cmds: p
                        .children()
                        .filter(|c| c.is_element())
                        .filter_map(|c| {
                            Some(match c.tag_name().name() {
                                "moveTo" => {
                                    let (x, y) = pt(c);
                                    Cmd::MoveTo(x, y)
                                }
                                "lnTo" => {
                                    let (x, y) = pt(c);
                                    Cmd::LineTo(x, y)
                                }
                                "arcTo" => Cmd::ArcTo { wr: attr(c, "wR"), hr: attr(c, "hR"), start: attr(c, "stAng"), swing: attr(c, "swAng") },
                                "quadBezTo" => {
                                    let p = pts(c);
                                    Cmd::QuadTo([p.first()?.clone(), p.get(1)?.clone()])
                                }
                                "cubicBezTo" => {
                                    let p = pts(c);
                                    Cmd::CubicTo([p.first()?.clone(), p.get(1)?.clone(), p.get(2)?.clone()])
                                }
                                "close" => Cmd::Close,
                                _ => return None,
                            })
                        })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    ShapeDef {
        adjust: guides(child("avLst")),
        guides: guides(child("gdLst")),
        paths,
        text_rect: child("rect").map(|r| [attr(r, "l"), attr(r, "t"), attr(r, "r"), attr(r, "b")]),
    }
}

fn definitions() -> &'static HashMap<String, ShapeDef> {
    static DEFS: OnceLock<HashMap<String, ShapeDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let doc = roxmltree::Document::parse(DEFINITIONS).expect("embedded preset definitions are valid XML");
        doc.root_element().children().filter(|c| c.is_element()).map(|c| (c.tag_name().name().to_string(), parse_def(c))).collect()
    })
}

pub fn is_known(name: &str) -> bool {
    definitions().contains_key(name)
}

/// Guide values for one shape instance.
struct Env {
    vars: HashMap<String, f64>,
}

impl Env {
    fn new(w: f64, h: f64) -> Self {
        let (ss, ls) = (w.min(h), w.max(h));
        let mut vars = HashMap::new();
        let mut set = |k: &str, v: f64| {
            vars.insert(k.to_string(), v);
        };
        set("w", w);
        set("h", h);
        set("l", 0.0);
        set("t", 0.0);
        set("r", w);
        set("b", h);
        set("hc", w / 2.0);
        set("vc", h / 2.0);
        set("ss", ss);
        set("ls", ls);
        for d in [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 16.0, 24.0, 32.0] {
            set(&format!("wd{d}"), w / d);
            set(&format!("hd{d}"), h / d);
            set(&format!("ssd{d}"), ss / d);
        }
        set("cd2", 10_800_000.0);
        set("cd4", 5_400_000.0);
        set("cd8", 2_700_000.0);
        set("3cd4", 16_200_000.0);
        set("3cd8", 8_100_000.0);
        set("5cd8", 13_500_000.0);
        set("7cd8", 18_900_000.0);
        Env { vars }
    }

    fn get(&self, token: &str) -> f64 {
        token.parse().ok().or_else(|| self.vars.get(token).copied()).unwrap_or(0.0)
    }

    fn eval(&self, fmla: &str) -> f64 {
        let mut it = fmla.split_whitespace();
        let op = it.next().unwrap_or("val");
        let a: Vec<f64> = it.map(|t| self.get(t)).collect();
        let x = |i: usize| a.get(i).copied().unwrap_or(0.0);
        let rad = |v: f64| v / ANGLE_UNIT * PI / 180.0;
        match op {
            "val" => x(0),
            "*/" => {
                if x(2) == 0.0 {
                    0.0
                } else {
                    x(0) * x(1) / x(2)
                }
            }
            "+-" => x(0) + x(1) - x(2),
            "+/" => {
                if x(2) == 0.0 {
                    0.0
                } else {
                    (x(0) + x(1)) / x(2)
                }
            }
            "?:" => {
                if x(0) > 0.0 {
                    x(1)
                } else {
                    x(2)
                }
            }
            "abs" => x(0).abs(),
            "at2" => x(1).atan2(x(0)).to_degrees() * ANGLE_UNIT,
            "cat2" => x(0) * x(2).atan2(x(1)).cos(),
            "sat2" => x(0) * x(2).atan2(x(1)).sin(),
            "cos" => x(0) * rad(x(1)).cos(),
            "sin" => x(0) * rad(x(1)).sin(),
            "tan" => x(0) * rad(x(1)).tan(),
            "max" => x(0).max(x(1)),
            "min" => x(0).min(x(1)),
            "mod" => (x(0) * x(0) + x(1) * x(1) + x(2) * x(2)).sqrt(),
            "pin" => {
                if x(1) < x(0) {
                    x(0)
                } else if x(1) > x(2) {
                    x(2)
                } else {
                    x(1)
                }
            }
            "sqrt" => x(0).max(0.0).sqrt(),
            _ => 0.0,
        }
    }
}

fn env_for(def: &ShapeDef, w: f64, h: f64, adjust: &[(String, f64)]) -> Env {
    let mut env = Env::new(w, h);
    for (name, fmla) in &def.adjust {
        let v = adjust.iter().find(|(k, _)| k == name).map(|(_, v)| *v).unwrap_or_else(|| env.eval(fmla));
        env.vars.insert(name.clone(), v);
    }
    // Files may write a lone `adj` for presets defined with `adj1` and vice versa.
    if let [(only, _)] = def.adjust.as_slice()
        && let Some((_, v)) = adjust.iter().find(|(k, _)| k == "adj" || k == "adj1")
    {
        env.vars.insert(only.clone(), *v);
    }
    for (name, fmla) in &def.guides {
        let v = env.eval(fmla);
        env.vars.insert(name.clone(), v);
    }
    env
}

/// Outlines of preset `name` at `w` × `h`, or `None` for an unknown preset.
pub fn outlines(name: &str, adjust: &[(String, f64)], w: f64, h: f64) -> Option<Vec<Outline>> {
    let def = definitions().get(name)?;
    let env = env_for(def, w, h, adjust);
    let mut out = Vec::new();
    for p in &def.paths {
        // Paths may use their own coordinate space.
        let sx = p.w.filter(|v| *v > 0.0).map_or(1.0, |pw| w / pw);
        let sy = p.h.filter(|v| *v > 0.0).map_or(1.0, |ph| h / ph);
        let point = |(x, y): &(String, String)| Point::new(env.get(x) * sx, env.get(y) * sy);
        let mut path = BezPath::new();
        let mut current = Point::ZERO;
        let mut start = Point::ZERO;
        for cmd in &p.cmds {
            match cmd {
                Cmd::MoveTo(x, y) => {
                    current = point(&(x.clone(), y.clone()));
                    start = current;
                    path.move_to(current);
                }
                Cmd::LineTo(x, y) => {
                    current = point(&(x.clone(), y.clone()));
                    path.line_to(current);
                }
                Cmd::QuadTo([a, b]) => {
                    current = point(b);
                    path.quad_to(point(a), current);
                }
                Cmd::CubicTo([a, b, c]) => {
                    current = point(c);
                    path.curve_to(point(a), point(b), current);
                }
                Cmd::ArcTo { wr, hr, start: st, swing } => {
                    let (rx, ry) = (env.get(wr) * sx, env.get(hr) * sy);
                    let (st, sw) = (env.get(st), env.get(swing));
                    current = arc_to(&mut path, current, rx, ry, st, sw);
                }
                Cmd::Close => {
                    path.close_path();
                    current = start;
                }
            }
        }
        if path.elements().is_empty() {
            continue;
        }
        out.push(Outline { path, fill: p.fill != Shade::None, stroke: p.stroke, shade: p.fill });
    }
    Some(out)
}

/// Appends an elliptical arc starting at `from`. Angles are true angles on the
/// ellipse (60000ths of a degree), converted to kurbo's parametric angles.
fn arc_to(path: &mut BezPath, from: Point, rx: f64, ry: f64, start: f64, swing: f64) -> Point {
    if rx <= 0.0 || ry <= 0.0 {
        return from;
    }
    let to_rad = |a: f64| a / ANGLE_UNIT * PI / 180.0;
    let parametric = |a: f64| {
        let a = to_rad(a);
        (rx * a.sin()).atan2(ry * a.cos())
    };
    let t0 = parametric(start);
    let t1 = parametric(start + swing);
    // atan2 folds away the direction and full turns; restore them from the swing.
    let sweep = if to_rad(swing).abs() >= 2.0 * PI - 1e-9 {
        2.0 * PI * swing.signum()
    } else {
        let d = t1 - t0;
        if swing > 0.0 && d <= 0.0 {
            d + 2.0 * PI
        } else if swing < 0.0 && d >= 0.0 {
            d - 2.0 * PI
        } else {
            d
        }
    };
    let center = from - Vec2::new(rx * t0.cos(), ry * t0.sin());
    let arc = Arc::new(center, Vec2::new(rx, ry), t0, sweep, 0.0);
    path.extend(arc.append_iter(0.1));
    center + Vec2::new(rx * (t0 + sweep).cos(), ry * (t0 + sweep).sin())
}

/// The preset's text rectangle (left, top, right, bottom) at `w` × `h`.
pub fn text_rect(name: &str, adjust: &[(String, f64)], w: f64, h: f64) -> Option<[f64; 4]> {
    let def = definitions().get(name)?;
    let rect = def.text_rect.as_ref()?;
    let env = env_for(def, w, h, adjust);
    Some(rect.clone().map(|g| env.get(&g)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(name: &str, w: f64, h: f64) -> kurbo::Rect {
        let outs = outlines(name, &[], w, h).unwrap_or_else(|| panic!("{name} unknown"));
        outs.iter().map(|o| kurbo::Shape::bounding_box(&o.path)).reduce(|a, b| a.union(b)).unwrap()
    }

    #[test]
    fn every_definition_evaluates() {
        assert!(definitions().len() > 180, "{}", definitions().len());
        for name in definitions().keys() {
            for o in outlines(name, &[], 200.0, 100.0).unwrap() {
                for el in o.path.elements() {
                    for p in [el.end_point()].into_iter().flatten() {
                        assert!(p.x.is_finite() && p.y.is_finite(), "{name}");
                    }
                }
            }
        }
    }

    #[test]
    fn common_shapes_fill_their_box() {
        for name in ["rect", "roundRect", "ellipse", "triangle", "diamond", "chevron", "rightArrow", "round2DiagRect", "homePlate", "star5"] {
            let b = bounds(name, 200.0, 100.0);
            assert!(b.x0.abs() < 0.5 && b.y0.abs() < 0.5 && (b.x1 - 200.0).abs() < 0.5 && (b.y1 - 100.0).abs() < 0.5, "{name}: {b:?}");
        }
    }

    #[test]
    fn ellipse_arcs_pass_through_the_edge_midpoints() {
        let o = &outlines("ellipse", &[], 200.0, 100.0).unwrap()[0];
        let near = |x: f64, y: f64| o.path.elements().iter().filter_map(|e| e.end_point()).any(|p| (p.x - x).abs() < 0.5 && (p.y - y).abs() < 0.5);
        assert!(near(200.0, 50.0) && near(100.0, 100.0) && near(0.0, 50.0) && near(100.0, 0.0));
    }

    #[test]
    fn adjust_values_change_the_outline() {
        let narrow = bounds_with("chevron", &[("adj".into(), 10_000.0)]);
        let wide = bounds_with("chevron", &[("adj".into(), 90_000.0)]);
        assert_ne!(narrow, wide);
        let r = text_rect("ellipse", &[], 200.0, 100.0).unwrap();
        assert!(r[0] > 0.0 && r[2] < 200.0, "{r:?}");
    }

    fn bounds_with(name: &str, adj: &[(String, f64)]) -> Vec<(i64, i64)> {
        outlines(name, adj, 200.0, 100.0).unwrap()[0].path.elements().iter().filter_map(|e| e.end_point()).map(|p| (p.x.round() as i64, p.y.round() as i64)).collect()
    }

    #[test]
    fn connectors_are_open_and_unfilled() {
        for name in ["bentConnector3", "curvedConnector3", "straightConnector1"] {
            let o = outlines(name, &[], 200.0, 100.0).unwrap();
            assert!(o.iter().all(|o| !o.fill), "{name}");
        }
    }
}
