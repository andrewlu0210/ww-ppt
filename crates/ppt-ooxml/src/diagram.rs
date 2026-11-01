//! SmartArt data model (`dgm:dataModel`) and drawing (`dsp:drawing`) parts:
//! reading the node tree, and writing both parts back after it was edited.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use ppt_model::diagram::{Arrangement, Diagram, Links, Node as DNode, PresPoint, Template, derived_id, template_index};
use ppt_model::{Emu, Shape, Xfrm};
use roxmltree::{Document, Node};

use crate::style::child;
use crate::writer::{Links as LinkRels, apply_edits, esc, paragraphs_xml, xfrm_xml};

/// A data point as read.
struct Pt<'a, 'i> {
    el: Node<'a, 'i>,
    id: &'a str,
    kind: &'a str,
}

/// A connection as read.
struct Cxn<'a, 'i> {
    el: Node<'a, 'i>,
    kind: &'a str,
    src: &'a str,
    dest: &'a str,
    src_ord: u32,
    dest_ord: u32,
}

struct Model<'a, 'i> {
    pts: Vec<Pt<'a, 'i>>,
    cxns: Vec<Cxn<'a, 'i>>,
    root: Node<'a, 'i>,
}

fn is_node(kind: &str) -> bool {
    kind == "node" || kind == "asst"
}

fn model<'a, 'i>(doc: &'a Document<'i>) -> Option<Model<'a, 'i>> {
    let root = doc.root_element();
    let pts = child(root, "ptLst")?
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "pt")
        .filter_map(|el| Some(Pt { el, id: el.attribute("modelId")?, kind: el.attribute("type").unwrap_or("node") }))
        .collect();
    let cxns = child(root, "cxnLst")
        .map(|l| {
            l.children()
                .filter(|n| n.is_element() && n.tag_name().name() == "cxn")
                .filter_map(|el| {
                    Some(Cxn {
                        el,
                        kind: el.attribute("type").unwrap_or("parOf"),
                        src: el.attribute("srcId")?,
                        dest: el.attribute("destId")?,
                        src_ord: el.attribute("srcOrd").and_then(|v| v.parse().ok()).unwrap_or(0),
                        dest_ord: el.attribute("destOrd").and_then(|v| v.parse().ok()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Model { pts, cxns, root })
}

/// Paragraph texts of a point's `dgm:t`.
fn pt_text(pt: Node) -> Vec<String> {
    let Some(t) = child(pt, "t") else { return vec![String::new()] };
    let lines: Vec<String> = t
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "p")
        .map(|p| p.descendants().filter(|n| n.is_element() && n.tag_name().name() == "t").filter_map(|n| n.text()).collect())
        .collect();
    if lines.is_empty() { vec![String::new()] } else { lines }
}

/// Reads the data model and matches the drawing's shapes (`dsp:sp`, in order) to its points.
pub(crate) fn read(data_xml: &str, drawing_xml: &str, shapes: Vec<Shape>, w: Emu, h: Emu) -> Option<Diagram> {
    let doc = Document::parse(data_xml).ok()?;
    let m = model(&doc)?;
    let doc_pt = m.pts.iter().find(|p| p.kind == "doc")?;
    let layout = child(doc_pt.el, "prSet").and_then(|s| s.attribute("loTypeId")).unwrap_or_default().to_string();
    let kinds: HashMap<&str, &Pt> = m.pts.iter().map(|p| (p.id, p)).collect();
    let mut kids: HashMap<&str, Vec<&Cxn>> = HashMap::new();
    for c in m.cxns.iter().filter(|c| c.kind == "parOf") {
        kids.entry(c.src).or_default().push(c);
    }
    for v in kids.values_mut() {
        v.sort_by_key(|c| c.src_ord);
    }
    let mut seen = HashSet::new();
    fn tree(id: &str, kids: &HashMap<&str, Vec<&Cxn>>, kinds: &HashMap<&str, &Pt>, seen: &mut HashSet<String>) -> Vec<DNode> {
        let mut out = Vec::new();
        for c in kids.get(id).into_iter().flatten() {
            let Some(pt) = kinds.get(c.dest).filter(|p| is_node(p.kind)) else { continue };
            if !seen.insert(pt.id.to_string()) {
                continue;
            }
            out.push(DNode { id: pt.id.to_string(), text: pt_text(pt.el), children: tree(pt.id, kids, kinds, seen) });
        }
        out
    }
    let nodes = tree(doc_pt.id, &kids, &kinds, &mut seen);
    if nodes.is_empty() {
        return None;
    }
    let links = m
        .cxns
        .iter()
        .filter(|c| c.kind == "parOf")
        .map(|c| {
            let a = |n: &str| c.el.attribute(n).unwrap_or_default().to_string();
            (c.dest.to_string(), Links { cxn: a("modelId"), par_trans: a("parTransId"), sib_trans: a("sibTransId") })
        })
        .collect();
    let mut sources: HashMap<&str, Vec<&Cxn>> = HashMap::new();
    for c in m.cxns.iter().filter(|c| c.kind == "presOf") {
        sources.entry(c.dest).or_default().push(c);
    }
    let pres: Vec<PresPoint> = m
        .pts
        .iter()
        .filter(|p| p.kind == "pres")
        .map(|p| {
            let set = child(p.el, "prSet");
            let mut src = sources.get(p.id).cloned().unwrap_or_default();
            src.sort_by_key(|c| c.dest_ord);
            PresPoint {
                id: p.id.to_string(),
                name: set.and_then(|s| s.attribute("presName")).unwrap_or_default().to_string(),
                assoc: set.and_then(|s| s.attribute("presAssocID")).unwrap_or_default().to_string(),
                sources: src.iter().map(|c| c.src.to_string()).collect(),
            }
        })
        .collect();
    let index: HashMap<&str, usize> = pres.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
    let drawing = Document::parse(drawing_xml).ok()?;
    let shape_pres: Vec<usize> = drawing
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "sp")
        .map(|sp| sp.attribute("modelId").and_then(|id| index.get(id).copied()))
        .collect::<Option<_>>()?;
    if shape_pres.len() != shapes.len() {
        return None;
    }
    let template = Template::new(layout, nodes, pres, links, shapes, shape_pres, w, h, data_xml.into(), drawing_xml.into());
    Some(Diagram::new(Arc::new(template)))
}

// --- Writing -----------------------------------------------------------------

/// Qualified name of an element as written, e.g. `dgm:pt`.
fn qname<'s>(src: &'s str, n: Node) -> &'s str {
    let tag = &src[n.range().start + 1..];
    &tag[..tag.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(tag.len())]
}

/// Prefix (with its colon) of an element as written.
fn prefix<'s>(src: &'s str, n: Node) -> &'s str {
    let q = qname(src, n);
    q.find(':').map_or("", |i| &q[..=i])
}

/// Edit setting attribute `name` of `el` to `value`, adding it if missing.
fn set_attr(src: &str, el: Node, name: &str, value: &str) -> (Range<usize>, String) {
    match el.attributes().find(|a| a.name() == name && a.namespace().is_none()) {
        Some(a) => (a.range_value(), esc(value)),
        None => {
            let at = el.range().start + 1 + qname(src, el).len();
            (at..at, format!(r#" {name}="{}""#, esc(value)))
        }
    }
}

fn remove_attr(el: Node, name: &str) -> Option<(Range<usize>, String)> {
    let a = el.attributes().find(|a| a.name() == name && a.namespace().is_none())?;
    // Include the whitespace before it.
    let r = a.range();
    Some((r.start - 1..r.end, String::new()))
}

/// `el`'s XML with `edits` (absolute ranges inside it) applied.
fn rewrite(src: &str, el: Node, mut edits: Vec<(Range<usize>, String)>) -> String {
    let base = el.range().start;
    for (r, _) in &mut edits {
        *r = r.start - base..r.end - base;
    }
    apply_edits(&src[el.range()], edits)
}

/// A point's `dgm:t` holding `lines`, formatted like the paragraphs of `like`.
fn text_xml(src: &str, like: Node, lines: &[String], tag_prefix: &str) -> String {
    let t = child(like, "t");
    let a = t.and_then(|t| t.children().find(|c| c.is_element())).map_or("a:", |c| prefix(src, c));
    let head: String = t
        .map(|t| t.children().filter(|c| c.is_element() && c.tag_name().name() != "p").map(|c| &src[c.range()]).collect())
        .unwrap_or_else(|| format!("<{a}bodyPr/><{a}lstStyle/>"));
    let paras: Vec<Node> = t.map(|t| t.children().filter(|c| c.is_element() && c.tag_name().name() == "p").collect()).unwrap_or_default();
    let mut body = String::new();
    for (k, line) in lines.iter().enumerate() {
        let p = paras.get(k.min(paras.len().saturating_sub(1)));
        let ppr = p.and_then(|p| child(*p, "pPr")).map_or("", |n| &src[n.range()]);
        let end = p.and_then(|p| child(*p, "endParaRPr")).map(|n| src[n.range()].to_string());
        let rpr = p
            .and_then(|p| p.children().find(|c| c.is_element() && c.tag_name().name() == "r"))
            .and_then(|r| child(r, "rPr"))
            .map(|n| src[n.range()].to_string())
            .or_else(|| end.as_ref().map(|e| e.replace(":endParaRPr", ":rPr")))
            .unwrap_or_default();
        body.push_str(&format!("<{a}p>{ppr}"));
        if !line.is_empty() {
            body.push_str(&format!("<{a}r>{rpr}<{a}t>{}</{a}t></{a}r>", esc(line)));
        }
        body.push_str(&end.unwrap_or_default());
        body.push_str(&format!("</{a}p>"));
    }
    format!("<{tag_prefix}t>{head}{body}</{tag_prefix}t>")
}

/// XML of node point `like` turned into a point for `node` (id and text).
fn node_pt_xml(src: &str, like: Node, node: &DNode, keep_id: bool) -> String {
    let mut edits = Vec::new();
    let dgm = prefix(src, like);
    if !keep_id {
        edits.push(set_attr(src, like, "modelId", &node.id));
    }
    let empty = node.text.iter().all(|l| l.is_empty());
    if let Some(set) = child(like, "prSet") {
        if empty {
            edits.push(set_attr(src, set, "phldr", "1"));
        } else {
            edits.extend(remove_attr(set, "phldr"));
        }
    }
    let new_t = text_xml(src, like, &node.text, dgm);
    match child(like, "t") {
        Some(t) => edits.push((t.range(), new_t)),
        None => {
            let at = like.range().end - format!("</{}>", qname(src, like)).len();
            edits.push((at..at, new_t));
        }
    }
    rewrite(src, like, edits)
}

/// Every node, depth first, with its parent id (`None` for the top level) and position.
fn walk<'a>(nodes: &'a [DNode], parent: Option<&'a str>, out: &mut Vec<(&'a DNode, Option<&'a str>, usize, usize)>, depth: usize) {
    for (i, n) in nodes.iter().enumerate() {
        out.push((n, parent, i, depth));
        walk(&n.children, Some(&n.id), out, depth + 1);
    }
}

fn find<'a>(nodes: &'a [DNode], id: &str) -> Option<&'a DNode> {
    nodes.iter().find_map(|n| if n.id == id { Some(n) } else { find(&n.children, id) })
}

/// The data model for `d`; `drawing_rel` is the slide's relationship id of the drawing part.
pub(crate) fn data_xml(d: &Diagram, drawing_rel: Option<&str>) -> String {
    let t = &d.template.0;
    let src: &str = &t.data_xml;
    let Ok(doc) = Document::parse(src) else { return src.to_string() };
    let Some(m) = model(&doc) else { return src.to_string() };
    let mut edits = Vec::new();
    if let Some(rel) = drawing_rel
        && let Some(ext) = doc.descendants().find(|n| n.tag_name().name() == "dataModelExt")
    {
        edits.push(set_attr(src, ext, "relId", rel));
    }
    let structure_same = {
        let mut a = Vec::new();
        let mut b = Vec::new();
        walk(&t.nodes, None, &mut a, 0);
        walk(&d.nodes, None, &mut b, 0);
        a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| x.0.id == y.0.id && x.1 == y.1 && x.2 == y.2)
    };
    if structure_same || !d.can_restructure() {
        // Text only.
        for p in m.pts.iter().filter(|p| is_node(p.kind)) {
            let (Some(new), Some(old)) = (find(&d.nodes, p.id), find(&t.nodes, p.id)) else { continue };
            if new.text != old.text {
                edits.push((p.el.range(), node_pt_xml(src, p.el, new, true)));
            }
        }
        return apply_edits(src, edits);
    }
    let (Some(pt_lst), Some(cxn_lst)) = (child(m.root, "ptLst"), child(m.root, "cxnLst")) else { return src.to_string() };
    let dgm = prefix(src, pt_lst);
    let pts: HashMap<&str, &Pt> = m.pts.iter().map(|p| (p.id, p)).collect();
    let orig_ids: HashSet<&str> = m.pts.iter().map(|p| p.id).collect();
    let doc_id = m.pts.iter().find(|p| p.kind == "doc").map(|p| p.id).unwrap_or_default();
    let pres_id = m.cxns.iter().find_map(|c| c.el.attribute("presId")).unwrap_or(&t.layout).to_string();
    let (mt, n) = (t.nodes.len(), d.nodes.len());
    let trans_count = |count: usize| if t.wraps() { count } else { count.saturating_sub(1) };

    // Ids of the points that belong to the loaded nodes' units (rebuilt below).
    let mut unit_assocs: HashSet<String> = HashSet::new();
    let mut all = Vec::new();
    walk(&t.nodes, None, &mut all, 0);
    for (node, ..) in &all {
        unit_assocs.insert(node.id.clone());
        if let Some(l) = t.links.get(&node.id) {
            unit_assocs.insert(l.sib_trans.clone());
            unit_assocs.insert(l.par_trans.clone());
        }
    }
    let rebuilt_pres: HashSet<&str> = t.pres.iter().filter(|p| unit_assocs.contains(&p.assoc)).map(|p| p.id.as_str()).collect();

    let mut out_pts = String::new();
    let mut out_cxns = String::new();
    // Data points: the document, then every node with its transitions.
    if let Some(p) = pts.get(doc_id) {
        out_pts.push_str(&src[p.el.range()]);
    }
    let mut nodes = Vec::new();
    walk(&d.nodes, None, &mut nodes, 0);
    // Formatting for new nodes: a loaded node at the same depth (or the top level).
    let mut by_depth: Vec<&str> = Vec::new();
    for (node, _, _, depth) in &all {
        if by_depth.len() <= *depth {
            by_depth.push(&node.id);
        }
    }
    let links_of = |id: &str| t.links.get(id).cloned().unwrap_or_else(|| Links { cxn: derived_id(id, "cxn"), par_trans: derived_id(id, "parTrans"), sib_trans: derived_id(id, "sibTrans") });
    for (node, parent, ord, depth) in &nodes {
        let like_id = match parent {
            None => t.nodes[template_index(*ord, n, mt)].id.as_str(),
            Some(_) => by_depth.get(*depth).or(by_depth.last()).copied().unwrap_or(&t.nodes[0].id),
        };
        let own = pts.get(node.id.as_str());
        let like = own.or_else(|| pts.get(like_id));
        if let Some(like) = like {
            let changed = own.is_none() || find(&t.nodes, &node.id).is_none_or(|old| old.text != node.text);
            out_pts.push_str(&if changed { node_pt_xml(src, like.el, node, own.is_some()) } else { src[like.el.range()].to_string() });
        }
        let l = links_of(&node.id);
        let tl = links_of(like_id);
        for (id, tpl, kind) in [(&l.par_trans, &tl.par_trans, "parTrans"), (&l.sib_trans, &tl.sib_trans, "sibTrans")] {
            if let Some(p) = pts.get(id.as_str()) {
                out_pts.push_str(&src[p.el.range()]);
            } else if let Some(p) = pts.get(tpl.as_str()) {
                out_pts.push_str(&rewrite(src, p.el, vec![set_attr(src, p.el, "modelId", id), set_attr(src, p.el, "cxnId", &l.cxn)]));
            } else {
                out_pts.push_str(&format!(r#"<{dgm}pt modelId="{}" type="{kind}" cxnId="{}"><{dgm}prSet/><{dgm}spPr/></{dgm}pt>"#, esc(id), esc(&l.cxn)));
            }
        }
        // Parent connection.
        let parent_id = parent.unwrap_or(doc_id);
        match m.cxns.iter().find(|c| c.kind == "parOf" && c.dest == node.id) {
            Some(c) => out_cxns.push_str(&rewrite(src, c.el, vec![set_attr(src, c.el, "srcId", parent_id), set_attr(src, c.el, "srcOrd", &ord.to_string())])),
            None => out_cxns.push_str(&format!(
                r#"<{dgm}cxn modelId="{}" srcId="{}" destId="{}" srcOrd="{ord}" destOrd="0" parTransId="{}" sibTransId="{}"/>"#,
                esc(&l.cxn),
                esc(parent_id),
                esc(&node.id),
                esc(&l.par_trans),
                esc(&l.sib_trans)
            )),
        }
    }
    // Points of the layout as a whole stay.
    for p in m.pts.iter().filter(|p| p.kind == "pres" && !rebuilt_pres.contains(p.id)) {
        out_pts.push_str(&src[p.el.range()]);
    }
    let rebuilt = |id: &str| rebuilt_pres.contains(id);
    if d.arrangement() == Arrangement::Tree {
        let ctx = TreeCtx { src, m: &m, t, pts: &pts, dgm, pres_id: &pres_id, all: &all };
        tree_pres(&ctx, &d.nodes, &mut out_pts, &mut out_cxns);
    } else {
    // Presentation points of each node and transition, and what they show.
    let mut mapped: HashMap<(usize, usize), String> = HashMap::new(); // (template point, position) → id
    let mut block: Vec<(usize, usize, String)> = Vec::new(); // (template point, position, id), in order
    for (j, node) in d.nodes.iter().enumerate() {
        let (unit, trans) = t.plan(&node.id, j, n);
        let tu = template_index(j, n, mt);
        let tt = if j + 1 == n { mt - 1 } else { j.min(mt.saturating_sub(2)) };
        let sib = links_of(&node.id).sib_trans;
        for (is_unit, list, assoc, tpl_pos, count, new_count) in [(true, &unit, node.id.as_str(), tu, mt, n), (false, &trans, sib.as_str(), tt, trans_count(mt), trans_count(n))] {
            for (id, tp) in list.iter() {
                let tpl = &t.pres[*tp];
                let Some(el) = pts.get(id.as_str()).or_else(|| pts.get(tpl.id.as_str())).map(|p| p.el) else { continue };
                let mut edits = Vec::new();
                if !orig_ids.contains(id.as_str()) {
                    edits.push(set_attr(src, el, "modelId", id));
                }
                if let Some(set) = child(el, "prSet") {
                    edits.push(set_attr(src, set, "presAssocID", assoc));
                    let attr = |n: &str| set.attribute(n).and_then(|v| v.parse::<usize>().ok());
                    if attr("presStyleCnt") == Some(count) && attr("presStyleIdx").is_some() {
                        edits.push(set_attr(src, set, "presStyleIdx", &j.to_string()));
                        edits.push(set_attr(src, set, "presStyleCnt", &new_count.to_string()));
                    }
                }
                out_pts.push_str(&rewrite(src, el, edits));
                mapped.insert((*tp, j), id.clone());
                block.push((*tp, j, id.clone()));
                // What it shows: its node, the node's descendants, or the transition.
                let tpl_node = &t.nodes[tpl_pos].id;
                let tpl_sib = links_of(tpl_node).sib_trans;
                let (shows_own, shows_desc) = t.shows(*tp);
                let tpl_cxns: Vec<&Cxn> = m.cxns.iter().filter(|c| c.kind == "presOf" && c.dest == tpl.id).collect();
                let mut emit = |src_id: &str, src_ord: u32, dest_ord: usize| {
                    out_cxns.push_str(&format!(
                        r#"<{dgm}cxn modelId="{}" type="presOf" srcId="{}" destId="{}" srcOrd="{src_ord}" destOrd="{dest_ord}" presId="{}"/>"#,
                        esc(&derived_id(src_id, id)),
                        esc(src_id),
                        esc(id),
                        esc(&pres_id)
                    ));
                };
                let ord_of = |pick: &dyn Fn(&Cxn) -> bool| tpl_cxns.iter().find(|c| pick(c)).map_or(0, |c| c.src_ord);
                let mut dest_ord = 0;
                if is_unit {
                    if shows_own {
                        emit(&node.id, ord_of(&|c| c.src == tpl_node.as_str()), dest_ord);
                        dest_ord += 1;
                    }
                    if shows_desc {
                        let below_ord = ord_of(&|c| c.src != tpl_node.as_str());
                        let mut below = Vec::new();
                        walk(&node.children, None, &mut below, 0);
                        for (b, ..) in below {
                            emit(&b.id, below_ord, dest_ord);
                            dest_ord += 1;
                        }
                    }
                } else if let Some(c) = tpl_cxns.iter().find(|c| c.src == tpl_sib) {
                    emit(&sib, c.src_ord, c.dest_ord as usize);
                }
            }
        }
    }
    // Parent-child links between presentation points.
    let tpl_index: HashMap<&str, usize> = t.pres.iter().enumerate().map(|(i, p)| (p.id.as_str(), i)).collect();
    // Which position's copy of a template point a link from `src` goes to.
    let mut parents: HashMap<&str, Vec<&Cxn>> = HashMap::new();
    for c in m.cxns.iter().filter(|c| c.kind == "presParOf") {
        parents.entry(c.src).or_default().push(c);
    }
    for v in parents.values_mut() {
        v.sort_by_key(|c| c.src_ord);
    }
    let presparof = |src_id: &str, dest_id: &str, ord: usize| format!(r#"<{dgm}cxn modelId="{}" type="presParOf" srcId="{}" destId="{}" srcOrd="{ord}" destOrd="0" presId="{}"/>"#, esc(&derived_id(src_id, dest_id)), esc(src_id), esc(dest_id), esc(&pres_id));
    let mut parent_ids: Vec<&str> = parents.keys().copied().collect();
    parent_ids.sort_by_key(|id| m.pts.iter().position(|p| p.id == *id));
    for parent in parent_ids {
        let list = &parents[parent];
        if rebuilt(parent) {
            // Inside a node's unit: repeat for each copy.
            let Some(&tp) = tpl_index.get(parent) else { continue };
            for (&(_, j), src_id) in mapped.iter().filter(|((p, _), _)| *p == tp) {
                for c in list {
                    if let Some(dest) = tpl_index.get(c.dest).and_then(|d| mapped.get(&(*d, j))) {
                        out_cxns.push_str(&presparof(src_id, dest, c.src_ord as usize));
                    }
                }
            }
            continue;
        }
        // A point of the whole layout: its node-unit children are replaced by the new sequence.
        let mut ord = 0;
        let mut placed = false;
        for c in list {
            if !rebuilt(c.dest) {
                out_cxns.push_str(&rewrite(src, c.el, vec![set_attr(src, c.el, "srcOrd", &ord.to_string())]));
                ord += 1;
                continue;
            }
            if placed {
                continue;
            }
            placed = true;
            let children: HashSet<usize> = list.iter().filter_map(|c| tpl_index.get(c.dest).copied()).collect();
            for (_, _, id) in block.iter().filter(|(tp, ..)| children.contains(tp)) {
                out_cxns.push_str(&presparof(parent, id, ord));
                ord += 1;
            }
        }
    }
    }
    // Remaining links of the whole layout (the document's `presOf`), and kinds we don't edit.
    for c in m.cxns.iter().filter(|c| match c.kind {
        "presOf" => !rebuilt(c.dest) && !unit_assocs.contains(c.src),
        "parOf" | "presParOf" => false,
        _ => true,
    }) {
        out_cxns.push_str(&src[c.el.range()]);
    }
    edits.push((pt_lst.range(), format!("<{}>{out_pts}</{}>", qname(src, pt_lst), qname(src, pt_lst))));
    edits.push((cxn_lst.range(), format!("<{}>{out_cxns}</{}>", qname(src, cxn_lst), qname(src, cxn_lst))));
    apply_edits(src, edits)
}

/// What writing an org chart's presentation points needs.
struct TreeCtx<'a, 'i> {
    src: &'a str,
    m: &'a Model<'a, 'i>,
    t: &'a Template,
    pts: &'a HashMap<&'a str, &'a Pt<'a, 'i>>,
    dgm: &'a str,
    pres_id: &'a str,
    /// The loaded nodes with their depths (see [`walk`]).
    all: &'a [(&'a DNode, Option<&'a str>, usize, usize)],
}

/// A node's presentation points in the loaded data: its own (`assoc` = the
/// node) and the connector's (`assoc` = its `parTrans`), in file order.
struct Unit<'a> {
    ids: Vec<&'a str>,
    /// The point of the node's box (hierRoot) and of the connector (Name37); both hang off a point outside the unit.
    top: &'a str,
    /// None for the root, which has no connector.
    trans: Option<&'a str>,
    /// Where the children's points hang: the box's second child.
    container: &'a str,
}

fn tree_unit<'a>(ctx: &TreeCtx<'a, '_>, node: &str) -> Option<Unit<'a>> {
    let t = ctx.t;
    let par_trans = t.links.get(node)?.par_trans.as_str();
    let ids: Vec<&str> = t.pres.iter().filter(|p| p.assoc == node || p.assoc == par_trans).map(|p| p.id.as_str()).collect();
    let inside: HashSet<&str> = ids.iter().copied().collect();
    let outside_parent = |id: &str| ctx.m.cxns.iter().any(|c| c.kind == "presParOf" && c.dest == id && !inside.contains(c.src));
    let assoc = |id: &str| t.pres.iter().find(|p| p.id == id).map_or("", |p| p.assoc.as_str());
    let top = *ids.iter().find(|id| assoc(id) == node && outside_parent(id))?;
    let trans = ids.iter().find(|id| assoc(id) == par_trans && outside_parent(id)).copied();
    let mut kids: Vec<&Cxn> = ctx.m.cxns.iter().filter(|c| c.kind == "presParOf" && c.src == top).collect();
    kids.sort_by_key(|c| c.src_ord);
    let container = kids.get(1)?.dest;
    Some(Unit { ids, top, trans, container })
}

/// Writes the presentation points of an org chart: each node's group of
/// points (box, its parts, children container, connector) repeats from the
/// node it looks like, and the groups are hung under their parents again.
fn tree_pres(ctx: &TreeCtx, nodes: &[DNode], out_pts: &mut String, out_cxns: &mut String) {
    let TreeCtx { src, m, t, pts, dgm, pres_id, all } = ctx;
    let mut list = Vec::new();
    walk(nodes, None, &mut list, 0);
    let links_of = |id: &str| t.links.get(id).cloned().unwrap_or_else(|| Links { cxn: derived_id(id, "cxn"), par_trans: derived_id(id, "parTrans"), sib_trans: derived_id(id, "sibTrans") });
    let depth_of: HashMap<&str, usize> = all.iter().map(|(n, .., d)| (n.id.as_str(), *d)).collect();
    let loaded_count = |depth: usize| all.iter().filter(|a| a.3 == depth).count();
    // Position of each node among those at its depth, and how many there are.
    let mut at_depth: HashMap<usize, usize> = HashMap::new();
    let mut slot: HashMap<&str, (usize, usize)> = HashMap::new();
    for (n, .., depth) in &list {
        let i = at_depth.entry(*depth).or_default();
        slot.insert(n.id.as_str(), (*i, 0));
        *i += 1;
    }
    for (n, .., depth) in &list {
        slot.get_mut(n.id.as_str()).expect("listed").1 = at_depth[depth];
    }
    let mut units: HashMap<&str, Unit> = HashMap::new();
    for (n, ..) in all.iter() {
        if let Some(u) = tree_unit(ctx, &n.id) {
            units.insert(n.id.as_str(), u);
        }
    }
    // Ids a node's points get: its own, or copies derived from the node it looks like.
    let mapped = |id: &str, node: &DNode, own: bool| if own { id.to_string() } else { derived_id(&node.id, id) };
    // Children of each node, in order, to number what hangs under a container.
    let mut child_index: HashMap<&str, usize> = HashMap::new();
    for (n, parent, ord, _) in &list {
        if let Some(p) = parent {
            let _ = p;
            child_index.insert(n.id.as_str(), *ord);
        }
    }
    let look_of = |node: &DNode, depth: usize| t.tree_look(&node.id, depth).map(|(l, own)| (l.to_string(), own));
    for (node, parent, ord, depth) in &list {
        let Some((look, own)) = look_of(node, *depth) else { continue };
        let Some(unit) = units.get(look.as_str()) else { continue };
        let l_links = links_of(&look);
        let n_links = links_of(&node.id);
        let lt = depth_of.get(look.as_str()).copied().unwrap_or(*depth);
        let (idx, cnt) = slot[node.id.as_str()];
        let new_id = |p: &str| mapped(p, node, own);
        // Points.
        for &p in &unit.ids {
            let Some(pt) = pts.get(p) else { continue };
            let mut edits = Vec::new();
            if !own {
                edits.push(set_attr(src, pt.el, "modelId", &new_id(p)));
            }
            if let Some(set) = child(pt.el, "prSet") {
                let assoc = set.attribute("presAssocID").unwrap_or_default();
                if !own {
                    edits.push(set_attr(src, set, "presAssocID", if assoc == look { &node.id } else { &n_links.par_trans }));
                    // Offsets belong to the node they were set on; sizes carry over.
                    for a in set.attributes() {
                        if a.name().starts_with("cust") && !a.name().starts_with("custScale") {
                            edits.extend(remove_attr(set, a.name()));
                        }
                    }
                }
                let attr = |n: &str| set.attribute(n).and_then(|v| v.parse::<usize>().ok());
                if attr("presStyleIdx").is_some() && attr("presStyleCnt") == Some(loaded_count(lt)) && loaded_count(lt) > 0 {
                    edits.push(set_attr(src, set, "presStyleIdx", &idx.to_string()));
                    edits.push(set_attr(src, set, "presStyleCnt", &cnt.to_string()));
                }
            }
            out_pts.push_str(&rewrite(src, pt.el, edits));
        }
        let inside: HashSet<&str> = unit.ids.iter().copied().collect();
        let unit_src = |s: &str| -> Option<String> {
            if s == look {
                Some(node.id.clone())
            } else if s == l_links.par_trans {
                Some(n_links.par_trans.clone())
            } else {
                None
            }
        };
        // What the points show.
        for c in m.cxns.iter().filter(|c| c.kind == "presOf" && inside.contains(c.dest)) {
            let Some(s) = unit_src(c.src) else { continue };
            if own {
                out_cxns.push_str(&src[c.el.range()]);
            } else {
                let dest = new_id(c.dest);
                out_cxns.push_str(&format!(
                    r#"<{dgm}cxn modelId="{}" type="presOf" srcId="{}" destId="{}" srcOrd="{}" destOrd="{}" presId="{}"/>"#,
                    esc(&derived_id(&s, &dest)),
                    esc(&s),
                    esc(&dest),
                    c.src_ord,
                    c.dest_ord,
                    esc(pres_id)
                ));
            }
        }
        // How the points nest inside the node's group.
        let nest = |parent_id: &str, dest_id: &str, ord: usize| {
            format!(r#"<{dgm}cxn modelId="{}" type="presParOf" srcId="{}" destId="{}" srcOrd="{ord}" destOrd="0" presId="{}"/>"#, esc(&derived_id(parent_id, dest_id)), esc(parent_id), esc(dest_id), esc(pres_id))
        };
        for c in m.cxns.iter().filter(|c| c.kind == "presParOf" && inside.contains(c.src) && inside.contains(c.dest)) {
            if own {
                out_cxns.push_str(&src[c.el.range()]);
            } else {
                out_cxns.push_str(&nest(&new_id(c.src), &new_id(c.dest), c.src_ord as usize));
            }
        }
        // Where the group hangs: under the parent's container, connector first.
        match parent {
            None => {
                for c in m.cxns.iter().filter(|c| c.kind == "presParOf" && c.dest == unit.top && !inside.contains(c.src)) {
                    out_cxns.push_str(&src[c.el.range()]);
                }
            }
            Some(pid) => {
                let Some(pnode) = list.iter().find(|x| x.0.id == *pid) else { continue };
                let Some((plook, pown)) = look_of(pnode.0, depth - 1) else { continue };
                let Some(punit) = units.get(plook.as_str()) else { continue };
                let container = mapped(punit.container, pnode.0, pown);
                let trans = unit.trans.map(new_id);
                if let Some(trans) = &trans {
                    out_cxns.push_str(&nest(&container, trans, 2 * ord));
                }
                out_cxns.push_str(&nest(&container, &new_id(unit.top), 2 * ord + 1));
            }
        }
    }
}

/// The drawing part for `shapes` (the diagram's current drawing), stretched by
/// `sx` × `sy` to the frame's size.
pub(crate) fn drawing_xml(d: &Diagram, shapes: &[Shape], sx: f64, sy: f64) -> String {
    let t = &d.template.0;
    let src: &str = &t.drawing_xml;
    let Ok(doc) = Document::parse(src) else { return src.to_string() };
    let sps: Vec<Node> = doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "sp").collect();
    let (Some(first), Some(last)) = (sps.first(), sps.last()) else { return src.to_string() };
    let scale = |x: &Xfrm| Xfrm { x: (x.x as f64 * sx).round() as Emu, y: (x.y as f64 * sy).round() as Emu, w: (x.w as f64 * sx).round() as Emu, h: (x.h as f64 * sy).round() as Emu, ..*x };
    let mut out = String::new();
    for (shape, drawn) in shapes.iter().zip(&d.drawn) {
        let (Some(sp), Some(tpl)) = (sps.get(drawn.template), t.shapes.get(drawn.template)) else { continue };
        let mut edits = vec![set_attr(src, *sp, "modelId", &drawn.model_id)];
        let new = scale(&shape.xfrm);
        if let Some(x) = sp.descendants().find(|n| n.tag_name().name() == "xfrm" && n.parent().is_some_and(|p| p.tag_name().name() == "spPr")) {
            edits.push((x.range(), xfrm_xml(qname(src, x), &new)));
        }
        // Connectors are redrawn between their boxes.
        if shape.geometry != tpl.geometry
            && let Some(geom) = sp.descendants().find(|n| n.tag_name().name() == "custGeom" && n.parent().is_some_and(|p| p.tag_name().name() == "spPr"))
        {
            edits.push((geom.range(), crate::writer::geometry_xml(&shape.geometry)));
        }
        // The text frame moves and scales with the shape.
        if let Some(tx) = child(*sp, "txXfrm")
            && let Some(old) = crate::parse_xfrm(tx)
        {
            let (fx, fy) = (new.w as f64 / tpl.xfrm.w.max(1) as f64, new.h as f64 / tpl.xfrm.h.max(1) as f64);
            let moved = Xfrm {
                x: new.x + ((old.x - tpl.xfrm.x) as f64 * fx).round() as Emu,
                y: new.y + ((old.y - tpl.xfrm.y) as f64 * fy).round() as Emu,
                w: (old.w as f64 * fx).round() as Emu,
                h: (old.h as f64 * fy).round() as Emu,
                rot: old.rot + (shape.xfrm.rot - tpl.xfrm.rot),
                ..old
            };
            edits.push((tx.range(), xfrm_xml(qname(src, tx), &moved)));
        }
        if shape.text != tpl.text
            && let (Some(tx), Some(body)) = (child(*sp, "txBody"), &shape.text)
        {
            let kept: String = tx.children().filter(|c| c.is_element() && c.tag_name().name() != "p").map(|c| &src[c.range()]).collect();
            let no_links: &mut LinkRels = &mut |_| String::new();
            edits.push((tx.range(), format!("<{}>{kept}{}</{}>", qname(src, tx), paragraphs_xml(body, no_links), qname(src, tx))));
        }
        out.push_str(&rewrite(src, *sp, edits));
    }
    apply_edits(src, vec![(first.range().start..last.range().end, out)])
}
