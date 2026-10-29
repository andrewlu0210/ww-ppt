//! Save → reopen round trips against the sample deck and the built-in template.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use ppt_model::{Color, Geometry, Line, Paint, Presentation, Shape, ShapeKind, TextBody, Xfrm};
use ppt_ooxml::{parse_pptx, save_pptx, template_package};

fn sample() -> Presentation {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx");
    parse_pptx(&std::fs::read(path).unwrap()).unwrap()
}

fn parts(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).unwrap();
            (f.name().unwrap().into_owned(), buf)
        })
        .collect()
}

fn texts(pres: &Presentation, slide: usize) -> Vec<String> {
    pres.slides[slide].shapes.iter().filter_map(|s| s.text.as_ref()).map(|t| t.plain_text()).collect()
}

#[test]
fn unchanged_deck_saves_identical_parts() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let saved = save_pptx(&parse_pptx(&original).unwrap()).unwrap();
    let (a, b) = (parts(&original), parts(&saved));
    assert_eq!(a.keys().collect::<std::collections::BTreeSet<_>>(), b.keys().collect());
    for (name, bytes) in &a {
        // The three bookkeeping parts are rewritten, everything else is copied.
        if matches!(name.as_str(), "[Content_Types].xml" | "ppt/presentation.xml" | "ppt/_rels/presentation.xml.rels") {
            continue;
        }
        assert_eq!(bytes, &b[name], "{name} changed");
    }
    let reparsed = parse_pptx(&saved).unwrap();
    assert_eq!(reparsed.slides.len(), 4);
    for i in 0..4 {
        assert_eq!(texts(&reparsed, i), texts(&parse_pptx(&original).unwrap(), i));
    }
}

#[test]
fn edited_text_keeps_other_formatting_and_slides() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut pres = parse_pptx(&original).unwrap();
    // Slide 2, content placeholder: change the first paragraph's text.
    let body = pres.slides[1].shapes[1].text.as_mut().unwrap();
    let end = body.paragraph_ranges()[0].end;
    body.delete(0..end);
    let style = body.style_at(0);
    body.insert(0, "新的第一段 & <特殊字元>", &style);
    let saved = save_pptx(&pres).unwrap();

    let (a, b) = (parts(&original), parts(&saved));
    assert_eq!(a["ppt/slides/slide1.xml"], b["ppt/slides/slide1.xml"], "untouched slide copied verbatim");
    assert_ne!(a["ppt/slides/slide2.xml"], b["ppt/slides/slide2.xml"]);

    let re = parse_pptx(&saved).unwrap();
    let body = re.slides[1].shapes[1].text.as_ref().unwrap();
    assert_eq!(body.paragraphs[0].text(), "新的第一段 & <特殊字元>");
    assert_eq!(body.paragraphs.len(), 6);
    assert!(body.paragraphs[0].bullet.is_some(), "bullet still inherited from the layout");
    assert_eq!(body.paragraphs[1].level, 1);
    let last = &body.paragraphs[5];
    assert!(last.runs[0].style.bold);
    assert_eq!(last.runs[1].style.color, Color::rgb(0xC0, 0, 0));
    // Only the changed paragraph was regenerated, and inherited sizes were not written out.
    let xml = String::from_utf8(b["ppt/slides/slide2.xml"].clone()).unwrap();
    assert!(!xml.contains(r#"sz="3200""#), "inherited size should stay implicit");
    assert_eq!(re.slides[1].shapes[0].text.as_ref().unwrap().plain_text(), "核心目標");
}

#[test]
fn moved_and_recolored_shapes() {
    let mut pres = sample();
    let shape = &mut pres.slides[2].shapes[2];
    shape.xfrm.x += 100_000;
    shape.xfrm.rot = 45.0;
    shape.fill = Some(Paint::Solid(Color::rgb(0x12, 0x34, 0x56)));
    shape.line = Some(Line::new(Color::rgb(1, 2, 3), 25_400));
    let expect = shape.xfrm;
    // Placeholder that inherits its position: moving it must add an xfrm.
    pres.slides[0].shapes[0].xfrm.y += 50_000;
    let title_xfrm = pres.slides[0].shapes[0].xfrm;
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let s = &re.slides[2].shapes[2];
    assert_eq!(s.xfrm, expect);
    assert_eq!(s.fill, Some(Paint::Solid(Color::rgb(0x12, 0x34, 0x56))));
    assert_eq!(s.line, Some(Line::new(Color::rgb(1, 2, 3), 25_400)));
    assert_eq!(s.text.as_ref().unwrap().plain_text(), "Ellipse");
    assert_eq!(re.slides[0].shapes[0].xfrm, title_xfrm);
}

#[test]
fn edited_group_child_keeps_its_group() {
    let mut pres = sample();
    let n = pres.slides[2].shapes.len();
    let group = pres.slides[2].shapes[n - 1].group;
    assert!(group.is_some(), "the sample's last shape is in a group");
    pres.slides[2].shapes[n - 1].xfrm.x += 200_000;
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let members = pres.slides[2].shapes.iter().filter(|s| s.group == group).count();
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
    let g = re.slides[2].shapes[n - 1].group;
    assert!(g.is_some());
    assert_eq!(re.slides[2].shapes.iter().filter(|s| s.group == g).count(), members);
}

#[test]
fn ungrouping_and_grouping_round_trip() {
    let mut pres = sample();
    let n = pres.slides[2].shapes.len();
    let group = pres.slides[2].shapes[n - 1].group;
    for s in pres.slides[2].shapes.iter_mut().filter(|s| s.group == group) {
        s.group = None;
    }
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
    assert!(re.slides[2].shapes.iter().all(|s| s.group.is_none()), "ungrouped shapes stay ungrouped");

    // Group the first two shapes of slide 3 into a new group.
    let mut pres = re;
    let gid = pres.alloc_id();
    pres.slides[2].shapes[0].group = Some(gid);
    pres.slides[2].shapes[1].group = Some(gid);
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let saved = save_pptx(&pres).unwrap();
    assert!(slide_xml(&saved, 3).contains("<p:grpSp>"));
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
    let g = re.slides[2].shapes[0].group;
    assert!(g.is_some() && re.slides[2].shapes[1].group == g);
}

#[test]
fn add_delete_reorder_duplicate_slides() {
    let mut pres = sample();
    // New slide from layout "Title and Content" with text and a new rectangle.
    let layout = pres.layouts.iter().position(|l| l.name == "Title and Content").unwrap();
    let mut slide = pres.slide_from_layout(layout);
    let title = slide.shapes.iter_mut().find(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "title")).unwrap();
    let body = title.text.as_mut().unwrap();
    let style = body.style_at(0);
    body.insert(0, "新增的投影片", &style);
    let id = pres.alloc_id();
    let mut rect = Shape::new(id, "rect", Xfrm::new(1_000_000, 4_000_000, 2_000_000, 1_000_000));
    rect.geometry = Geometry::preset("roundRect");
    rect.fill = Some(Paint::Solid(Color::rgb(0x44, 0x72, 0xC4)));
    slide.shapes.push(rect);
    pres.slides.insert(1, slide);
    // Duplicate the picture slide, delete the shapes slide, move the title slide last.
    let dup = pres.slides[4].clone();
    pres.slides.push(dup);
    pres.slides.remove(3);
    let first = pres.slides.remove(0);
    pres.slides.push(first);

    let saved = save_pptx(&pres).unwrap();
    let names = parts(&saved);
    assert!(!names.contains_key("ppt/slides/slide3.xml"), "deleted slide's part is dropped");
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides.len(), 5);
    assert_eq!(texts(&re, 0)[0], "新增的投影片");
    assert!(re.slides[0].shapes.iter().any(|s| matches!(&s.geometry, Geometry::Preset { name, .. } if name == "roundRect")));
    assert_eq!(texts(&re, 4)[0], "ww-ppt M0 技術驗證");
    // Both picture slides reference real media.
    for i in [2, 3] {
        let pic = re.slides[i].shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Picture { media, .. } => media.clone(),
            _ => None,
        });
        assert!(re.media.contains_key(&pic.unwrap()));
    }
}

#[test]
fn shape_pasted_onto_another_slide_carries_its_image() {
    let mut pres = sample();
    let mut pic = pres.slides[3].shapes[1].clone();
    pic.id = pres.alloc_id();
    pres.slides[0].shapes.push(pic);
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Picture { media: Some(m), .. } = &re.slides[0].shapes.last().unwrap().kind else { panic!("picture lost") };
    assert!(re.media.contains_key(m));
}

#[test]
fn template_is_a_complete_deck() {
    let pres = parse_pptx(&template_package()).unwrap();
    assert_eq!(pres.slide_width, 12_192_000);
    assert_eq!(pres.layouts.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["標題投影片", "標題及內容", "區段標題", "兩項物件", "只有標題", "空白"]);
    let slide = &pres.slides[0];
    assert_eq!(slide.shapes.len(), 2);
    let title = &slide.shapes[0];
    assert_eq!(title.placeholder.as_ref().unwrap().kind, "ctrTitle");
    assert!(title.text.as_ref().unwrap().is_empty());
    assert_eq!(title.text.as_ref().unwrap().paragraphs[0].end_style.size_pt, 60.0);
    assert_eq!(title.xfrm.w, 9_144_000);
    // Body placeholders on the content layout carry bullets from the master.
    let content = &pres.layouts[1].placeholders[1];
    let p = &content.text.as_ref().unwrap().paragraphs[0];
    assert_eq!(p.bullet.as_ref().map(|b| b.kind.clone()), Some(ppt_model::BulletKind::Char('•')));
    assert_eq!(p.end_style.size_pt, 28.0);
    assert_eq!(pres.minor_font, ("Calibri".to_string(), "新細明體".to_string()));
}

#[test]
fn new_deck_from_template_round_trips() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    let body = pres.slides[0].shapes[0].text.as_mut().unwrap();
    let style = body.style_at(0);
    body.insert(0, "我的簡報", &style);
    body.apply_style(0.."我的".len(), |s| s.bold = true);
    let id = pres.alloc_id();
    let mut tb = Shape::new(id, "文字方塊", Xfrm::new(500_000, 500_000, 3_000_000, 400_000));
    let mut text = TextBody { auto_grow: true, defaults: pres.text_defaults.clone(), ..Default::default() };
    text.paragraphs[0].end_style = pres.default_run_style();
    let style = text.style_at(0);
    text.insert(0, "Hello\n世界", &style);
    tb.text = Some(text);
    pres.slides[0].shapes.push(tb);
    let layout = pres.slide_from_layout(5);
    pres.slides.push(layout);

    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides.len(), 2);
    let title = re.slides[0].shapes[0].text.as_ref().unwrap();
    assert_eq!(title.plain_text(), "我的簡報");
    assert!(title.paragraphs[0].runs[0].style.bold && !title.paragraphs[0].runs[1].style.bold);
    assert_eq!(title.paragraphs[0].runs[0].style.size_pt, 60.0);
    let tb = re.slides[0].shapes.last().unwrap();
    assert_eq!(tb.text.as_ref().unwrap().plain_text(), "Hello\n世界");
    assert!(tb.text.as_ref().unwrap().auto_grow);
    assert_eq!(tb.fill, None);
    assert_eq!(re.slides[1].layout, Some(5));
}

fn table_shape(pres: &mut Presentation, rows: usize, cols: usize) -> Shape {
    use ppt_model::table::{TableCell, TableFlags, TableRow};
    let mut rows_v = Vec::new();
    for r in 0..rows {
        let cells = (0..cols)
            .map(|c| {
                let mut text = TextBody { insets: [91_440, 45_720, 91_440, 45_720], ..Default::default() };
                text.set_plain_text(&format!("格{r}{c}"));
                TableCell { text, ..Default::default() }
            })
            .collect();
        rows_v.push(TableRow { height: 370_840, cells });
    }
    let table = ppt_model::Table {
        cols: vec![2_000_000; cols],
        rows: rows_v,
        flags: TableFlags { first_row: true, band_row: true, ..Default::default() },
        style: pres.table_style.clone(),
    };
    let id = pres.alloc_id();
    let mut shape = Shape::new(id, "表格", Xfrm::new(914_400, 914_400, 2_000_000 * cols as i64, 370_840 * rows as i64));
    shape.kind = ShapeKind::Table(Box::new(table));
    shape
}

fn table_of(shape: &Shape) -> &ppt_model::Table {
    match &shape.kind {
        ShapeKind::Table(t) => t,
        other => panic!("expected a table, got {other:?}"),
    }
}

fn cell_texts(t: &ppt_model::Table) -> Vec<Vec<String>> {
    t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.plain_text()).collect()).collect()
}

fn slide_xml(bytes: &[u8], n: usize) -> String {
    String::from_utf8(parts(bytes)[&format!("ppt/slides/slide{n}.xml")].clone()).unwrap()
}

#[test]
fn new_table_round_trips_with_default_style() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    assert!(pres.table_style.is_some(), "template should get the built-in default table style");
    let shape = table_shape(&mut pres, 3, 2);
    pres.slides[0].shapes.push(shape);
    let saved = save_pptx(&pres).unwrap();
    let re = parse_pptx(&saved).unwrap();
    let t = table_of(re.slides[0].shapes.last().unwrap());
    assert_eq!(t.cols, vec![2_000_000; 2]);
    assert_eq!(cell_texts(t), vec![vec!["格00", "格01"], vec!["格10", "格11"], vec!["格20", "格21"]]);
    assert!(t.flags.first_row && t.flags.band_row);
    assert_eq!(t.style.as_ref().unwrap().id, "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}");
    // Header cells take the style's white bold text.
    assert!(t.look(0, 0).fill.is_some());
    assert_eq!(t.rows[0].cells[0].text.style_at(0).color, Color::WHITE);
}

#[test]
fn editing_one_cell_rewrites_only_that_cell() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    let shape = table_shape(&mut pres, 2, 2);
    pres.slides[0].shapes.push(shape);
    let first = save_pptx(&pres).unwrap();
    let mut pres = parse_pptx(&first).unwrap();
    let ShapeKind::Table(t) = &mut pres.slides[0].shapes.last_mut().unwrap().kind else { panic!() };
    t.rows[1].cells[1].text.set_plain_text("改過 & <新>");
    t.cols[0] = 2_500_000;
    let second = save_pptx(&pres).unwrap();

    let (a, b) = (slide_xml(&first, 1), slide_xml(&second, 1));
    // Untouched cells keep their XML byte for byte.
    for text in ["格00", "格01", "格10"] {
        let cell = |xml: &str| {
            let at = xml.find(text).unwrap();
            let start = xml[..at].rfind("<a:tc>").unwrap();
            let end = at + xml[at..].find("</a:tc>").unwrap();
            xml[start..end].to_string()
        };
        assert_eq!(cell(&a), cell(&b), "{text}");
    }
    assert!(b.contains("改過 &amp; &lt;新&gt;"));
    let re = parse_pptx(&second).unwrap();
    let t = table_of(re.slides[0].shapes.last().unwrap());
    assert_eq!(t.cols[0], 2_500_000);
    assert_eq!(cell_texts(t)[1], vec!["格10", "改過 & <新>"]);
}

#[test]
fn table_rows_and_merges_survive_structural_edits() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    let shape = table_shape(&mut pres, 2, 3);
    pres.slides[0].shapes.push(shape);
    let mut pres = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Table(t) = &mut pres.slides[0].shapes.last_mut().unwrap().kind else { panic!() };
    let mut row = t.rows[1].clone();
    row.cells[0].text.set_plain_text("新列");
    row.cells[0].grid_span = 2;
    row.cells[1].h_merge = true;
    t.rows.push(row);
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let t = table_of(re.slides[0].shapes.last().unwrap());
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.rows[2].cells[0].grid_span, 2);
    assert!(t.rows[2].cells[1].h_merge);
    assert_eq!(t.rows[2].cells[0].text.plain_text(), "新列");
    assert_eq!(t.style.as_ref().unwrap().id, "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}");
}

#[test]
fn moving_a_whole_group_only_changes_its_outer_frame() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut pres = parse_pptx(&original).unwrap();
    let n = pres.slides[2].shapes.len();
    let group = pres.slides[2].shapes[n - 1].group;
    for s in pres.slides[2].shapes.iter_mut().filter(|s| s.group == group) {
        s.xfrm.x += 300_000;
        s.xfrm.y += 100_000;
    }
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let saved = save_pptx(&pres).unwrap();
    let (a, b) = (slide_xml(&original, 3), slide_xml(&saved, 3));
    let group_of = |xml: &str| xml[xml.find("<p:grpSp>").unwrap()..xml.rfind("</p:grpSp>").unwrap()].to_string();
    let (ga, gb) = (group_of(&a), group_of(&b));
    // Children keep their XML (group-relative coordinates); only the outer a:off changes.
    let children = |g: &str| g[g.find("</p:grpSpPr>").unwrap()..].to_string();
    assert_eq!(children(&ga), children(&gb));
    assert_ne!(ga, gb);
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
}

#[test]
fn hyperlinks_survive_saving_and_text_edits() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    let id = pres.alloc_id();
    let mut shape = Shape::new(id, "連結", Xfrm::new(914_400, 914_400, 4_000_000, 600_000));
    let mut body = TextBody::default();
    let plain = body.style_at(0);
    let linked = ppt_model::RunStyle { link: Some(std::sync::Arc::new(ppt_model::Link::url("https://example.com/a?b=1&c=2", None))), ..plain.clone() };
    body.insert(0, "前往 ", &plain);
    body.insert(3, "範例網站", &linked);
    shape.text = Some(body);
    pres.slides[0].shapes.push(shape);
    let saved = save_pptx(&pres).unwrap();
    let rels = String::from_utf8(parts(&saved)["ppt/slides/_rels/slide1.xml.rels"].clone()).unwrap();
    assert!(rels.contains(r#"Target="https://example.com/a?b=1&amp;c=2" TargetMode="External""#), "{rels}");

    let link_of = |pres: &Presentation| {
        let body = pres.slides[0].shapes.last().unwrap().text.as_ref().unwrap();
        body.paragraphs[0].runs.iter().find_map(|r| r.style.link.clone().map(|l| (r.text.clone(), l.target.clone(), l.external)))
    };
    let mut re = parse_pptx(&saved).unwrap();
    assert_eq!(link_of(&re), Some(("範例網站".to_string(), "https://example.com/a?b=1&c=2".to_string(), true)));
    // Editing other text in the paragraph regenerates it; the link stays.
    let body = re.slides[0].shapes.last_mut().unwrap().text.as_mut().unwrap();
    let style = body.style_at(0);
    body.insert(0, "請", &style);
    let re = parse_pptx(&save_pptx(&re).unwrap()).unwrap();
    assert_eq!(link_of(&re).map(|l| l.0), Some("範例網站".to_string()));
}

#[test]
fn gallery_styles_are_written_to_table_styles() {
    let mut pres = parse_pptx(&template_package()).unwrap();
    assert!(pres.table_styles.len() >= 9, "{}", pres.table_styles.len());
    let accent3 = pres.table_styles.iter().find(|s| s.name == "Medium Style 2 - Accent 3").cloned().unwrap();
    let mut shape = table_shape(&mut pres, 2, 2);
    let ShapeKind::Table(t) = &mut shape.kind else { panic!() };
    t.style = Some(accent3.clone());
    pres.slides[0].shapes.push(shape);
    let saved = save_pptx(&pres).unwrap();
    let p = parts(&saved);
    let styles = String::from_utf8(p.get("ppt/tableStyles.xml").expect("tableStyles part created").clone()).unwrap();
    assert!(styles.contains(&accent3.id) && styles.contains("Medium Style 2 - Accent 3"));
    let ct = String::from_utf8(p["[Content_Types].xml"].clone()).unwrap();
    assert!(ct.contains("/ppt/tableStyles.xml"));
    let re = parse_pptx(&saved).unwrap();
    let t = table_of(re.slides[0].shapes.last().unwrap());
    assert_eq!(t.style.as_ref().unwrap().id, accent3.id);
    assert_eq!(t.look(0, 0).fill, accent3.first_row.fill.and_then(|f| match f {
        ppt_model::table::CellFill::Solid(c) => Some(c),
        _ => None,
    }));
    // Saving again adds nothing: the style is defined now.
    let again = save_pptx(&re).unwrap();
    assert_eq!(parts(&again)["ppt/tableStyles.xml"], p["ppt/tableStyles.xml"]);
}
