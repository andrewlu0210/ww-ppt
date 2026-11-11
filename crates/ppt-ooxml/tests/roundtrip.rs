//! Save → reopen round trips against the sample deck and the built-in template.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace

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
    let body = pres.slide_mut(1).unwrap().shapes[1].text.as_mut().unwrap();
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
    let shape = &mut pres.slide_mut(2).unwrap().shapes[2];
    shape.xfrm.x += 100_000;
    shape.xfrm.rot = 45.0;
    shape.fill = Some(Paint::Solid(Color::rgb(0x12, 0x34, 0x56)));
    shape.line = Some(Line::new(Color::rgb(1, 2, 3), 25_400));
    let expect = shape.xfrm;
    // Placeholder that inherits its position: moving it must add an xfrm.
    pres.slide_mut(0).unwrap().shapes[0].xfrm.y += 50_000;
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
    pres.slide_mut(2).unwrap().shapes[n - 1].xfrm.x += 200_000;
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
    for s in pres.slide_mut(2).unwrap().shapes.iter_mut().filter(|s| s.group == group) {
        s.group = None;
    }
    let expect: Vec<Xfrm> = pres.slides[2].shapes.iter().map(|s| s.xfrm).collect();
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides[2].shapes.iter().map(|s| s.xfrm).collect::<Vec<_>>(), expect);
    assert!(re.slides[2].shapes.iter().all(|s| s.group.is_none()), "ungrouped shapes stay ungrouped");

    // Group the first two shapes of slide 3 into a new group.
    let mut pres = re;
    let gid = pres.alloc_id();
    pres.slide_mut(2).unwrap().shapes[0].group = Some(gid);
    pres.slide_mut(2).unwrap().shapes[1].group = Some(gid);
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
    pres.slides.insert(1, std::sync::Arc::new(slide));
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
    pres.slide_mut(0).unwrap().shapes.push(pic);
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
    let body = pres.slide_mut(0).unwrap().shapes[0].text.as_mut().unwrap();
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
    pres.slide_mut(0).unwrap().shapes.push(tb);
    let layout = pres.slide_from_layout(5);
    pres.slides.push(std::sync::Arc::new(layout));

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
    pres.slide_mut(0).unwrap().shapes.push(shape);
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
    pres.slide_mut(0).unwrap().shapes.push(shape);
    let first = save_pptx(&pres).unwrap();
    let mut pres = parse_pptx(&first).unwrap();
    let ShapeKind::Table(t) = &mut pres.slide_mut(0).unwrap().shapes.last_mut().unwrap().kind else { panic!() };
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
    pres.slide_mut(0).unwrap().shapes.push(shape);
    let mut pres = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    let ShapeKind::Table(t) = &mut pres.slide_mut(0).unwrap().shapes.last_mut().unwrap().kind else { panic!() };
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
    for s in pres.slide_mut(2).unwrap().shapes.iter_mut().filter(|s| s.group == group) {
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
    pres.slide_mut(0).unwrap().shapes.push(shape);
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
    let body = re.slide_mut(0).unwrap().shapes.last_mut().unwrap().text.as_mut().unwrap();
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
    pres.slide_mut(0).unwrap().shapes.push(shape);
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

#[test]
fn notes_are_added_edited_and_duplicated() {
    // The template has no notes master: the first notes create one.
    let mut pres = parse_pptx(&template_package()).unwrap();
    pres.slide_mut(0).unwrap().notes = "第一頁的備忘稿\n\n第三行 <&>".into();
    let mut second = pres.slide_from_layout(1);
    second.notes = "second".into();
    pres.slides.push(std::sync::Arc::new(second));
    let blank = pres.slide_from_layout(5);
    pres.slides.push(std::sync::Arc::new(blank));

    let saved = save_pptx(&pres).unwrap();
    let files = parts(&saved);
    let pres_xml = String::from_utf8(files["ppt/presentation.xml"].clone()).unwrap();
    let (masters, notes_masters, slides) = (pres_xml.find("<p:sldMasterIdLst").unwrap(), pres_xml.find("<p:notesMasterIdLst").unwrap(), pres_xml.find("<p:sldIdLst").unwrap());
    assert!(masters < notes_masters && notes_masters < slides, "{pres_xml}");
    let ct = String::from_utf8(files["[Content_Types].xml"].clone()).unwrap();
    assert!(ct.contains("/ppt/notesMasters/notesMaster1.xml") && ct.contains("/ppt/notesSlides/notesSlide2.xml"), "{ct}");
    assert!(files.contains_key("ppt/notesMasters/_rels/notesMaster1.xml.rels"));
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides.iter().map(|s| s.notes.as_str()).collect::<Vec<_>>(), ["第一頁的備忘稿\n\n第三行 <&>", "second", ""]);

    // Editing existing notes, adding notes to a slide without them, duplicating a slide with notes.
    let mut pres = re;
    pres.slide_mut(0).unwrap().notes = "改過了".into();
    pres.slide_mut(2).unwrap().notes = "後來加的".into();
    let dup = pres.slides[1].clone();
    pres.slides.push(dup);
    let saved2 = save_pptx(&pres).unwrap();
    let files2 = parts(&saved2);
    // The existing notes master is reused, and untouched notes keep their part.
    assert!(!files2.contains_key("ppt/notesMasters/notesMaster2.xml"));
    assert_eq!(files["ppt/notesSlides/notesSlide2.xml"], files2["ppt/notesSlides/notesSlide2.xml"]);
    let re = parse_pptx(&saved2).unwrap();
    assert_eq!(re.slides.iter().map(|s| s.notes.as_str()).collect::<Vec<_>>(), ["改過了", "second", "後來加的", "second"]);
    // Deleting a slide drops its notes.
    let mut pres = re;
    pres.slides.remove(0);
    let files3 = parts(&save_pptx(&pres).unwrap());
    assert!(!files3.contains_key("ppt/notesSlides/notesSlide1.xml"));
}

/// The template with a chart part on its first slide.
fn template_with_chart() -> Vec<u8> {
    use std::io::Write;
    let src = template_package();
    let mut zip = zip::ZipArchive::new(Cursor::new(src.as_slice())).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut text = String::new();
        f.read_to_string(&mut text).unwrap();
        match name.as_str() {
            "[Content_Types].xml" => {
                text = text.replace("</Types>", r#"<Override PartName="/ppt/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>"#)
            }
            "ppt/slides/_rels/slide1.xml.rels" => {
                text = text.replace("</Relationships>", r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/></Relationships>"#)
            }
            "ppt/slides/slide1.xml" => {
                text = text.replace(
                    "</p:spTree>",
                    r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Chart 1"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="4000000" cy="3000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId9"/></a:graphicData></a:graphic></p:graphicFrame></p:spTree>"#,
                )
            }
            _ => {}
        }
        out.start_file(name, opts).unwrap();
        out.write_all(text.as_bytes()).unwrap();
    }
    out.start_file("ppt/charts/chart1.xml", opts).unwrap();
    out.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea/></c:chart></c:chartSpace>"#).unwrap();
    out.finish().unwrap().into_inner()
}

#[test]
fn duplicated_chart_slides_get_their_own_chart_part() {
    let mut pres = parse_pptx(&template_with_chart()).unwrap();
    assert!(pres.slides[0].shapes.iter().any(|s| s.name == "Chart 1"));
    // Duplicate placed before the original, plus a second copy of the chart on the original slide.
    let dup = pres.slides[0].clone();
    pres.slides.insert(0, dup);
    let mut again = pres.slides[1].shapes.iter().find(|s| s.name == "Chart 1").unwrap().clone();
    again.id = pres.alloc_id();
    pres.slide_mut(1).unwrap().shapes.push(again);

    let files = parts(&save_pptx(&pres).unwrap());
    let text = |n: &str| String::from_utf8(files[n].clone()).unwrap();
    let chart_targets = |rels: &str| rels.split("Target=\"").skip(1).filter_map(|t| t.split('"').next()).filter(|t| t.contains("charts/")).map(str::to_string).collect::<Vec<_>>();
    // Three chart frames across the two slides: three distinct chart parts.
    let mut all: Vec<String> = files
        .keys()
        .filter(|n| n.starts_with("ppt/slides/_rels/"))
        .flat_map(|n| chart_targets(&text(n)))
        .collect();
    assert_eq!(all.len(), 3, "{all:?}");
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3, "{all:?}");
    let ct = text("[Content_Types].xml");
    for t in &all {
        assert!(ct.contains(&format!("/ppt/charts/{}", t.rsplit('/').next().unwrap())), "{t} has no content type");
    }
}

#[test]
fn transitions_round_trip_and_edit() {
    use ppt_model::transition::{Dir, Transition, TransitionKind};
    let mut pres = parse_pptx(&template_package()).unwrap();
    let push = Transition { kind: TransitionKind::Push(Dir::Up), duration_ms: 1000, on_click: true, after_ms: None };
    let wipe = Transition { kind: TransitionKind::Wipe(Dir::Down), duration_ms: 1300, on_click: false, after_ms: Some(4000) };
    pres.slide_mut(0).unwrap().transition = Some(push.clone());
    let mut second = pres.slide_from_layout(1);
    second.transition = Some(wipe.clone());
    pres.slides.push(std::sync::Arc::new(second));
    let re = parse_pptx(&save_pptx(&pres).unwrap()).unwrap();
    assert_eq!(re.slides[0].transition.as_ref(), Some(&push));
    assert_eq!(re.slides[1].transition.as_ref(), Some(&wipe));

    // Unchanged slides keep their part; an edited transition replaces the old one; None removes it.
    let saved = save_pptx(&re).unwrap();
    let mut pres = parse_pptx(&saved).unwrap();
    pres.slide_mut(0).unwrap().transition = Some(Transition { kind: TransitionKind::Circle, ..push });
    pres.slide_mut(1).unwrap().transition = None;
    let saved2 = save_pptx(&pres).unwrap();
    let re = parse_pptx(&saved2).unwrap();
    assert_eq!(re.slides[0].transition.as_ref().map(|t| &t.kind), Some(&TransitionKind::Circle));
    assert_eq!(re.slides[1].transition, None);
    let xml = String::from_utf8(parts(&saved2)["ppt/slides/slide1.xml"].clone()).unwrap();
    assert_eq!(xml.matches("<p:transition").count(), 1, "{xml}");
}

#[test]
fn animations_are_written_kept_and_edited() {
    use ppt_model::anim::{Player, Target, Timing, Trigger, build_effect, effect_duration, set_effect_duration};
    let mut pres = sample();
    let shapes: Vec<_> = pres.slides[0].shapes.iter().map(|s| (s.id, s.text.is_some())).collect();
    let (a, b) = (shapes[0].0, shapes[1].0);
    let target = |id| Target { element: id, shapes: vec![id], paragraphs: None };
    let mut fly = build_effect("entr", 2, Some(8), &target(a)).unwrap();
    fly.grp_id = Some(0);
    let mut spin = build_effect("emph", 8, None, &target(b)).unwrap();
    spin.node_type = Trigger::After.node_type().into();
    spin.delay_ms = 300.0;
    let mut out = build_effect("exit", 10, None, &target(a)).unwrap();
    out.grp_id = Some(1);
    let mut t = Timing::new();
    t.set_effects(vec![fly, spin, out]);
    pres.slide_mut(0).unwrap().timing = Some(std::sync::Arc::new(t));

    let saved = save_pptx(&pres).unwrap();
    let xml = slide_xml(&saved, 1);
    assert!(roxmltree::Document::parse(&xml).is_ok());
    let ids: Vec<&str> = xml.match_indices("<p:cTn id=\"").map(|(i, _)| xml[i + 11..].split('"').next().unwrap()).collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "cTn ids must be unique: {xml}");
    if shapes[0].1 {
        assert!(xml.contains(r#"grpId="1" animBg="1"/>"#), "{xml}");
    }
    let re = parse_pptx(&saved).unwrap();
    let timing = re.slides[0].timing.as_deref().unwrap();
    let effects = timing.effects();
    assert_eq!(effects.iter().map(|e| (e.preset_class.as_str(), e.preset_id, Trigger::of(e))).collect::<Vec<_>>(), vec![("entr", Some(2), Trigger::Click), ("emph", Some(8), Trigger::After), ("exit", Some(10), Trigger::Click)]);
    assert_eq!((effects[1].delay_ms, effect_duration(effects[1])), (300.0, 2000.0));
    assert_eq!(Player::new(timing).clicks(), 2);

    // Unedited animations save as they were.
    let again = save_pptx(&re).unwrap();
    assert_eq!(slide_xml(&again, 1), xml);

    // Editing one effect rewrites the sequence but keeps the others' XML.
    let mut pres = re;
    let mut t = pres.slides[0].timing.as_deref().unwrap().clone();
    let mut effects: Vec<_> = t.effects().into_iter().cloned().collect();
    set_effect_duration(&mut effects[0], 1500.0);
    effects[2].node_type = Trigger::With.node_type().into();
    effects.remove(1);
    t.set_effects(effects);
    pres.slide_mut(0).unwrap().timing = Some(std::sync::Arc::new(t));
    let saved = save_pptx(&pres).unwrap();
    let xml2 = slide_xml(&saved, 1);
    assert_eq!(xml2.matches("<p:timing>").count(), 1);
    let re = parse_pptx(&saved).unwrap();
    let effects = re.slides[0].timing.as_deref().unwrap().effects().into_iter().cloned().collect::<Vec<_>>();
    assert_eq!(effects.len(), 2);
    assert_eq!((effect_duration(&effects[0]), Trigger::of(&effects[1])), (1500.0, Trigger::With));
    assert!(xml2.contains(r#"dur="1500""#) && !xml2.contains(r#"presetID="8""#), "{xml2}");

    // Without effects the timing goes away.
    let mut pres = re;
    let mut t = pres.slides[0].timing.as_deref().unwrap().clone();
    t.set_effects(Vec::new());
    pres.slide_mut(0).unwrap().timing = (!t.is_empty()).then(|| std::sync::Arc::new(t));
    let xml3 = slide_xml(&save_pptx(&pres).unwrap(), 1);
    assert!(!xml3.contains("<p:timing"), "{xml3}");
}

#[test]
fn strike_scripts_spacing_and_anchor_round_trip() {
    let mut pres = sample();
    let i = pres.slides[0].shapes.iter().position(|s| s.text.as_ref().is_some_and(|t| t.len() > 4)).unwrap();
    {
        let body = pres.slide_mut(0).unwrap().shapes[i].text.as_mut().unwrap();
        body.apply_style(0..1, |s| s.strike = true);
        body.apply_style(1..2, |s| s.baseline = 0.3);
        body.apply_style(2..3, |s| s.baseline = -0.25);
        body.apply_style(3..4, |s| s.spacing_pt = 2.5);
        body.anchor = ppt_model::Anchor::Bottom;
    }
    let saved = save_pptx(&pres).unwrap();
    let xml = slide_xml(&saved, 1);
    assert!(xml.contains(r#"strike="sngStrike""#) && xml.contains(r#"baseline="30000""#) && xml.contains(r#"baseline="-25000""#) && xml.contains(r#"spc="250""#), "{xml}");
    let re = parse_pptx(&saved).unwrap();
    let body = re.slides[0].shapes[i].text.as_ref().unwrap();
    let at = |pos: usize| body.styles_in(pos..pos + 1)[0].clone();
    assert!(at(0).strike && !at(1).strike);
    assert_eq!((at(1).baseline, at(2).baseline, at(3).spacing_pt, at(4).baseline), (0.3, -0.25, 2.5, 0.0));
    assert_eq!(body.anchor, ppt_model::Anchor::Bottom);
}

#[test]
fn geometry_shadow_flip_and_crop_edits_round_trip() {
    use ppt_model::{Crop, Geometry, Shadow};
    let mut pres = sample();
    let (si, pi) = (2, 3);
    let shape = pres.slides[si].shapes.iter().position(|s| matches!(s.kind, ShapeKind::Shape) && s.fill.is_some() && !s.geometry.is_line()).unwrap();
    let pic = pres.slides[pi].shapes.iter().position(|s| matches!(s.kind, ShapeKind::Picture { .. })).unwrap();
    let shadow = Shadow { color: Color { r: 0, g: 0, b: 0, a: 102 }, blur: 50_800, dist: 38_100, dir: 45.0 };
    {
        let s = &mut pres.slide_mut(si).unwrap().shapes[shape];
        s.geometry = Geometry::preset("ellipse");
        s.shadow = Some(shadow);
        s.xfrm.flip_h = true;
        s.xfrm.rot = 15.0;
    }
    if let ShapeKind::Picture { crop, .. } = &mut pres.slide_mut(pi).unwrap().shapes[pic].kind {
        *crop = Crop { left: 0.1, top: 0.2, right: 0.0, bottom: 0.05 };
    }
    let saved = save_pptx(&pres).unwrap();
    let re = parse_pptx(&saved).unwrap();
    let s = &re.slides[si].shapes[shape];
    assert_eq!(s.geometry, Geometry::preset("ellipse"));
    assert_eq!(s.shadow, Some(shadow));
    assert!(s.xfrm.flip_h && (s.xfrm.rot - 15.0).abs() < 1e-6);
    let ShapeKind::Picture { crop, .. } = &re.slides[pi].shapes[pic].kind else { panic!() };
    assert_eq!(*crop, Crop { left: 0.1, top: 0.2, right: 0.0, bottom: 0.05 });
    // Removing the shadow writes an explicit empty effect list; removing the crop drops srcRect.
    let mut pres = re;
    pres.slide_mut(si).unwrap().shapes[shape].shadow = None;
    if let ShapeKind::Picture { crop, .. } = &mut pres.slide_mut(pi).unwrap().shapes[pic].kind {
        *crop = Crop::default();
    }
    let saved = save_pptx(&pres).unwrap();
    assert!(slide_xml(&saved, si + 1).contains("<a:effectLst/>"));
    assert!(!slide_xml(&saved, pi + 1).contains("srcRect"));
    let re = parse_pptx(&saved).unwrap();
    assert_eq!(re.slides[si].shapes[shape].shadow, None);
}
