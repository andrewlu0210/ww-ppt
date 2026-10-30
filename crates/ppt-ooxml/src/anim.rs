//! Reading `p:timing` into the animation model. Animations are saved by
//! keeping the original XML (see `slide_xml`), so there is no writer.

use std::collections::HashMap;

use ppt_model::ShapeId;
use ppt_model::anim::{Behavior, EffectMode, Key, NodeKind, Target, TimeNode, Timing, Value};
use roxmltree::Node;

use crate::style::ColorCtx;

/// Shapes by their `cNvPr` id; a group's id maps to all its members.
pub(crate) type SpidMap = HashMap<u32, Vec<ShapeId>>;

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn elements<'a, 'i>(n: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(|c| c.is_element())
}

/// Percentages are stored in 1000ths (`50000` = 50%).
fn pct(v: Option<&str>) -> Option<f64> {
    v.and_then(|s| s.parse::<f64>().ok()).map(|n| n / 100_000.0)
}

pub(crate) fn parse(root: Node, spids: &SpidMap, colors: &ColorCtx) -> Option<Timing> {
    let timing = child(root, "timing")?;
    let top = elements(child(timing, "tnLst")?).next()?;
    let cx = Cx { spids, colors };
    Some(Timing { root: cx.node(top)? })
}

struct Cx<'a> {
    spids: &'a SpidMap,
    colors: &'a ColorCtx,
}

impl Cx<'_> {
    /// Timing attributes of a `p:cTn`, with its children.
    fn timed(&self, ctn: Node, kind: NodeKind) -> TimeNode {
        let mut n = TimeNode { kind, ..Default::default() };
        n.dur_ms = ctn.attribute("dur").and_then(|d| d.parse().ok());
        n.repeat = match ctn.attribute("repeatCount") {
            Some("indefinite") => 1.0,
            v => v.and_then(|s| s.parse::<f64>().ok()).map_or(1.0, |r| r / 1000.0),
        };
        n.auto_rev = matches!(ctn.attribute("autoRev"), Some("1" | "true"));
        n.accel = pct(ctn.attribute("accel")).unwrap_or(0.0);
        n.decel = pct(ctn.attribute("decel")).unwrap_or(0.0);
        n.fill_hold = matches!(ctn.attribute("fill"), Some("hold" | "freeze"));
        n.node_type = ctn.attribute("nodeType").unwrap_or_default().to_string();
        n.preset_class = ctn.attribute("presetClass").unwrap_or_default().to_string();
        n.preset_id = ctn.attribute("presetID").and_then(|v| v.parse().ok());
        if let Some(conds) = child(ctn, "stCondLst") {
            for c in elements(conds) {
                match (c.attribute("evt"), c.attribute("delay")) {
                    (None, Some("indefinite")) => n.click = true,
                    (Some("onBegin"), _) => n.auto_begin = true,
                    (None, Some(d)) => n.delay_ms = d.parse().unwrap_or(0.0),
                    _ => {}
                }
            }
        }
        if let Some(kids) = child(ctn, "childTnLst") {
            n.children = elements(kids).filter_map(|k| self.node(k)).collect();
        }
        n
    }

    fn node(&self, n: Node) -> Option<TimeNode> {
        let name = n.tag_name().name();
        match name {
            "par" | "excl" => Some(self.timed(child(n, "cTn")?, NodeKind::Par)),
            "seq" => Some(self.timed(child(n, "cTn")?, NodeKind::Seq)),
            "set" | "anim" | "animEffect" | "animScale" | "animRot" | "animMotion" | "animClr" => {
                let bhvr = child(n, "cBhvr")?;
                let ctn = child(bhvr, "cTn")?;
                let kind = match (self.target(bhvr), self.behavior(n, bhvr)) {
                    (Some(t), Some(b)) => NodeKind::Behavior(t, b),
                    _ => NodeKind::Other,
                };
                Some(self.timed(ctn, kind))
            }
            _ => {
                // Commands, media: keep their timing so sequences stay aligned.
                let ctn = n.descendants().find(|d| d.tag_name().name() == "cTn")?;
                Some(TimeNode { children: Vec::new(), ..self.timed(ctn, NodeKind::Other) })
            }
        }
    }

    fn target(&self, bhvr: Node) -> Option<Target> {
        let sp = child(child(bhvr, "tgtEl")?, "spTgt")?;
        let shapes = self.spids.get(&sp.attribute("spid")?.parse().ok()?)?.clone();
        let paragraphs = child(sp, "txEl").and_then(|t| child(t, "pRg")).and_then(|r| Some((r.attribute("st")?.parse().ok()?, r.attribute("end")?.parse().ok()?)));
        Some(Target { shapes, paragraphs })
    }

    fn value(&self, v: Node) -> Option<Value> {
        let attr = v.attribute("val");
        Some(match v.tag_name().name() {
            "fltVal" | "intVal" => Value::Num(attr?.parse().ok()?),
            "boolVal" => Value::Num(if matches!(attr, Some("1" | "true")) { 1.0 } else { 0.0 }),
            "strVal" => Value::Str(attr?.to_string()),
            "clrVal" => Value::Color(self.colors.color_in(v, None)?),
            _ => return None,
        })
    }

    fn behavior(&self, n: Node, bhvr: Node) -> Option<Behavior> {
        let attr = child(bhvr, "attrNameLst").and_then(|l| elements(l).find_map(|a| a.text())).unwrap_or_default().to_string();
        let xy = |name: &str| child(n, name).and_then(|e| Some((pct(e.attribute("x"))?, pct(e.attribute("y"))?)));
        Some(match n.tag_name().name() {
            "set" => Behavior::Set { attr, to: self.value(elements(child(n, "to")?).next()?)? },
            "anim" => {
                let mut keys: Vec<Key> = Vec::new();
                if let Some(list) = child(n, "tavLst") {
                    let tavs: Vec<Node> = elements(list).collect();
                    let count = tavs.len();
                    for (i, tav) in tavs.into_iter().enumerate() {
                        let Some(value) = child(tav, "val").and_then(|v| elements(v).next()).and_then(|v| self.value(v)) else { continue };
                        let t = pct(tav.attribute("tm")).unwrap_or(if count > 1 { i as f64 / (count - 1) as f64 } else { 0.0 });
                        keys.push(Key { t, value, fmla: tav.attribute("fmla").map(str::to_string) });
                    }
                } else {
                    // from / to / by attributes; a missing start is the attribute's own value.
                    let base = format!("#{attr}");
                    let from = n.attribute("from").map(str::to_string).unwrap_or(base);
                    let to = match (n.attribute("to"), n.attribute("by")) {
                        (Some(t), _) => t.to_string(),
                        (None, Some(b)) => format!("({from})+({b})"),
                        _ => return None,
                    };
                    keys = vec![Key { t: 0.0, value: Value::Str(from), fmla: None }, Key { t: 1.0, value: Value::Str(to), fmla: None }];
                }
                Behavior::Anim { attr, keys, discrete: n.attribute("calcmode") == Some("discrete") }
            }
            "animEffect" => {
                let mode = match n.attribute("transition") {
                    Some("out") => EffectMode::Out,
                    Some("none") => EffectMode::None,
                    _ => EffectMode::In,
                };
                Behavior::Effect { mode, filter: n.attribute("filter").unwrap_or("fade").to_string() }
            }
            "animScale" => Behavior::Scale { from: xy("from"), to: xy("to"), by: xy("by") },
            "animRot" => {
                let deg = |name: &str| n.attribute(name).and_then(|v| v.parse::<f64>().ok()).map(|v| v / 60_000.0);
                Behavior::Rotate { from: deg("from"), to: deg("to"), by: deg("by") }
            }
            "animMotion" => Behavior::Motion { points: motion_path(n.attribute("path").unwrap_or_default()) },
            "animClr" => Behavior::Color { attr, to: child(n, "to").and_then(|t| self.colors.color_in(t, None)) },
            _ => return None,
        })
    }
}

/// Flattens a motion path (`M 0 0 L 0.25 0 C ... E`) into points.
pub(crate) fn motion_path(path: &str) -> Vec<(f64, f64)> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut nums: Vec<f64> = Vec::new();
    let mut cmd = 'M';
    let flush = |cmd: char, nums: &mut Vec<f64>, points: &mut Vec<(f64, f64)>| {
        let cur = points.last().copied().unwrap_or((0.0, 0.0));
        match cmd {
            'M' | 'L' | 'm' | 'l' => {
                for p in nums.as_chunks::<2>().0 {
                    let (x, y) = if cmd.is_ascii_lowercase() { (cur.0 + p[0], cur.1 + p[1]) } else { (p[0], p[1]) };
                    points.push((x, y));
                }
            }
            'C' | 'c' => {
                let mut start = cur;
                for c in nums.as_chunks::<6>().0 {
                    let off = if cmd == 'c' { start } else { (0.0, 0.0) };
                    let (p1, p2, p3) = ((c[0] + off.0, c[1] + off.1), (c[2] + off.0, c[3] + off.1), (c[4] + off.0, c[5] + off.1));
                    for i in 1..=12 {
                        let t = i as f64 / 12.0;
                        let u = 1.0 - t;
                        let f = |a: f64, b: f64, c: f64, d: f64| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d;
                        points.push((f(start.0, p1.0, p2.0, p3.0), f(start.1, p1.1, p2.1, p3.1)));
                    }
                    start = p3;
                }
            }
            'Z' | 'z' => {
                if let Some(&first) = points.first() {
                    points.push(first);
                }
            }
            _ => {}
        }
        nums.clear();
    };
    for tok in path.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()) {
        match tok.parse::<f64>() {
            Ok(v) => nums.push(v),
            Err(_) => {
                flush(cmd, &mut nums, &mut points);
                cmd = tok.chars().next().unwrap_or('E');
                if matches!(cmd, 'Z' | 'z') {
                    flush(cmd, &mut nums, &mut points);
                }
            }
        }
    }
    flush(cmd, &mut nums, &mut points);
    if points.first() != Some(&(0.0, 0.0)) {
        // Paths are relative to the shape: they start where it is.
        points.insert(0, (0.0, 0.0));
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_paths() {
        assert_eq!(motion_path("M 0 0 L 0.25 0 L 0.25 0.5 E"), vec![(0.0, 0.0), (0.25, 0.0), (0.25, 0.5)]);
        let curve = motion_path("M 0 0 C 0 0.1 0.1 0.1 0.1 0 E");
        assert_eq!(curve.len(), 13);
        assert!((curve[12].0 - 0.1).abs() < 1e-9 && curve[12].1.abs() < 1e-9);
        assert!(curve[6].1 > 0.05);
        assert_eq!(motion_path("M 0 0 l 0.1 0 l 0 0.1 Z").last(), Some(&(0.0, 0.0)));
    }
}
