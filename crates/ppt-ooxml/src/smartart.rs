//! Inserting a new SmartArt: a "starter kit" (the five parts of one diagram of
//! the chosen layout, kept with the program) goes into the deck's package, and
//! the diagram is then read back through the ordinary loader, so editing and
//! saving it works as for any SmartArt in a file.

use std::collections::HashSet;
use std::io::{Cursor, Write};
use std::sync::Arc;

use ppt_model::{Presentation, Shape, ShapeKind};
use roxmltree::Document;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::writer::{apply_edits, esc};
use crate::{Error, parse_pptx_owned};

/// One layout that can be inserted.
struct Starter {
    kind: &'static str,
    data: &'static str,
    layout: &'static str,
    quick_style: &'static str,
    colors: &'static str,
    drawing: &'static str,
    /// Size of the graphic frame the drawing was made for (EMU).
    frame: (i64, i64),
    /// Top-level nodes a new one starts with.
    top: usize,
    /// Deepest level kept (0 is the top level).
    depth: usize,
    /// Children each node below the top level keeps.
    children: usize,
}

macro_rules! starter {
    ($kind:literal, $frame:expr, $top:expr, $depth:expr, $children:expr) => {
        Starter {
            kind: $kind,
            data: include_str!(concat!("../assets/smartart/", $kind, "/data.xml")),
            layout: include_str!(concat!("../assets/smartart/", $kind, "/layout.xml")),
            quick_style: include_str!(concat!("../assets/smartart/", $kind, "/quickStyle.xml")),
            colors: include_str!(concat!("../assets/smartart/", $kind, "/colors.xml")),
            drawing: include_str!(concat!("../assets/smartart/", $kind, "/drawing.xml")),
            frame: $frame,
            top: $top,
            depth: $depth,
            children: $children,
        }
    };
}

const STARTERS: &[Starter] = &[
    starter!("vList2", (2528664, 1786324), 3, 0, 0),
    starter!("chevron1", (7367146, 292990), 3, 0, 0),
    starter!("hList1", (8128000, 2718148), 3, 1, 2),
    starter!("cycle2", (4234921, 2134352), 4, 0, 0),
    starter!("cycle3", (8128000, 5418667), 4, 0, 0),
    starter!("cycle5", (8128000, 5418667), 4, 0, 0),
    starter!("cycle8", (9519557, 5400861), 4, 0, 0),
    starter!("pyramid1", (6636512, 5418667), 3, 0, 0),
    starter!("orgChart1", (2409189, 2743845), 1, 2, 2),
    starter!("hierarchy1", (10023676, 3087848), 1, 2, 2),
    starter!("hierarchy2", (11098574, 5772954), 1, 2, 2),
    starter!("process1", (10417215, 2735376), 3, 0, 0),
    starter!("process2", (5594773, 6362749), 3, 0, 0),
    starter!("process3", (3034878, 1287921), 3, 1, 2),
    starter!("hChevron3", (11087357, 1250423), 3, 0, 0),
    starter!("chevron2", (8292618, 5919250), 3, 1, 2),
    starter!("hProcess9", (4179378, 709735), 3, 0, 0),
];

/// Layout ids that can be inserted, as [`new_smartart`] takes them.
pub fn smartart_kinds() -> Vec<&'static str> {
    STARTERS.iter().map(|s| s.kind).collect()
}

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const DRAWING_REL: &str = "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing";
const CT: &str = "application/vnd.openxmlformats-officedocument.drawingml";

fn zip_err(e: impl ToString) -> Error {
    Error::Zip(e.to_string())
}

/// `xml` with every length in a SmartArt drawing multiplied by `k`: frames,
/// custom paths, insets, line widths and font sizes.
fn scale_drawing(xml: &str, k: f64) -> String {
    let Ok(doc) = Document::parse(xml) else { return xml.to_string() };
    let scaled = |v: &str, min: f64| ((v.parse::<f64>().unwrap_or(0.0) * k).round()).max(min).to_string();
    let mut edits = Vec::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let parent = n.parent().map(|p| p.tag_name().name()).unwrap_or_default();
        let attrs: &[(&str, f64)] = match (n.tag_name().name(), parent) {
            ("off", "xfrm" | "txXfrm") => &[("x", f64::MIN), ("y", f64::MIN)],
            ("ext", "xfrm" | "txXfrm") => &[("cx", 0.0), ("cy", 0.0)],
            ("pt", _) => &[("x", f64::MIN), ("y", f64::MIN)],
            ("path", _) => &[("w", 0.0), ("h", 0.0)],
            ("bodyPr", _) => &[("lIns", 0.0), ("tIns", 0.0), ("rIns", 0.0), ("bIns", 0.0)],
            ("rPr" | "endParaRPr" | "defRPr", _) => &[("sz", 100.0)],
            ("ln", _) => &[("w", 0.0)],
            _ => &[],
        };
        for (name, min) in attrs {
            if let Some(a) = n.attribute_node(*name) {
                edits.push((a.range_value(), scaled(a.value(), *min)));
            }
        }
    }
    apply_edits(xml, edits)
}

/// `data` with the drawing's relationship id (`dsp:dataModelExt`) set to `rel`.
fn point_at_drawing(data: &str, rel: &str) -> String {
    let Ok(doc) = Document::parse(data) else { return data.to_string() };
    let edits = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "dataModelExt")
        .filter_map(|n| n.attribute_node("relId"))
        .map(|a| (a.range_value(), rel.to_string()))
        .collect();
    apply_edits(data, edits)
}

/// Rewrites `package` with `replace` (part → new bytes) and `add` parts.
fn rewrite_zip(package: &[u8], replace: &[(String, Vec<u8>)], add: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let mut zip = ZipArchive::new(Cursor::new(package)).map_err(zip_err)?;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for i in 0..zip.len() {
        let name = zip.by_index_raw(i).map_err(zip_err)?.name().map_err(zip_err)?.into_owned();
        match replace.iter().find(|(n, _)| *n == name) {
            Some((_, bytes)) => {
                writer.start_file(name, opts).map_err(zip_err)?;
                writer.write_all(bytes).map_err(zip_err)?;
            }
            None => writer.raw_copy_file(zip.by_index_raw(i).map_err(zip_err)?).map_err(zip_err)?,
        }
    }
    for (name, bytes) in add {
        writer.start_file(name.clone(), opts).map_err(zip_err)?;
        writer.write_all(bytes).map_err(zip_err)?;
    }
    Ok(writer.finish().map_err(zip_err)?.into_inner())
}

fn read_part(package: &[u8], name: &str) -> Result<String, Error> {
    use std::io::Read;
    let mut zip = ZipArchive::new(Cursor::new(package)).map_err(zip_err)?;
    let mut s = String::new();
    zip.by_name(name).map_err(|_| Error::Missing(name.to_string()))?.read_to_string(&mut s).map_err(zip_err)?;
    Ok(s)
}

/// A relationships part with `rels` (id, type, target).
fn rels_xml(rels: &[(&str, &str, String)]) -> String {
    let items: String = rels.iter().map(|(id, kind, target)| format!(r#"<Relationship Id="{id}" Type="{kind}" Target="{}"/>"#, esc(target))).collect();
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{items}</Relationships>"#)
}

/// What the new diagram's texts look like: `top` for top-level nodes (`root`
/// for an org chart's one) and `child` for the ones below, each with `{}`
/// standing for its number.
pub struct Labels<'a> {
    /// An org chart's root.
    pub root: &'a str,
    pub top: &'a str,
    pub child: &'a str,
}

/// A new SmartArt of layout `kind` (see [`smartart_kinds`]) for the end of a
/// slide, centered on a slide of `slide` size, together with `pres`'s package
/// holding its parts. The caller gives the shape an id and adds both to the deck.
pub fn new_smartart(pres: &Presentation, kind: &str, slide: (i64, i64), labels: &Labels) -> Result<(Shape, Arc<Vec<u8>>), Error> {
    let starter = STARTERS.iter().find(|s| s.kind == kind).ok_or_else(|| Error::Missing(format!("SmartArt layout {kind}")))?;
    let package = pres.package.as_ref().ok_or_else(|| Error::Missing("source package".into()))?;
    let names: HashSet<String> = ZipArchive::new(Cursor::new(package.as_slice())).map_err(zip_err)?.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect();

    // Size: as large as fits within most of the slide, drawing and all.
    let k = (slide.0 as f64 * 0.8 / starter.frame.0 as f64).min(slide.1 as f64 * 0.7 / starter.frame.1 as f64).clamp(0.5, 2.5);
    let (w, h) = ((starter.frame.0 as f64 * k).round() as i64, (starter.frame.1 as f64 * k).round() as i64);
    let (x, y) = ((slide.0 - w) / 2, (slide.1 - h) / 2);

    // Part names nobody uses yet.
    let n = (1..).find(|n| ["data", "layout", "quickStyle", "colors", "drawing"].iter().all(|p| !names.contains(&format!("ppt/diagrams/{p}{n}.xml")))).expect("unbounded");
    let part = |p: &str| format!("ppt/diagrams/{p}{n}.xml");
    let sld = (1..).map(|i| 9000 + i).find(|i| !names.contains(&format!("ppt/slides/slide{i}.xml"))).expect("unbounded");
    let sld_part = format!("ppt/slides/slide{sld}.xml");
    let parts: Vec<(String, Vec<u8>)> = vec![
        (part("data"), point_at_drawing(starter.data, "rId6").into_bytes()),
        (part("layout"), starter.layout.as_bytes().to_vec()),
        (part("quickStyle"), starter.quick_style.as_bytes().to_vec()),
        (part("colors"), starter.colors.as_bytes().to_vec()),
        (part("drawing"), scale_drawing(starter.drawing, k).into_bytes()),
    ];
    let overrides = [
        (part("data"), format!("{CT}.diagramData+xml")),
        (part("layout"), format!("{CT}.diagramLayout+xml")),
        (part("quickStyle"), format!("{CT}.diagramStyle+xml")),
        (part("colors"), format!("{CT}.diagramColors+xml")),
        (part("drawing"), "application/vnd.ms-office.drawingml.diagramDrawing+xml".to_string()),
    ];
    let content_types = read_part(package, "[Content_Types].xml")?;
    let add_overrides = |extra: &[(String, String)]| -> String {
        let items: String = extra.iter().map(|(p, t)| format!(r#"<Override PartName="/{p}" ContentType="{t}"/>"#)).collect();
        content_types.replace("</Types>", &format!("{items}</Types>"))
    };

    // The deck with the parts, and a slide holding the diagram, to read it back from.
    let layout = names.iter().filter(|n| n.starts_with("ppt/slideLayouts/slideLayout") && n.ends_with(".xml")).min().ok_or_else(|| Error::Missing("a slide layout".into()))?;
    let slide_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{REL}" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="2" name="SmartArt"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId2" r:lo="rId3" r:qs="rId4" r:cs="rId5"/></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>"#
    );
    let rel = |t: &str| format!("{REL}/{t}");
    let slide_rels = rels_xml(&[
        ("rId1", &rel("slideLayout"), format!("../slideLayouts/{}", layout.rsplit('/').next().unwrap_or_default())),
        ("rId2", &rel("diagramData"), format!("../diagrams/data{n}.xml")),
        ("rId3", &rel("diagramLayout"), format!("../diagrams/layout{n}.xml")),
        ("rId4", &rel("diagramQuickStyle"), format!("../diagrams/quickStyle{n}.xml")),
        ("rId5", &rel("diagramColors"), format!("../diagrams/colors{n}.xml")),
        ("rId6", DRAWING_REL, format!("../diagrams/drawing{n}.xml")),
    ]);
    let pres_part = "ppt/presentation.xml";
    let pres_xml = read_part(package, pres_part)?;
    let pres_rels = read_part(package, "ppt/_rels/presentation.xml.rels")?;
    let rid = (1..).map(|i| format!("rIdSmartArt{i}")).find(|r| !pres_rels.contains(r)).expect("unbounded");
    let sld_id = 100_000 + sld;
    let pres_xml = if pres_xml.contains("</p:sldIdLst>") {
        pres_xml.replace("</p:sldIdLst>", &format!(r#"<p:sldId id="{sld_id}" r:id="{rid}"/></p:sldIdLst>"#))
    } else {
        return Err(Error::Missing("a slide list".into()));
    };
    let pres_rels = pres_rels.replace("</Relationships>", &format!(r#"<Relationship Id="{rid}" Type="{}" Target="slides/slide{sld}.xml"/></Relationships>"#, rel("slide")));
    let mut with_slide = overrides.to_vec();
    with_slide.push((sld_part.clone(), "application/vnd.openxmlformats-officedocument.presentationml.slide+xml".to_string()));
    let mut extra = parts.clone();
    extra.push((sld_part.clone(), slide_xml.into_bytes()));
    extra.push((format!("ppt/slides/_rels/slide{sld}.xml.rels"), slide_rels.into_bytes()));
    let probe = rewrite_zip(
        package,
        &[("[Content_Types].xml".to_string(), add_overrides(&with_slide).into_bytes()), (pres_part.to_string(), pres_xml.into_bytes()), ("ppt/_rels/presentation.xml.rels".to_string(), pres_rels.into_bytes())],
        &extra,
    )?;
    let read = parse_pptx_owned(probe)?;
    let mut shape = read
        .slides
        .last()
        .and_then(|s| s.shapes.iter().find(|s| matches!(s.kind, ShapeKind::Diagram { .. })))
        .cloned()
        .ok_or_else(|| Error::Missing("the new SmartArt".into()))?;
    // The kept deck holds the parts only; the shape's source is not part of any slide yet.
    let mut source = (**shape.source.as_ref().ok_or_else(|| Error::Missing("the new SmartArt's source".into()))?).clone();
    source.part = "ppt/slides/(new SmartArt).xml".into();
    shape.source = Some(Arc::new(source));
    shape.name = ppt_model::lang().pick("SmartArt 圖形", "SmartArt 图形", "SmartArt Graphic").to_string();
    fit_nodes(&mut shape, starter, labels)?;
    let kept = rewrite_zip(package, &[("[Content_Types].xml".to_string(), add_overrides(&overrides).into_bytes())], &parts)?;
    Ok((shape, Arc::new(kept)))
}

/// Gives the new diagram its first nodes: as many as the layout starts with, named by `labels`.
fn fit_nodes(shape: &mut Shape, starter: &Starter, labels: &Labels) -> Result<(), Error> {
    use ppt_model::diagram::Insert;
    let ShapeKind::Diagram { w, h, data: Some(data), .. } = &shape.kind else { return Err(Error::Missing("an editable SmartArt".into())) };
    let (w, h) = (*w, *h);
    let mut d = (**data).clone();
    // Fewer levels, then the right number of top-level nodes.
    while let Some(i) = d.rows().iter().rposition(|r| r.depth > starter.depth) {
        d.delete(i);
    }
    // Few children each.
    let surplus = |d: &ppt_model::diagram::Diagram| {
        let rows = d.rows();
        (0..rows.len()).rev().find(|&i| {
            let depth = rows[i].depth;
            depth > 0 && (0..i).rev().take_while(|&k| rows[k].depth >= depth).filter(|&k| rows[k].depth == depth).count() >= starter.children
        })
    };
    while let Some(i) = surplus(&d) {
        if d.delete(i).is_none() {
            break;
        }
    }
    let tops = |d: &ppt_model::diagram::Diagram| d.rows().iter().filter(|r| r.depth == 0).count();
    while tops(&d) > starter.top.max(1) {
        let Some(i) = d.rows().iter().rposition(|r| r.depth == 0) else { break };
        if d.delete(i).is_none() {
            break;
        }
    }
    while tops(&d) < starter.top {
        let Some(i) = d.rows().iter().rposition(|r| r.depth == 0) else { break };
        if d.insert(i, Insert::After).is_none() {
            break;
        }
    }
    // An org chart's departments are below its one root.
    if starter.top == 1 {
        while d.rows().iter().filter(|r| r.depth == 1).count() < 2 {
            if d.insert(0, Insert::Child).is_none() {
                break;
            }
        }
    }
    let (mut top, mut child) = (0, 0);
    for (i, row) in d.rows().into_iter().enumerate() {
        let (template, n) = if row.depth == 0 && starter.top > 1 || row.depth == 1 && starter.top == 1 {
            top += 1;
            (labels.top, top)
        } else if row.depth == 0 {
            (labels.root, 1)
        } else {
            child += 1;
            (labels.child, child)
        };
        d.set_text(i, &template.replace("{}", &n.max(1).to_string()));
    }
    let shapes = d.layout();
    shape.kind = ShapeKind::Diagram { shapes: Arc::new(shapes), w, h, data: Some(Arc::new(d)) };
    Ok(())
}
