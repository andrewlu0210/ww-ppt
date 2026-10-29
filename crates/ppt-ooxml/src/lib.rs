//! PPTX (PresentationML) import.
//!
//! Resolves the OOXML inheritance chain (slide → layout → master → theme) into
//! the flat [`ppt_model`] representation. Export arrives in M1.

mod chart;
mod package;
mod style;
mod table;
mod table_writer;
mod template;
mod writer;

pub use template::template_package;
pub use writer::save_pptx;

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use package::{Package, REL_NS, Rel};
use ppt_model::{
    Color, ContainerSource, Crop, CustomPath, Dash, EndKind, Gradient, GradientKind, LineEnd, Link, Paint, Shadow, Emu, Geometry, Layout as ModelLayout, Line, ParaDefaults, Paragraph, ParsedShape,
    PathCmd, Placeholder, Presentation, Run, RunStyle, Shape, ShapeKind, ShapeSource, Slide, SlideSource, TextBody, Xfrm,
};
use ppt_model::table::TableStyle;
use roxmltree::{Document, Node};
use style::{BodyProps, ColorCtx, LevelStyle, ListStyle, Theme, attr_bool, attr_f64, attr_i64, child, children, path};

#[derive(Debug)]
pub enum Error {
    Zip(String),
    Xml(String),
    Missing(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Zip(e) => write!(f, "invalid zip package: {e}"),
            Error::Xml(e) => write!(f, "invalid XML: {e}"),
            Error::Missing(p) => write!(f, "missing package part: {p}"),
        }
    }
}

impl std::error::Error for Error {}

fn parse_xml(xml: &str) -> Result<Document<'_>, Error> {
    Document::parse(xml).map_err(|e| Error::Xml(e.to_string()))
}

/// Placeholder properties defined on a layout or master.
#[derive(Clone, Debug)]
struct PhInfo {
    kind: String,
    idx: Option<u32>,
    xfrm: Option<Xfrm>,
    body: BodyProps,
    lst: ListStyle,
}

struct Master {
    /// Layout parts in the master's `p:sldLayoutIdLst` order.
    layout_parts: Vec<String>,
    ctx: ColorCtx,
    phs: Vec<PhInfo>,
    shapes: Vec<Shape>,
    bg: Option<Color>,
    title: ListStyle,
    body: ListStyle,
    other: ListStyle,
    default_text: ListStyle,
}

struct Layout {
    /// Index in [`Presentation::layouts`].
    index: usize,
    master: Rc<Master>,
    phs: Vec<PhInfo>,
    shapes: Vec<Shape>,
    bg: Option<Color>,
    show_master_shapes: bool,
}

struct Parser<'p> {
    pkg: Package<'p>,
    pres: Presentation,
    pres_xml: String,
    masters: HashMap<String, Rc<Master>>,
    /// Masters in the order they were first seen.
    master_order: Vec<String>,
    layouts: HashMap<String, Rc<Layout>>,
    /// `ppt/tableStyles.xml`, read on first use.
    table_styles_xml: Option<Option<String>>,
    /// Fill of the group being parsed, for members with `a:grpFill`.
    group_fill: Option<Paint>,
    /// Resolved table styles by (master, style id); colors depend on the master's theme.
    table_styles: HashMap<(usize, String), Option<Arc<TableStyle>>>,
}

/// Inheritance context for the shapes of one part.
struct ShapeCx<'a> {
    master: &'a Master,
    /// Placeholder definitions to inherit from, nearest first (layout, then master).
    ph_chain: Vec<&'a [PhInfo]>,
    rels: &'a HashMap<String, Rel>,
    /// Set for slides: shapes then remember their XML for saving.
    source: Option<&'a SourceCx>,
}

struct SourceCx {
    part: String,
    doc: Arc<str>,
    namespaces: Arc<Vec<(String, String)>>,
}

impl SourceCx {
    fn new(part: &str, doc: Arc<str>, root: Node) -> Self {
        let namespaces = root.namespaces().map(|ns| (ns.name().unwrap_or_default().to_string(), ns.uri().to_string())).collect();
        Self { part: part.to_string(), doc, namespaces: Arc::new(namespaces) }
    }
}

/// Child-to-parent mapping of a group shape: `x' = s * x + t`.
#[derive(Clone, Copy)]
struct GroupTf {
    sx: f64,
    sy: f64,
    tx: f64,
    ty: f64,
}

impl GroupTf {
    const IDENTITY: GroupTf = GroupTf { sx: 1.0, sy: 1.0, tx: 0.0, ty: 0.0 };

    fn then(self, outer: GroupTf) -> GroupTf {
        GroupTf { sx: self.sx * outer.sx, sy: self.sy * outer.sy, tx: self.tx * outer.sx + outer.tx, ty: self.ty * outer.sy + outer.ty }
    }

    fn apply(self, x: Xfrm) -> Xfrm {
        Xfrm {
            x: (x.x as f64 * self.sx + self.tx).round() as Emu,
            y: (x.y as f64 * self.sy + self.ty).round() as Emu,
            w: (x.w as f64 * self.sx).round() as Emu,
            h: (x.h as f64 * self.sy).round() as Emu,
            ..x
        }
    }
}

/// Parses a `.pptx` file into a [`Presentation`].
pub fn parse_pptx(bytes: &[u8]) -> Result<Presentation, Error> {
    let mut pkg = Package::open(bytes)?;
    let pres_part = pkg.rel_of_type("", "officeDocument").unwrap_or_else(|| "ppt/presentation.xml".to_string());
    let pres_xml = pkg.read_string(&pres_part)?;
    let mut parser = Parser {
        pkg,
        pres: Presentation::default(),
        pres_xml,
        masters: HashMap::new(),
        master_order: Vec::new(),
        layouts: HashMap::new(),
        table_styles_xml: None,
        table_styles: HashMap::new(),
        group_fill: None,
    };

    let slide_parts = {
        let doc = parse_xml(&parser.pres_xml)?;
        let root = doc.root_element();
        if let Some(sz) = child(root, "sldSz") {
            parser.pres.slide_width = attr_i64(sz, "cx").unwrap_or(parser.pres.slide_width);
            parser.pres.slide_height = attr_i64(sz, "cy").unwrap_or(parser.pres.slide_height);
        }
        let rels = parser.pkg.rels(&pres_part);
        child(root, "sldIdLst")
            .map(|lst| {
                children(lst, "sldId")
                    .filter_map(|s| s.attribute((REL_NS, "id")))
                    .filter_map(|id| rels.get(id).map(|r| r.target.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    parser.pres.package = Some(Arc::new(bytes.to_vec()));
    for part in slide_parts {
        let slide = parser.slide(&part)?;
        parser.pres.slides.push(slide);
    }
    // Register every layout (not only the used ones), then order them like the masters list them.
    let masters: Vec<Rc<Master>> = parser.master_order.iter().map(|m| parser.masters[m].clone()).collect();
    for master in &masters {
        for part in &master.layout_parts {
            parser.layout(part)?;
        }
    }
    let order: Vec<usize> = masters
        .iter()
        .flat_map(|m| m.layout_parts.iter())
        .filter_map(|p| parser.layouts.get(p).map(|l| l.index))
        .chain(0..parser.pres.layouts.len())
        .fold(Vec::new(), |mut acc, i| {
            if !acc.contains(&i) {
                acc.push(i);
            }
            acc
        });
    let old = std::mem::take(&mut parser.pres.layouts);
    parser.pres.layouts = order.iter().map(|&i| old[i].clone()).collect();
    for slide in parser.pres.slides.iter_mut() {
        slide.layout = slide.layout.and_then(|old_i| order.iter().position(|&i| i == old_i));
    }
    if let Some(master) = masters.first() {
        parser.pres.minor_font = master.ctx.theme.minor.clone();
        let finalize = |s: &LevelStyle| run_style(s, &master.ctx.theme);
        parser.pres.text_defaults = Some(Arc::new((0..9).map(|lvl| para_defaults(&master.default_text.level(lvl).clone(), &finalize)).collect()));
        let def = table::default_style_id(parser.table_styles_xml());
        parser.pres.table_style = parser.table_style(master, &def);
        parser.pres.link_color = master.ctx.scheme("hlink");
        parser.pres.table_styles = table::resolved_builtins(&master.ctx).into_iter().map(Arc::new).collect();
    }
    Ok(parser.pres)
}

impl Parser<'_> {
    fn master(&mut self, part: &str) -> Result<Rc<Master>, Error> {
        if let Some(m) = self.masters.get(part) {
            return Ok(m.clone());
        }
        let theme = self.pkg.rel_of_type(part, "theme").and_then(|t| self.pkg.read_string(&t).ok()).map(|x| Theme::parse(&x)).unwrap_or_default();
        let xml = self.pkg.read_string(part)?;
        let doc = parse_xml(&xml)?;
        let root = doc.root_element();
        let clr_map = child(root, "clrMap")
            .map(|m| m.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect())
            .unwrap_or_default();
        let ctx = ColorCtx { theme, clr_map };
        let tx = child(root, "txStyles");
        let pres_doc = parse_xml(&self.pres_xml)?;
        let rels = self.pkg.rels(part);
        let layout_parts = child(root, "sldLayoutIdLst")
            .map(|l| {
                children(l, "sldLayoutId")
                    .filter_map(|n| n.attribute((REL_NS, "id")))
                    .filter_map(|id| rels.get(id).map(|r| r.target.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut master = Master {
            layout_parts,
            phs: placeholders(root, &ctx),
            shapes: Vec::new(),
            bg: background(root, &ctx),
            title: ListStyle::parse(tx.and_then(|t| child(t, "titleStyle")), &ctx),
            body: ListStyle::parse(tx.and_then(|t| child(t, "bodyStyle")), &ctx),
            other: ListStyle::parse(tx.and_then(|t| child(t, "otherStyle")), &ctx),
            default_text: ListStyle::parse(child(pres_doc.root_element(), "defaultTextStyle"), &ctx),
            ctx,
        };
        let mut shapes = Vec::new();
        if let Some(tree) = path(root, &["cSld", "spTree"]) {
            let cx = ShapeCx { master: &master, ph_chain: Vec::new(), rels: &rels, source: None };
            self.sp_tree(tree, &cx, GroupTf::IDENTITY, false, &mut shapes);
        }
        master.shapes = shapes;
        let master = Rc::new(master);
        self.masters.insert(part.to_string(), master.clone());
        self.master_order.push(part.to_string());
        Ok(master)
    }

    fn layout(&mut self, part: &str) -> Result<Rc<Layout>, Error> {
        if let Some(l) = self.layouts.get(part) {
            return Ok(l.clone());
        }
        let master_part = self.pkg.rel_of_type(part, "slideMaster").ok_or_else(|| Error::Missing(format!("master of {part}")))?;
        let master = self.master(&master_part)?;
        let xml = self.pkg.read_string(part)?;
        let doc = parse_xml(&xml)?;
        let root = doc.root_element();
        let rels = self.pkg.rels(part);
        let phs = placeholders(root, &master.ctx);
        let mut shapes = Vec::new();
        let mut prototypes = Vec::new();
        if let Some(tree) = path(root, &["cSld", "spTree"]) {
            let cx = ShapeCx { master: &master, ph_chain: vec![&master.phs], rels: &rels, source: None };
            self.sp_tree(tree, &cx, GroupTf::IDENTITY, false, &mut shapes);
            // Placeholders a new slide starts with: resolved against this layout itself.
            let cx = ShapeCx { master: &master, ph_chain: vec![&phs, &master.phs], rels: &rels, source: None };
            for n in tree.children().filter(|n| n.is_element() && matches!(n.tag_name().name(), "sp" | "pic")) {
                let Some(ph) = n.children().find_map(|nv| path(nv, &["nvPr", "ph"])) else { continue };
                if matches!(ph.attribute("type"), Some("dt" | "ftr" | "sldNum" | "hdr")) {
                    continue;
                }
                if let Some(mut shape) = self.shape(n, &cx, GroupTf::IDENTITY) {
                    if let Some(body) = &mut shape.text {
                        let first = body.paragraphs.first().cloned().unwrap_or_default();
                        let end_style = first.first_style().clone();
                        body.paragraphs = vec![Paragraph { runs: Vec::new(), end_style, ..first }];
                    }
                    prototypes.push(shape);
                }
            }
        }
        let show_master_shapes = attr_bool(root, "showMasterSp").unwrap_or(true);
        let bg = background(root, &master.ctx);
        let mut background_shapes = if show_master_shapes { master.shapes.clone() } else { Vec::new() };
        background_shapes.extend(shapes.iter().cloned());
        let index = self.pres.layouts.len();
        self.pres.layouts.push(ModelLayout {
            name: path(root, &["cSld"]).and_then(|c| c.attribute("name")).unwrap_or_default().to_string(),
            placeholders: prototypes,
            background_shapes,
            background: bg.or(master.bg).or(Some(Color::WHITE)),
            part: part.to_string(),
        });
        let layout = Rc::new(Layout { index, bg, show_master_shapes, master, phs, shapes });
        self.layouts.insert(part.to_string(), layout.clone());
        Ok(layout)
    }

    fn slide(&mut self, part: &str) -> Result<Slide, Error> {
        let layout_part = self.pkg.rel_of_type(part, "slideLayout").ok_or_else(|| Error::Missing(format!("layout of {part}")))?;
        let layout = self.layout(&layout_part)?;
        let master = layout.master.clone();
        let xml: Arc<str> = Arc::from(self.pkg.read_string(part)?);
        let doc = parse_xml(&xml)?;
        let root = doc.root_element();
        let rels = self.pkg.rels(part);
        let src = SourceCx::new(part, xml.clone(), root);

        let mut slide = Slide {
            layout: Some(layout.index),
            background: background(root, &master.ctx).or(layout.bg).or(master.bg).or(Some(Color::WHITE)),
            hidden: attr_bool(root, "show") == Some(false),
            ..Default::default()
        };
        let show_master = attr_bool(root, "showMasterSp").unwrap_or(true);
        if show_master && layout.show_master_shapes {
            slide.background_shapes.extend(master.shapes.iter().cloned());
        }
        if show_master {
            slide.background_shapes.extend(layout.shapes.iter().cloned());
        }
        if let Some(tree) = path(root, &["cSld", "spTree"]) {
            let cx = ShapeCx { master: &master, ph_chain: vec![&layout.phs, &master.phs], rels: &rels, source: Some(&src) };
            let mut shapes = Vec::new();
            self.sp_tree(tree, &cx, GroupTf::IDENTITY, true, &mut shapes);
            slide.shapes = shapes;
        }
        for shape in slide.background_shapes.iter_mut() {
            shape.id = self.pres.alloc_id();
        }
        slide.source = Some(Arc::new(SlideSource {
            part: part.to_string(),
            doc: xml.clone(),
            namespaces: src.namespaces.clone(),
            shape_ids: slide.shapes.iter().map(|s| s.id).collect(),
        }));
        Ok(slide)
    }

    /// Collects drawable shapes from `p:spTree` / `p:grpSp`. On layouts and
    /// masters (`include_placeholders == false`) placeholders are prompts only
    /// and are skipped.
    fn sp_tree(&mut self, tree: Node, cx: &ShapeCx, group: GroupTf, include_placeholders: bool, out: &mut Vec<Shape>) {
        for n in tree.children().filter(|c| c.is_element()) {
            let name = n.tag_name().name();
            // The group's own properties are not shapes.
            if matches!(name, "nvGrpSpPr" | "grpSpPr" | "extLst") {
                continue;
            }
            match name {
                "sp" | "cxnSp" | "pic" => {
                    let is_ph = n.children().any(|nv| path(nv, &["nvPr", "ph"]).is_some());
                    if is_ph && !include_placeholders {
                        continue;
                    }
                    let shape = if is_hidden(n) { None } else { self.shape(n, cx, group) };
                    match shape {
                        Some(shape) => out.push(shape),
                        // Hidden or unplaceable shapes are kept (invisible) so saving preserves them.
                        None => out.extend(self.opaque(n, cx, group, "hidden")),
                    }
                }
                "grpSp" | "AlternateContent" => {
                    if is_hidden(n) {
                        out.extend(self.opaque(n, cx, group, "hidden"));
                        continue;
                    }
                    let mut inner_out = Vec::new();
                    if name == "grpSp" {
                        let inner = path(n, &["grpSpPr", "xfrm"]).map(group_tf).unwrap_or(GroupTf::IDENTITY);
                        // Members with `a:grpFill` take this group's fill (which may itself come from outside).
                        let own = match child(n, "grpSpPr").map(|p| fill_spec(p, &cx.master.ctx)).unwrap_or(FillSpec::Unset) {
                            FillSpec::Paint(p) => Some(p),
                            FillSpec::Group => self.group_fill.clone(),
                            FillSpec::None | FillSpec::Unset => None,
                        };
                        let outer = std::mem::replace(&mut self.group_fill, own);
                        self.sp_tree(n, cx, inner.then(group), include_placeholders, &mut inner_out);
                        self.group_fill = outer;
                    } else if let Some(fallback) = child(n, "Fallback") {
                        self.sp_tree(fallback, cx, group, include_placeholders, &mut inner_out);
                    }
                    if name == "grpSp" {
                        // Outer groups overwrite inner ones: members belong to the outermost group.
                        let id = self.pres.alloc_id();
                        for shape in inner_out.iter_mut() {
                            shape.group = Some(id);
                        }
                    }
                    if let Some(src) = cx.source {
                        // Children remember the outermost container so it can be saved as a unit.
                        let container = Arc::new(ContainerSource { range: n.range(), children: inner_out.len(), part: src.part.clone() });
                        for shape in inner_out.iter_mut() {
                            if let Some(s) = &shape.source {
                                let mut src = ShapeSource { container: Some(container.clone()), ..(**s).clone() };
                                src.parsed.group = shape.group;
                                shape.source = Some(Arc::new(src));
                            }
                        }
                    }
                    out.extend(inner_out);
                }
                "graphicFrame" => {
                    let uri = path(n, &["graphic", "graphicData"]).and_then(|g| g.attribute("uri")).unwrap_or_default();
                    if uri.ends_with("/table")
                        && !is_hidden(n)
                        && let Some(shape) = self.table_frame(n, cx, group)
                    {
                        out.push(shape);
                        continue;
                    }
                    let label = if uri.ends_with("/table") {
                        "表格"
                    } else if uri.ends_with("/chart") {
                        "圖表"
                    } else if uri.ends_with("/diagram") {
                        "SmartArt"
                    } else {
                        "物件"
                    };
                    let mut shape = self.opaque(n, cx, group, label);
                    if uri.ends_with("/chart")
                        && let Some(s) = &mut shape
                        && let Some(chart) = self.chart(n, cx)
                    {
                        s.kind = ShapeKind::Chart(Box::new(chart));
                        if let Some(src) = &s.source {
                            let mut src = (**src).clone();
                            src.parsed.kind = s.kind.clone();
                            s.source = Some(Arc::new(src));
                        }
                    }
                    if uri.ends_with("/diagram")
                        && let Some(s) = &mut shape
                        && let Some(shapes) = self.diagram_drawing(n, cx)
                    {
                        s.kind = ShapeKind::Diagram { shapes: Arc::new(shapes), w: s.xfrm.w, h: s.xfrm.h };
                        // Keep the parsed snapshot in step so an untouched diagram saves verbatim.
                        if let Some(src) = &s.source {
                            let mut src = (**src).clone();
                            src.parsed.kind = s.kind.clone();
                            s.source = Some(Arc::new(src));
                        }
                    }
                    out.extend(shape);
                }
                // Anything else (ink, 3D models, ...) is preserved without being drawn.
                _ => out.extend(self.opaque(n, cx, group, "hidden")),
            }
        }
    }

    /// The chart a graphic frame refers to (`c:chart r:id`).
    fn chart(&mut self, frame: Node, cx: &ShapeCx) -> Option<ppt_model::chart::Chart> {
        let id = path(frame, &["graphic", "graphicData", "chart"])?.attribute((REL_NS, "id"))?;
        let part = cx.rels.get(id)?.target.clone();
        let xml = self.pkg.read_string(&part).ok()?;
        chart::chart(&xml, &cx.master.ctx, &cx.master.ctx.theme.minor)
    }

    /// Shapes of a SmartArt's stored drawing (`dsp:drawing`), relative to its frame.
    fn diagram_drawing(&mut self, frame: Node, cx: &ShapeCx) -> Option<Vec<Shape>> {
        let src = cx.source?;
        let data_id = path(frame, &["graphic", "graphicData", "relIds"])?.attribute((REL_NS, "dm"))?;
        let data_part = cx.rels.get(data_id)?.target.clone();
        let data_xml = self.pkg.read_string(&data_part).ok()?;
        // The data model names the drawing by a relationship id of the slide.
        let drawing_id = Document::parse(&data_xml)
            .ok()?
            .descendants()
            .find(|n| n.tag_name().name() == "dataModelExt")
            .and_then(|n| n.attribute("relId").map(str::to_string));
        let drawing_part = match drawing_id.and_then(|id| cx.rels.get(&id)) {
            Some(rel) => rel.target.clone(),
            None => self.pkg.rel_of_type(&src.part, "diagramDrawing")?,
        };
        let xml = self.pkg.read_string(&drawing_part).ok()?;
        let doc = parse_xml(&xml).ok()?;
        let tree = path(doc.root_element(), &["spTree"])?;
        let rels = self.pkg.rels(&drawing_part);
        let inner = ShapeCx { master: cx.master, ph_chain: Vec::new(), rels: &rels, source: None };
        let mut shapes = Vec::new();
        self.sp_tree(tree, &inner, GroupTf::IDENTITY, false, &mut shapes);
        Some(shapes)
    }

    /// A `p:graphicFrame` holding a table.
    fn table_frame(&mut self, n: Node, cx: &ShapeCx, group: GroupTf) -> Option<Shape> {
        let tbl = path(n, &["graphic", "graphicData", "tbl"])?;
        let xfrm = group.apply(child(n, "xfrm").and_then(parse_xfrm)?);
        let name = path(n, &["nvGraphicFramePr", "cNvPr"]).and_then(|c| c.attribute("name")).unwrap_or("表格").to_string();
        let style = table::style_id(tbl).and_then(|id| self.table_style(cx.master, &id));
        let mut shape = Shape::new(self.pres.alloc_id(), name, xfrm);
        shape.kind = ShapeKind::Table(Box::new(table::table(tbl, cx.master, style, cx.rels)));
        shape.source = self.source_of(n, cx, &shape);
        Some(shape)
    }

    fn table_style(&mut self, master: &Master, id: &str) -> Option<Arc<TableStyle>> {
        let key = (master as *const Master as usize, id.to_string());
        if let Some(s) = self.table_styles.get(&key) {
            return s.clone();
        }
        let xml = self.table_styles_xml();
        let style = table::table_style(xml, id, &master.ctx).map(Arc::new);
        self.table_styles.insert(key, style.clone());
        style
    }

    fn table_styles_xml(&mut self) -> Option<&str> {
        self.table_styles_xml
            .get_or_insert_with(|| {
                let pres_part = self.pkg.rel_of_type("", "officeDocument").unwrap_or_else(|| "ppt/presentation.xml".to_string());
                self.pkg.rel_of_type(&pres_part, "tableStyles").and_then(|p| self.pkg.read_string(&p).ok())
            })
            .as_deref()
    }

    /// A shape the editor shows as a frame (or not at all) and saves verbatim.
    fn opaque(&mut self, n: Node, cx: &ShapeCx, group: GroupTf, label: &str) -> Option<Shape> {
        cx.source?;
        let xfrm = if label == "hidden" {
            Xfrm::default()
        } else {
            n.children().find(|c| c.tag_name().name() == "xfrm").and_then(parse_xfrm).map(|x| group.apply(x)).unwrap_or_default()
        };
        let name = n
            .children()
            .find(|c| c.tag_name().name().starts_with("nv"))
            .and_then(|nv| child(nv, "cNvPr"))
            .and_then(|c| c.attribute("name"))
            .unwrap_or(label)
            .to_string();
        let mut shape = Shape::new(self.pres.alloc_id(), name, xfrm);
        shape.kind = ShapeKind::Opaque { label: label.to_string() };
        shape.source = self.source_of(n, cx, &shape);
        Some(shape)
    }

    fn source_of(&self, n: Node, cx: &ShapeCx, shape: &Shape) -> Option<Arc<ShapeSource>> {
        let src = cx.source?;
        // Every relationship id the XML refers to, so copies can carry them to other slides.
        let mut rels = Vec::new();
        for d in n.descendants().filter(|d| d.is_element()) {
            for a in d.attributes().filter(|a| a.namespace() == Some(REL_NS)) {
                if let Some(rel) = cx.rels.get(a.value())
                    && !rels.iter().any(|(id, _, _): &(String, String, String)| id == a.value())
                {
                    rels.push((a.value().to_string(), rel.kind.clone(), if rel.external { format!("external:{}", rel.target) } else { rel.target.clone() }));
                }
            }
        }
        Some(Arc::new(ShapeSource {
            part: src.part.clone(),
            doc: src.doc.clone(),
            range: n.range(),
            namespaces: src.namespaces.clone(),
            rels,
            container: None,
            parsed: ParsedShape { xfrm: shape.xfrm, fill: shape.fill.clone(), line: shape.line, text: shape.text.clone(), kind: shape.kind.clone(), group: shape.group },
        }))
    }

    fn shape(&mut self, n: Node, cx: &ShapeCx, group: GroupTf) -> Option<Shape> {
        let nv = n.children().find(|c| c.is_element() && c.tag_name().name().starts_with("nv"))?;
        let c_nv_pr = child(nv, "cNvPr")?;
        let placeholder = path(nv, &["nvPr", "ph"]).map(|ph| Placeholder {
            kind: ph.attribute("type").unwrap_or("obj").to_string(),
            idx: attr_i64(ph, "idx").map(|v| v as u32).unwrap_or(0),
        });
        let has_idx = path(nv, &["nvPr", "ph"]).is_some_and(|ph| ph.attribute("idx").is_some());
        let inherited: Vec<&PhInfo> = match &placeholder {
            Some(ph) => cx
                .ph_chain
                .iter()
                .enumerate()
                .filter_map(|(level, phs)| {
                    // Only the nearest level (the layout) matches by index; masters match by type.
                    let by_idx = level == 0 && cx.ph_chain.len() > 1 && has_idx;
                    find_placeholder(phs, &ph.kind, by_idx.then_some(ph.idx))
                })
                .collect(),
            None => Vec::new(),
        };

        let sp_pr = child(n, "spPr");
        let xfrm = sp_pr.and_then(|p| child(p, "xfrm")).and_then(parse_xfrm).or_else(|| inherited.iter().find_map(|p| p.xfrm))?;
        let xfrm = group.apply(xfrm);
        let ctx = &cx.master.ctx;
        let style = child(n, "style");
        let style_ref = |name: &str| style.and_then(|s| child(s, name));

        let geometry = sp_pr.map(parse_geometry).unwrap_or_default();
        let fill = match sp_pr.map(|p| fill_spec(p, ctx)).unwrap_or(FillSpec::Unset) {
            FillSpec::Paint(p) => Some(p),
            FillSpec::None => None,
            FillSpec::Group => self.group_fill.clone(),
            FillSpec::Unset => style_ref("fillRef").and_then(|r| theme_fill(r, ctx)),
        };
        let line = parse_line(sp_pr.and_then(|p| child(p, "ln")), style_ref("lnRef"), ctx);

        let kind = if n.tag_name().name() == "pic" {
            let media = path(n, &["blipFill", "blip"])
                .and_then(|b| b.attribute((REL_NS, "embed")))
                .and_then(|id| cx.rels.get(id))
                .map(|r| r.target.clone());
            if let Some(m) = &media
                && !self.pres.media.contains_key(m)
                && let Ok(bytes) = self.pkg.read_bytes(m)
            {
                self.pres.media.insert(m.clone(), Arc::new(bytes));
            }
            let crop = path(n, &["blipFill", "srcRect"])
                .map(|r| {
                    let f = |name| attr_f64(r, name).unwrap_or(0.0) / 100_000.0;
                    Crop { left: f("l"), top: f("t"), right: f("r"), bottom: f("b") }
                })
                .unwrap_or_default();
            ShapeKind::Picture { media, crop }
        } else {
            ShapeKind::Shape
        };

        let text = child(n, "txBody").map(|tx| {
            let font_ref = style_ref("fontRef").map(|r| {
                let major = r.attribute("idx") == Some("major");
                LevelStyle {
                    color: ctx.color_in(r, None),
                    latin: Some(if major { "+mj-lt" } else { "+mn-lt" }.to_string()),
                    ea: Some(if major { "+mj-ea" } else { "+mn-ea" }.to_string()),
                    ..Default::default()
                }
            });
            let mut body = text_body(tx, cx.master, placeholder.as_ref(), &inherited, font_ref, cx.rels);
            // SmartArt drawings give the text its own frame (`dsp:txXfrm`): fold it into the insets.
            if let Some(tx_frame) = child(n, "txXfrm").and_then(parse_xfrm).map(|x| group.apply(x))
                && xfrm.rot == 0.0
            {
                let [l, t, r, b] = body.insets;
                body.insets = [
                    l + (tx_frame.x - xfrm.x).max(0),
                    t + (tx_frame.y - xfrm.y).max(0),
                    r + (xfrm.x + xfrm.w - tx_frame.x - tx_frame.w).max(0),
                    b + (xfrm.y + xfrm.h - tx_frame.y - tx_frame.h).max(0),
                ];
            }
            body
        });

        let mut shape = Shape {
            id: self.pres.alloc_id(),
            name: c_nv_pr.attribute("name").unwrap_or_default().to_string(),
            kind,
            xfrm,
            geometry,
            fill,
            line,
            text,
            placeholder,
            group: None,
            shadow: sp_pr.and_then(|p| path(p, &["effectLst", "outerShdw"])).and_then(|s| {
                Some(Shadow {
                    color: ctx.color_in(s, None)?,
                    blur: attr_i64(s, "blurRad").unwrap_or(0),
                    dist: attr_i64(s, "dist").unwrap_or(0),
                    dir: attr_f64(s, "dir").unwrap_or(0.0) / 60_000.0,
                })
            }),
            source: None,
        };
        shape.source = self.source_of(n, cx, &shape);
        Some(shape)
    }
}

fn is_hidden(n: Node) -> bool {
    n.children()
        .filter(|c| c.tag_name().name().starts_with("nv"))
        .any(|nv| child(nv, "cNvPr").and_then(|c| attr_bool(c, "hidden")).unwrap_or(false))
}

/// Normalizes placeholder types so slide, layout and master placeholders match.
fn ph_family(kind: &str) -> &str {
    match kind {
        "title" | "ctrTitle" => "title",
        "body" | "subTitle" | "obj" => "body",
        other => other,
    }
}

fn find_placeholder<'a>(phs: &'a [PhInfo], kind: &str, idx: Option<u32>) -> Option<&'a PhInfo> {
    if let Some(idx) = idx
        && let Some(p) = phs.iter().find(|p| p.idx == Some(idx))
    {
        return Some(p);
    }
    phs.iter().find(|p| p.kind == kind).or_else(|| phs.iter().find(|p| ph_family(&p.kind) == ph_family(kind)))
}

fn placeholders(root: Node, ctx: &ColorCtx) -> Vec<PhInfo> {
    let Some(tree) = path(root, &["cSld", "spTree"]) else { return Vec::new() };
    tree.descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "sp" | "pic"))
        .filter_map(|n| {
            let ph = n.children().find_map(|nv| path(nv, &["nvPr", "ph"]))?;
            let tx = child(n, "txBody");
            Some(PhInfo {
                kind: ph.attribute("type").unwrap_or("obj").to_string(),
                idx: attr_i64(ph, "idx").map(|v| v as u32),
                xfrm: path(n, &["spPr", "xfrm"]).and_then(parse_xfrm),
                body: BodyProps::parse(tx.and_then(|t| child(t, "bodyPr"))),
                lst: ListStyle::parse(tx.and_then(|t| child(t, "lstStyle")), ctx),
            })
        })
        .collect()
}

fn background(root: Node, ctx: &ColorCtx) -> Option<Color> {
    let bg = path(root, &["cSld", "bg"])?;
    if let Some(pr) = child(bg, "bgPr") {
        return match fill_spec(pr, ctx) {
            FillSpec::Paint(p) => Some(p.color()),
            _ => None,
        };
    }
    child(bg, "bgRef").and_then(|r| ctx.color_in(r, None))
}

fn parse_xfrm(x: Node) -> Option<Xfrm> {
    let off = child(x, "off")?;
    let ext = child(x, "ext")?;
    Some(Xfrm {
        x: attr_i64(off, "x")?,
        y: attr_i64(off, "y")?,
        w: attr_i64(ext, "cx")?,
        h: attr_i64(ext, "cy")?,
        rot: attr_f64(x, "rot").unwrap_or(0.0) / 60_000.0,
        flip_h: attr_bool(x, "flipH").unwrap_or(false),
        flip_v: attr_bool(x, "flipV").unwrap_or(false),
    })
}

fn group_tf(x: Node) -> GroupTf {
    let get = |name: &str, a: &str, b: &str| child(x, name).map(|n| (attr_f64(n, a).unwrap_or(0.0), attr_f64(n, b).unwrap_or(0.0)));
    let (Some(off), Some(ext), Some(ch_off), Some(ch_ext)) =
        (get("off", "x", "y"), get("ext", "cx", "cy"), get("chOff", "x", "y"), get("chExt", "cx", "cy"))
    else {
        return GroupTf::IDENTITY;
    };
    let sx = if ch_ext.0 > 0.0 { ext.0 / ch_ext.0 } else { 1.0 };
    let sy = if ch_ext.1 > 0.0 { ext.1 / ch_ext.1 } else { 1.0 };
    GroupTf { sx, sy, tx: off.0 - ch_off.0 * sx, ty: off.1 - ch_off.1 * sy }
}

fn parse_geometry(sp_pr: Node) -> Geometry {
    if let Some(prst) = child(sp_pr, "prstGeom") {
        let adjust = child(prst, "avLst")
            .map(|av| {
                children(av, "gd")
                    .filter_map(|gd| {
                        let name = gd.attribute("name")?;
                        let val = gd.attribute("fmla")?.strip_prefix("val ")?.trim().parse().ok()?;
                        Some((name.to_string(), val))
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Geometry::Preset { name: prst.attribute("prst").unwrap_or("rect").to_string(), adjust };
    }
    if let Some(lst) = path(sp_pr, &["custGeom", "pathLst"]) {
        return Geometry::Custom(children(lst, "path").map(custom_path).collect());
    }
    Geometry::default()
}

fn custom_path(p: Node) -> CustomPath {
    let pt = |n: Node| (attr_f64(n, "x").unwrap_or(0.0), attr_f64(n, "y").unwrap_or(0.0));
    let mut cmds = Vec::new();
    let mut cur = (0.0, 0.0);
    for c in p.children().filter(|c| c.is_element()) {
        let pts: Vec<(f64, f64)> = children(c, "pt").map(pt).collect();
        match (c.tag_name().name(), pts.as_slice()) {
            ("moveTo", [a, ..]) => {
                cmds.push(PathCmd::MoveTo(a.0, a.1));
                cur = *a;
            }
            ("lnTo", [a, ..]) => {
                cmds.push(PathCmd::LineTo(a.0, a.1));
                cur = *a;
            }
            ("quadBezTo", [a, b, ..]) => {
                cmds.push(PathCmd::QuadTo(a.0, a.1, b.0, b.1));
                cur = *b;
            }
            ("cubicBezTo", [a, b, d, ..]) => {
                cmds.push(PathCmd::CubicTo(a.0, a.1, b.0, b.1, d.0, d.1));
                cur = *d;
            }
            ("arcTo", _) => cur = arc_to(c, cur, &mut cmds),
            ("close", _) => cmds.push(PathCmd::Close),
            _ => {}
        }
    }
    CustomPath {
        w: attr_f64(p, "w").unwrap_or(0.0),
        h: attr_f64(p, "h").unwrap_or(0.0),
        cmds,
        fill: p.attribute("fill") != Some("none"),
        stroke: attr_bool(p, "stroke").unwrap_or(true),
    }
}

/// `a:arcTo` continues from the current point along an ellipse; emitted as cubic segments.
fn arc_to(n: Node, cur: (f64, f64), cmds: &mut Vec<PathCmd>) -> (f64, f64) {
    let wr = attr_f64(n, "wR").unwrap_or(0.0);
    let hr = attr_f64(n, "hR").unwrap_or(0.0);
    let st = (attr_f64(n, "stAng").unwrap_or(0.0) / 60_000.0).to_radians();
    let sw = (attr_f64(n, "swAng").unwrap_or(0.0) / 60_000.0).to_radians();
    let center = (cur.0 - wr * st.cos(), cur.1 - hr * st.sin());
    let segments = (sw.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sw / segments as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = st;
    let mut end = cur;
    for _ in 0..segments {
        let b = a + step;
        let p0 = (center.0 + wr * a.cos(), center.1 + hr * a.sin());
        end = (center.0 + wr * b.cos(), center.1 + hr * b.sin());
        let c1 = (p0.0 - k * wr * a.sin(), p0.1 + k * hr * a.cos());
        let c2 = (end.0 + k * wr * b.sin(), end.1 - k * hr * b.cos());
        cmds.push(PathCmd::CubicTo(c1.0, c1.1, c2.0, c2.1, end.0, end.1));
        a = b;
    }
    end
}

enum FillSpec {
    Unset,
    None,
    Paint(Paint),
    /// `a:grpFill`: the enclosing group's fill.
    Group,
}

impl FillSpec {
    /// The fill as one color (gradients by their first stop), for lines and table cells.
    fn color(self) -> FillSpecColor {
        match self {
            FillSpec::Paint(p) => FillSpecColor::Solid(p.color()),
            FillSpec::None => FillSpecColor::None,
            FillSpec::Unset | FillSpec::Group => FillSpecColor::Unset,
        }
    }
}

enum FillSpecColor {
    Unset,
    None,
    Solid(Color),
}

/// The theme fill a `a:fillRef` points at, colored with the reference's color (`phClr`).
fn theme_fill(r: Node, ctx: &ColorCtx) -> Option<Paint> {
    let idx = attr_i64(r, "idx").unwrap_or(0);
    let ph = ctx.color_in(r, None);
    // 0 means no fill; 1001+ are background styles, which shapes rarely use: treat as solid.
    if idx <= 0 {
        return None;
    }
    let xml = ctx.theme.fill_styles.get(idx as usize - 1).filter(|_| idx < 1000);
    let Some(xml) = xml else { return ph.map(Paint::Solid) };
    let doc = Document::parse(xml).ok()?;
    let fill = doc.root_element().first_element_child()?;
    match fill.tag_name().name() {
        "solidFill" => ctx.color_in(fill, ph).map(Paint::Solid),
        "gradFill" => gradient(fill, ctx, ph).map(|g| Paint::Gradient(Arc::new(g))),
        "noFill" => None,
        _ => ph.map(Paint::Solid),
    }
}

/// `a:gradFill`. Stops are sorted; positions are fractions. `ph` fills in `phClr`.
fn gradient(g: Node, ctx: &ColorCtx, ph: Option<Color>) -> Option<Gradient> {
    let mut stops: Vec<(f32, Color)> = path(g, &["gsLst"])
        .map(|l| {
            children(l, "gs")
                .filter_map(|gs| Some(((attr_f64(gs, "pos").unwrap_or(0.0) / 100_000.0) as f32, ctx.color_in(gs, ph)?)))
                .collect()
        })
        .unwrap_or_default();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    if stops.is_empty() {
        return None;
    }
    let kind = if let Some(lin) = child(g, "lin") {
        GradientKind::Linear { angle: attr_f64(lin, "ang").unwrap_or(0.0) / 60_000.0, scaled: attr_bool(lin, "scaled").unwrap_or(false) }
    } else if let Some(p) = child(g, "path") {
        let focus = child(p, "fillToRect")
            .map(|r| ["l", "t", "r", "b"].map(|k| attr_f64(r, k).unwrap_or(0.0) / 100_000.0))
            .unwrap_or([0.5; 4]);
        GradientKind::Path { circle: p.attribute("path") == Some("circle"), focus }
    } else {
        // No direction given: PowerPoint shades top to bottom.
        GradientKind::Linear { angle: 90.0, scaled: false }
    };
    Some(Gradient { stops, kind, rotate_with_shape: attr_bool(g, "rotWithShape").unwrap_or(true) })
}

fn fill_spec(n: Node, ctx: &ColorCtx) -> FillSpec {
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "noFill" => return FillSpec::None,
            "solidFill" => return ctx.color_in(c, None).map(|c| FillSpec::Paint(Paint::Solid(c))).unwrap_or(FillSpec::None),
            "gradFill" => return gradient(c, ctx, None).map(|g| FillSpec::Paint(Paint::Gradient(Arc::new(g)))).unwrap_or(FillSpec::None),
            // Patterns show as their foreground color.
            "pattFill" => return child(c, "fgClr").and_then(|f| ctx.color_in(f, None)).map(|c| FillSpec::Paint(Paint::Solid(c))).unwrap_or(FillSpec::None),
            "grpFill" => return FillSpec::Group,
            "blipFill" => return FillSpec::None,
            _ => {}
        }
    }
    FillSpec::Unset
}

fn parse_line(ln: Option<Node>, ln_ref: Option<Node>, ctx: &ColorCtx) -> Option<Line> {
    let ref_idx = ln_ref.and_then(|r| attr_i64(r, "idx")).unwrap_or(0);
    let ref_color = ln_ref.filter(|_| ref_idx > 0).and_then(|r| ctx.color_in(r, None));
    let color = match ln.map(|l| fill_spec(l, ctx).color()).unwrap_or(FillSpecColor::Unset) {
        FillSpecColor::Solid(c) => c,
        FillSpecColor::None => return None,
        FillSpecColor::Unset => ref_color?,
    };
    let ref_width = ctx.theme.line_widths.get((ref_idx as usize).wrapping_sub(1)).copied();
    let width = ln.and_then(|l| attr_i64(l, "w")).or(ref_width).unwrap_or(9525);
    let ln_child = |name: &str| ln.and_then(|l| child(l, name));
    let end = |name: &str| {
        ln_child(name).map_or(LineEnd::NONE, |e| {
            let size = |a: &str| match e.attribute(a) {
                Some("sm") => 0,
                Some("lg") => 2,
                _ => 1,
            };
            LineEnd { kind: EndKind::parse(e.attribute("type").unwrap_or("none")), width: size("w"), length: size("len") }
        })
    };
    Some(Line {
        color,
        width,
        dash: ln_child("prstDash").and_then(|d| d.attribute("val")).map_or(Dash::Solid, Dash::parse),
        head: end("headEnd"),
        tail: end("tailEnd"),
    })
}

fn text_body(tx: Node, master: &Master, ph: Option<&Placeholder>, inherited: &[&PhInfo], font_ref: Option<LevelStyle>, rels: &HashMap<String, Rel>) -> TextBody {
    let theme = &master.ctx.theme;
    let ctx = &master.ctx;
    let body = inherited.iter().fold(BodyProps::parse(child(tx, "bodyPr")), |acc, p| acc.or(&p.body));
    let defaults = TextBody::default();
    let insets = std::array::from_fn(|i| body.insets[i].unwrap_or(defaults.insets[i]));

    // Lowest-priority layers: master text styles, then the presentation defaults.
    let master_style = match ph.map(|p| ph_family(&p.kind)) {
        Some("title") => Some(&master.title),
        Some("body") => Some(&master.body),
        Some(_) => Some(&master.other),
        None => None,
    };
    let shape_lst = ListStyle::parse(child(tx, "lstStyle"), ctx);
    let level_chain = |lvl: u8| -> LevelStyle {
        let mut s = shape_lst.level(lvl).clone();
        for p in inherited {
            s = s.or(p.lst.level(lvl));
        }
        if let Some(m) = master_style {
            s = s.or(m.level(lvl));
        }
        if let Some(f) = &font_ref {
            s = s.or(f);
        }
        s.or(master.default_text.level(lvl))
    };

    let finalize = |s: &LevelStyle| run_style(s, theme);

    let paragraphs = children(tx, "p")
        .map(|p| {
            let ppr = child(p, "pPr");
            let lvl = ppr.and_then(|n| attr_i64(n, "lvl")).unwrap_or(0).clamp(0, 8) as u8;
            let pstyle = ppr.map(|n| LevelStyle::parse_ppr(n, ctx)).unwrap_or_default().or(&level_chain(lvl));
            let mut runs: Vec<Run> = Vec::new();
            for c in p.children().filter(|c| c.is_element()) {
                match c.tag_name().name() {
                    "r" | "fld" => {
                        let rstyle = child(c, "rPr").map(|r| LevelStyle::parse_rpr(r, ctx)).unwrap_or_default().or(&pstyle);
                        let raw = child(c, "t").and_then(|t| t.text()).unwrap_or_default();
                        // Symbol-font codes (U+F020..U+F0FF) become the characters they stand for.
                        let sym = child(c, "rPr").and_then(|r| child(r, "sym")).and_then(|f| f.attribute("typeface")).unwrap_or("Wingdings");
                        let text: String = raw
                            .chars()
                            .map(|ch| match ch as u32 {
                                0xF020..=0xF0FF => ppt_model::bullet::symbol_char(sym, ch).or_else(|| ppt_model::bullet::symbol_char("Wingdings", ch)).unwrap_or(ch),
                                _ => ch,
                            })
                            .collect();
                        let mut style = finalize(&rstyle);
                        style.link = child(c, "rPr").and_then(|r| child(r, "hlinkClick")).map(|h| hyperlink(h, rels, ctx));
                        runs.push(Run { text, style });
                    }
                    // Soft line break inside a paragraph.
                    "br" => {
                        let rstyle = child(c, "rPr").map(|r| LevelStyle::parse_rpr(r, ctx)).unwrap_or_default().or(&pstyle);
                        runs.push(Run { text: "\u{2028}".to_string(), style: finalize(&rstyle) });
                    }
                    _ => {}
                }
            }
            let end = child(p, "endParaRPr").map(|r| LevelStyle::parse_rpr(r, ctx)).unwrap_or_default().or(&pstyle);
            let end_style = finalize(&end);
            let first_size = runs.first().map(|r| r.style.size_pt).unwrap_or(end_style.size_pt);
            Paragraph {
                runs,
                align: pstyle.align.unwrap_or_default(),
                level: lvl,
                bullet: pstyle.bullet(),
                line_spacing: pstyle.line_spacing.unwrap_or(1.0),
                mar_l: pstyle.mar_l.unwrap_or(0),
                indent: pstyle.indent.unwrap_or(0),
                space_before: pstyle.space_before.map(|s| s.to_points(first_size)).unwrap_or(0.0),
                space_after: pstyle.space_after.map(|s| s.to_points(first_size)).unwrap_or(0.0),
                end_style,
            }
        })
        .collect::<Vec<_>>();

    let level_defaults = (0..9).map(|lvl| para_defaults(&level_chain(lvl), &finalize)).collect();
    TextBody {
        paragraphs: if paragraphs.is_empty() { defaults.paragraphs } else { paragraphs },
        anchor: body.anchor.unwrap_or_default(),
        insets,
        wrap: body.wrap.unwrap_or(true),
        font_scale: body.font_scale.unwrap_or(1.0),
        line_spacing_reduction: body.line_spacing_reduction.unwrap_or(0.0),
        auto_grow: body.auto_grow.unwrap_or(false),
        defaults: Some(Arc::new(level_defaults)),
    }
}

/// `a:hlinkClick`: the relationship it names (if any) and its action.
fn hyperlink(h: Node, rels: &HashMap<String, Rel>, ctx: &ColorCtx) -> Arc<Link> {
    let rel = h.attribute((REL_NS, "id")).filter(|id| !id.is_empty()).and_then(|id| rels.get(id));
    Arc::new(Link {
        kind: rel.map_or_else(|| Link::HYPERLINK.to_string(), |r| r.kind.clone()),
        target: rel.map(|r| r.target.clone()).unwrap_or_default(),
        external: rel.is_some_and(|r| r.external),
        action: h.attribute("action").map(str::to_string),
        tooltip: h.attribute("tooltip").map(str::to_string),
        color: ctx.scheme("hlink"),
    })
}

/// Resolves inherited run properties, falling back to theme fonts and 18pt black.
fn run_style(s: &LevelStyle, theme: &Theme) -> RunStyle {
    RunStyle {
        size_pt: s.size.unwrap_or(18.0),
        bold: s.bold.unwrap_or(false),
        italic: s.italic.unwrap_or(false),
        underline: s.underline.unwrap_or(false),
        color: s.color.unwrap_or(Color::BLACK),
        latin_font: theme.resolve_typeface(s.latin.as_deref().unwrap_or("+mn-lt")),
        ea_font: theme.resolve_typeface(s.ea.as_deref().unwrap_or("+mn-ea")),
        link: None,
    }
}

fn para_defaults(s: &LevelStyle, finalize: &dyn Fn(&LevelStyle) -> RunStyle) -> ParaDefaults {
    let style = finalize(s);
    ParaDefaults {
        align: s.align.unwrap_or_default(),
        bullet: s.bullet(),
        line_spacing: s.line_spacing.unwrap_or(1.0),
        mar_l: s.mar_l.unwrap_or(0),
        indent: s.indent.unwrap_or(0),
        space_before: s.space_before.map(|v| v.to_points(style.size_pt)).unwrap_or(0.0),
        space_after: s.space_after.map(|v| v.to_points(style.size_pt)).unwrap_or(0.0),
        style,
    }
}

#[cfg(test)]
mod tests;
