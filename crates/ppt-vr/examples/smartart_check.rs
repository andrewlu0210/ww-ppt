//! Writes one deck with a slide for every SmartArt layout that can be inserted,
//! each edited (a node added, a child added, a node removed), to open in
//! PowerPoint, and a picture of each slide as this program draws it.
//! `cargo run -p ppt-vr --release --example smartart_check -- out-dir`
//! (the checklist is in `powerpoint-check.md`).

use ppt_editor::Editor;
use ppt_model::diagram::Insert;
use ppt_ooxml::{parse_pptx, save_pptx, smartart_kinds, template_package};
use ppt_text::TextEngine;
use ppt_vr::SlideRenderer;

fn engine() -> TextEngine {
    let mut engine = TextEngine::new(false);
    engine.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
    engine.register_font(std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc").unwrap());
    engine.set_alias("Calibri", "Helvetica");
    engine.set_fallbacks(vec!["Hiragino Sans GB".into()]);
    engine
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("out dir"));
    std::fs::create_dir_all(out.join("smartart-all")).unwrap();
    let mut eng = engine();
    let mut e = Editor::new(parse_pptx(&template_package()).unwrap());
    let mut rows = Vec::new();
    for (n, kind) in smartart_kinds().into_iter().enumerate() {
        // The first slide of a new deck is used for the first layout.
        if n > 0 || !e.pres.slides.is_empty() {
            e.add_slide(&mut eng, None);
        }
        assert!(e.insert_smartart(&mut eng, kind), "{kind}");
        let start = e.diagram_info().map(|i| i.rows.len()).unwrap_or(0);
        let tops = |e: &Editor| e.diagram_info().map(|i| i.rows.iter().filter(|r| r.depth == 0).count()).unwrap_or(0);
        let single_root = e.diagram_info().is_some_and(|i| i.single_root);
        // A: another top-level node (an org chart's goes under its root).
        let last = e.diagram_info().and_then(|i| i.rows.iter().rposition(|r| r.depth == 0)).unwrap_or(0);
        if let Some(r) = e.diagram_insert(&mut eng, last, if single_root { Insert::Child } else { Insert::After }) {
            e.diagram_set_text(&mut eng, r, "新增 A");
        }
        // B: a child under the first node, where the layout has levels.
        let mut child = false;
        if e.diagram_info().is_some_and(|i| i.nest)
            && let Some(r) = e.diagram_insert(&mut eng, if single_root { 1 } else { 0 }, Insert::Child)
        {
            e.diagram_set_text(&mut eng, r, "新增 B");
            child = true;
        }
        // C: one node fewer, from the top level's middle when there are enough.
        let mut removed = false;
        if tops(&e) >= 4 {
            let rows = e.diagram_info().unwrap().rows;
            let at = rows.iter().enumerate().filter(|(_, r)| r.depth == 0).nth(1).map(|(i, _)| i).unwrap();
            removed = e.diagram_delete(&mut eng, at).is_some();
        }
        let end = e.diagram_info().map(|i| i.rows.len()).unwrap_or(0);
        rows.push((n + 1, kind, start, end, child, removed));
    }
    let bytes = save_pptx(&e.pres).unwrap();
    std::fs::write(out.join("smartart-all.pptx"), &bytes).unwrap();
    // As this program draws the saved deck.
    let pres = parse_pptx(&bytes).unwrap();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    for (n, kind, ..) in &rows {
        r.render(&pres, &pres.slides[n - 1], 960).unwrap().save(out.join("smartart-all").join(format!("{n:02}-{kind}.png"))).unwrap();
    }
    println!("| 投影片 | 版面 | 插入時節點數 | 編輯後節點數 | 加了子項目 | 刪了節點 |");
    println!("|---|---|---|---|---|---|");
    for (n, kind, start, end, child, removed) in rows {
        println!("| {n} | `{kind}` | {start} | {end} | {} | {} |", if child { "是" } else { "否" }, if removed { "是" } else { "否" });
    }
}
