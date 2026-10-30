//! Reading `p:timing` into the animation model, and writing it back after
//! edits. Effects that were not edited keep their original XML.

use std::collections::HashMap;

use ppt_model::ShapeId;
use ppt_model::anim::{Behavior, Build, EffectMode, Key, NodeKind, RawXml, Target, TimeNode, Timing, Value};
use roxmltree::{Document, Node};

use crate::style::ColorCtx;
use crate::writer::esc;

/// Shapes by their `cNvPr` id: the shape or group it names, and the shapes
/// drawn for it (a group's members).
pub(crate) type SpidMap = HashMap<u32, (ShapeId, Vec<ShapeId>)>;

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
    let cx = Cx { spids, colors, text: root.document().input_text() };
    let mut root_node = cx.node(top)?;
    // Sequences besides the main one (triggers) are kept as they were.
    for (c, xml) in root_node.children.iter_mut().zip(elements(child(top, "cTn").and_then(|c| child(c, "childTnLst"))?)) {
        if c.node_type != "mainSeq" {
            c.source = Some(RawXml::new(cx.raw(xml)));
        }
    }
    let builds = child(timing, "bldLst")
        .into_iter()
        .flat_map(elements)
        .filter_map(|b| {
            let (element, _) = spids.get(&b.attribute("spid")?.parse().ok()?)?;
            Some(Build { element: *element, grp_id: b.attribute("grpId")?.parse().ok()?, xml: cx.raw(b).into() })
        })
        .collect();
    Some(Timing { root: root_node, spids: spids.iter().map(|(k, v)| (*k, v.0)).collect(), builds })
}

struct Cx<'a> {
    spids: &'a SpidMap,
    colors: &'a ColorCtx,
    text: &'a str,
}

impl Cx<'_> {
    fn raw(&self, n: Node) -> &str {
        &self.text[n.range()]
    }

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
        n.preset_subtype = ctn.attribute("presetSubtype").and_then(|v| v.parse().ok());
        n.grp_id = ctn.attribute("grpId").and_then(|v| v.parse().ok());
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
            "par" | "excl" => {
                let mut node = self.timed(child(n, "cTn")?, NodeKind::Par);
                if matches!(node.node_type.as_str(), "clickEffect" | "withEffect" | "afterEffect") {
                    node.source = Some(RawXml::new(self.raw(n)));
                }
                Some(node)
            }
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
        let (element, shapes) = self.spids.get(&sp.attribute("spid")?.parse().ok()?)?.clone();
        let paragraphs = child(sp, "txEl").and_then(|t| child(t, "pRg")).and_then(|r| Some((r.attribute("st")?.parse().ok()?, r.attribute("end")?.parse().ok()?)));
        Some(Target { element, shapes, paragraphs })
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

// --- Writing -------------------------------------------------------------------

/// What writing a slide's timing needs to know about the written shapes.
pub(crate) struct TimingCx<'a> {
    /// The `cNvPr` id a shape or group was written with.
    pub ids: &'a dyn Fn(ShapeId) -> Option<u32>,
    /// Whether the shape has a text body (its effects get a build entry).
    pub has_text: &'a dyn Fn(ShapeId) -> bool,
    /// Namespace declarations of the slide, for parsing kept XML.
    pub namespaces: &'a [(String, String)],
}

struct W<'a> {
    cx: &'a TimingCx<'a>,
    timing: &'a Timing,
    next_id: u32,
}

fn ms(v: f64) -> String {
    (v.round() as i64).to_string()
}

fn per100k(v: f64) -> String {
    ((v * 100_000.0).round() as i64).to_string()
}

/// `p:timing` for edited animations, or `None` when nothing is left to play.
pub(crate) fn timing_xml(timing: &Timing, cx: &TimingCx) -> Option<String> {
    // Effects on shapes that were not written would make PowerPoint repair the file.
    let mut t = timing.clone();
    t.retain_elements(|e| (cx.ids)(e).is_some());
    if t.is_empty() {
        return None;
    }
    let mut w = W { cx, timing: &t, next_id: 1 };
    let root_id = w.id();
    let mut kids = String::new();
    let mut written: Vec<&TimeNode> = Vec::new();
    for c in &t.root.children {
        if c.node_type == "mainSeq" {
            let effects = t.effects();
            if !effects.is_empty() {
                kids.push_str(&w.main_seq(c));
                written.extend(effects);
            }
        } else if let Some(xml) = c.source.as_ref().and_then(|raw| w.raw(raw, None)) {
            kids.push_str(&xml);
        }
    }
    if kids.is_empty() {
        return None;
    }
    let builds = w.builds(&written);
    Some(format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="{root_id}" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>{kids}</p:childTnLst></p:cTn></p:par></p:tnLst>{builds}</p:timing>"#
    ))
}

impl W<'_> {
    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn main_seq(&mut self, main: &TimeNode) -> String {
        let id = self.id();
        let mut groups = String::new();
        for g in &main.children {
            let gid = self.id();
            let begin = if g.auto_begin { format!(r#"<p:cond evt="onBegin" delay="0"><p:tn val="{id}"/></p:cond>"#) } else { String::new() };
            let mut subs = String::new();
            for sub in &g.children {
                let sid = self.id();
                let effects: String = sub.children.iter().map(|e| self.effect(e)).collect();
                subs.push_str(&format!(r#"<p:par><p:cTn id="{sid}" fill="hold"><p:stCondLst><p:cond delay="{}"/></p:stCondLst><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:par>"#, ms(sub.delay_ms)));
            }
            groups.push_str(&format!(r#"<p:par><p:cTn id="{gid}" fill="hold"><p:stCondLst><p:cond delay="indefinite"/>{begin}</p:stCondLst><p:childTnLst>{subs}</p:childTnLst></p:cTn></p:par>"#));
        }
        let slide_cond = |evt: &str| format!(r#"<p:cond evt="{evt}" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond>"#);
        format!(
            r#"<p:seq concurrent="1" nextAc="seek"><p:cTn id="{id}" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{groups}</p:childTnLst></p:cTn><p:prevCondLst>{}</p:prevCondLst><p:nextCondLst>{}</p:nextCondLst></p:seq>"#,
            slide_cond("onPrev"),
            slide_cond("onNext")
        )
    }

    fn effect(&mut self, e: &TimeNode) -> String {
        if let Some(xml) = e.source.as_ref().and_then(|raw| self.raw(raw, Some(e))) {
            return xml;
        }
        self.node(e)
    }

    /// A `p:cTn` with id `id` (allocated before its children's, in document order).
    fn ctn(&self, id: u32, n: &TimeNode, inner: &str) -> String {
        let mut a = format!(r#" id="{id}""#);
        if let Some(v) = n.preset_id {
            a.push_str(&format!(r#" presetID="{v}""#));
        }
        if !n.preset_class.is_empty() {
            a.push_str(&format!(r#" presetClass="{}""#, esc(&n.preset_class)));
        }
        if let Some(v) = n.preset_subtype {
            a.push_str(&format!(r#" presetSubtype="{v}""#));
        }
        if let Some(d) = n.dur_ms {
            a.push_str(&format!(r#" dur="{}""#, ms(d)));
        }
        if n.repeat != 1.0 {
            a.push_str(&format!(r#" repeatCount="{}""#, ms(n.repeat * 1000.0)));
        }
        if n.accel > 0.0 {
            a.push_str(&format!(r#" accel="{}""#, per100k(n.accel)));
        }
        if n.decel > 0.0 {
            a.push_str(&format!(r#" decel="{}""#, per100k(n.decel)));
        }
        if n.auto_rev {
            a.push_str(r#" autoRev="1""#);
        }
        if n.fill_hold {
            a.push_str(r#" fill="hold""#);
        }
        if let Some(g) = n.grp_id.or_else(|| (!n.preset_class.is_empty()).then_some(0)) {
            a.push_str(&format!(r#" grpId="{g}""#));
        }
        if !n.node_type.is_empty() {
            a.push_str(&format!(r#" nodeType="{}""#, esc(&n.node_type)));
        }
        let cond = if n.delay_ms != 0.0 || !n.node_type.is_empty() { format!(r#"<p:stCondLst><p:cond delay="{}"/></p:stCondLst>"#, ms(n.delay_ms)) } else { String::new() };
        if inner.is_empty() && cond.is_empty() {
            format!("<p:cTn{a}/>")
        } else {
            format!("<p:cTn{a}>{cond}{inner}</p:cTn>")
        }
    }

    /// A node written from the model (new or edited effects).
    fn node(&mut self, n: &TimeNode) -> String {
        match &n.kind {
            NodeKind::Par | NodeKind::Seq => {
                let id = self.id();
                let kids: String = n.children.iter().map(|c| self.node(c)).collect();
                let tag = if matches!(n.kind, NodeKind::Seq) { "seq" } else { "par" };
                let list = if kids.is_empty() { String::new() } else { format!("<p:childTnLst>{kids}</p:childTnLst>") };
                format!("<p:{tag}>{}</p:{tag}>", self.ctn(id, n, &list))
            }
            NodeKind::Behavior(target, b) => self.behavior(n, target, b),
            NodeKind::Other => String::new(),
        }
    }

    fn behavior(&mut self, n: &TimeNode, target: &Target, b: &Behavior) -> String {
        let Some(spid) = (self.cx.ids)(target.element) else { return String::new() };
        let tx = target.paragraphs.map(|(st, end)| format!(r#"<p:txEl><p:pRg st="{st}" end="{end}"/></p:txEl>"#)).unwrap_or_default();
        let tgt = format!(r#"<p:tgtEl><p:spTgt spid="{spid}">{tx}</p:spTgt></p:tgtEl>"#).replace(r#""></p:spTgt>"#, r#""/>"#);
        let names = |names: &[&str]| -> String {
            let list: String = names.iter().map(|n| format!("<p:attrName>{}</p:attrName>", esc(n))).collect();
            format!("<p:attrNameLst>{list}</p:attrNameLst>")
        };
        let id = self.id();
        let ctn = self.ctn(id, n, "");
        let bhvr = |extra: &str, attrs: &str| format!("<p:cBhvr{extra}>{ctn}{tgt}{attrs}</p:cBhvr>");
        match b {
            Behavior::Set { attr, to } => format!("<p:set>{}<p:to>{}</p:to></p:set>", bhvr("", &names(&[attr])), value_xml(to)),
            Behavior::Anim { attr, keys, discrete } => {
                let value_type = match keys.first().map(|k| &k.value) {
                    Some(Value::Color(_)) => "clr",
                    _ if attr == "style.visibility" => "str",
                    _ => "num",
                };
                let calc = if *discrete { "discrete" } else { "lin" };
                let additive = if matches!(attr.as_str(), "ppt_x" | "ppt_y") { r#" additive="base""# } else { "" };
                let tavs: String = keys
                    .iter()
                    .map(|k| {
                        let fmla = k.fmla.as_ref().map(|f| format!(r#" fmla="{}""#, esc(f))).unwrap_or_default();
                        format!(r#"<p:tav tm="{}"{fmla}><p:val>{}</p:val></p:tav>"#, per100k(k.t), value_xml(&k.value))
                    })
                    .collect();
                format!(r#"<p:anim calcmode="{calc}" valueType="{value_type}">{}<p:tavLst>{tavs}</p:tavLst></p:anim>"#, bhvr(additive, &names(&[attr])))
            }
            Behavior::Effect { mode, filter } => {
                let mode = match mode {
                    EffectMode::In => "in",
                    EffectMode::Out => "out",
                    EffectMode::None => "none",
                };
                format!(r#"<p:animEffect transition="{mode}" filter="{}">{}</p:animEffect>"#, esc(filter), bhvr("", ""))
            }
            Behavior::Scale { from, to, by } => {
                let pt = |tag: &str, v: &Option<(f64, f64)>| v.map(|(x, y)| format!(r#"<p:{tag} x="{}" y="{}"/>"#, per100k(x), per100k(y))).unwrap_or_default();
                format!("<p:animScale>{}{}{}{}</p:animScale>", bhvr("", ""), pt("by", by), pt("from", from), pt("to", to))
            }
            Behavior::Rotate { from, to, by } => {
                let deg = |name: &str, v: &Option<f64>| v.map(|d| format!(r#" {name}="{}""#, ((d * 60_000.0).round() as i64))).unwrap_or_default();
                format!("<p:animRot{}{}{}>{}</p:animRot>", deg("by", by), deg("from", from), deg("to", to), bhvr("", &names(&["r"])))
            }
            Behavior::Motion { points } => {
                let mut path = String::new();
                for (i, (x, y)) in points.iter().enumerate() {
                    path.push_str(&format!("{} {x} {y} ", if i == 0 { "M" } else { "L" }));
                }
                path.push('E');
                format!(r#"<p:animMotion origin="layout" path="{path}" pathEditMode="relative">{}</p:animMotion>"#, bhvr("", &names(&["ppt_x", "ppt_y"])))
            }
            Behavior::Color { attr, to } => {
                let to = to.map(|c| format!("<p:to>{}</p:to>", color_xml(c))).unwrap_or_default();
                format!(r#"<p:animClr clrSpc="rgb" dir="cw">{}{to}</p:animClr>"#, bhvr("", &names(&[attr])))
            }
        }
    }

    /// Kept original XML with fresh node ids, current shape ids, and (for an
    /// effect) its current trigger, delay and time scale. `None` when it
    /// names a shape that is gone.
    fn raw(&mut self, raw: &RawXml, effect: Option<&TimeNode>) -> Option<String> {
        let (wrapped, offset) = crate::writer::wrap(&raw.xml, self.cx.namespaces);
        let doc = Document::parse(&wrapped).ok()?;
        let el = doc.root_element().first_element_child()?;
        let own = child(el, "cTn");
        let mut edits = Vec::new();
        let mut renumbered = HashMap::new();
        for ctn in el.descendants().filter(|n| n.tag_name().name() == "cTn") {
            let attr = ctn.attribute_node("id")?;
            let id = self.id();
            renumbered.insert(attr.value().to_string(), id);
            edits.push((attr.range_value(), id.to_string()));
            if raw.time_scale != 1.0
                && let Some(d) = ctn.attribute_node("dur")
                && let Ok(v) = d.value().parse::<f64>()
                && v > 1.0
            {
                edits.push((d.range_value(), ms(v * raw.time_scale)));
            }
        }
        for n in el.descendants().filter(|n| n.is_element()) {
            match n.tag_name().name() {
                "spTgt" => {
                    let attr = n.attribute_node("spid")?;
                    let element = self.timing.spids.get(&attr.value().parse().ok()?)?;
                    edits.push((attr.range_value(), (self.cx.ids)(*element)?.to_string()));
                }
                "tn" => {
                    if let Some(attr) = n.attribute_node("val")
                        && let Some(id) = renumbered.get(attr.value())
                    {
                        edits.push((attr.range_value(), id.to_string()));
                    }
                }
                "cond" => {
                    let Some(attr) = n.attribute_node("delay") else { continue };
                    let Ok(v) = attr.value().parse::<f64>() else { continue };
                    let is_own = own.is_some_and(|c| n.ancestors().nth(2) == Some(c));
                    match effect {
                        Some(e) if is_own => edits.push((attr.range_value(), ms(e.delay_ms))),
                        _ if raw.time_scale != 1.0 && !is_own => edits.push((attr.range_value(), ms(v * raw.time_scale))),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if let (Some(e), Some(c)) = (effect, own) {
            match c.attribute_node("nodeType") {
                Some(attr) => edits.push((attr.range_value(), esc(&e.node_type))),
                None => {
                    let name_end = c.range().start + wrapped[c.range().start..].find(|ch: char| ch.is_whitespace() || ch == '>' || ch == '/')?;
                    edits.push((name_end..name_end, format!(r#" nodeType="{}""#, esc(&e.node_type))));
                }
            }
        }
        let patched = crate::writer::apply_edits(&wrapped, edits);
        Some(patched[offset..patched.len() - "</w>".len()].to_string())
    }

    /// The build list: loaded entries still in use, and one per new effect on a shape with text.
    fn builds(&self, effects: &[&TimeNode]) -> String {
        let mut used: Vec<(ShapeId, u32)> = Vec::new();
        for e in effects {
            for el in ppt_model::anim::effect_elements(e) {
                let key = (el, e.grp_id.unwrap_or(0));
                if !used.contains(&key) {
                    used.push(key);
                }
            }
        }
        let mut out = String::new();
        for (el, grp) in used {
            let Some(spid) = (self.cx.ids)(el) else { continue };
            match self.timing.builds.iter().find(|b| b.element == el && b.grp_id == grp) {
                Some(b) => {
                    let (wrapped, offset) = crate::writer::wrap(&b.xml, self.cx.namespaces);
                    let Ok(doc) = Document::parse(&wrapped) else { continue };
                    let Some(attr) = doc.root_element().first_element_child().and_then(|n| n.attribute_node("spid")) else { continue };
                    let r = attr.range_value();
                    out.push_str(&format!("{}{spid}{}", &b.xml[..r.start - offset], &b.xml[r.end - offset..]));
                }
                None if (self.cx.has_text)(el) => out.push_str(&format!(r#"<p:bldP spid="{spid}" grpId="{grp}" animBg="1"/>"#)),
                None => {}
            }
        }
        if out.is_empty() { out } else { format!("<p:bldLst>{out}</p:bldLst>") }
    }
}

fn color_xml(c: ppt_model::Color) -> String {
    format!(r#"<a:srgbClr val="{:02X}{:02X}{:02X}"/>"#, c.r, c.g, c.b)
}

fn value_xml(v: &Value) -> String {
    match v {
        Value::Num(n) => format!(r#"<p:fltVal val="{n}"/>"#),
        Value::Str(s) => format!(r#"<p:strVal val="{}"/>"#, esc(s)),
        Value::Color(c) => format!("<p:clrVal>{}</p:clrVal>", color_xml(*c)),
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
