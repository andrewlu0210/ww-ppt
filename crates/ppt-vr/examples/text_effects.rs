//! Draws text with outlines and shadows to a PNG, to look at.
//! `cargo run -p ppt-vr --release --example text_effects -- out.png` (also writes out.pdf next to it).

use ppt_editor::Editor;
use ppt_model::{Color, Run, Shadow, TextBody, TextOutline, Xfrm};
use ppt_ooxml::{parse_pptx, template_package};
use ppt_text::TextEngine;
use ppt_vr::SlideRenderer;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("out png"));
    let mut eng = TextEngine::new(false);
    eng.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
    eng.set_alias("Calibri", "Helvetica");
    let mut e = Editor::new(parse_pptx(&template_package()).unwrap());
    let rows: [(&str, Option<TextOutline>, Option<Shadow>); 5] = [
        ("Plain text", None, None),
        ("Outline only", Some(TextOutline { color: Color::rgb(200, 0, 0), width: 25_400 }), None),
        ("Hollow outline", Some(TextOutline { color: Color::rgb(0, 80, 200), width: 19_050 }), None),
        ("Hard shadow", None, Some(Shadow { color: Color { r: 0, g: 0, b: 0, a: 160 }, blur: 0, dist: 50_800, dir: 45.0 })),
        ("Soft shadow + outline", Some(TextOutline { color: Color::rgb(255, 255, 255), width: 12_700 }), Some(Shadow { color: Color { r: 0, g: 0, b: 0, a: 200 }, blur: 101_600, dist: 63_500, dir: 45.0 })),
    ];
    for (i, (text, outline, shadow)) in rows.iter().enumerate() {
        assert!(e.add_shape(&mut eng, "rect"));
        assert!(e.set_selection_frame(Xfrm::new(500_000, 300_000 + i as i64 * 1_250_000, 11_000_000, 1_100_000)));
        let mut style = e.pres.default_run_style();
        style.size_pt = 44.0;
        style.bold = true;
        style.color = if *text == "Hollow outline" { Color { r: 0, g: 0, b: 0, a: 0 } } else if shadow.is_some() && outline.is_some() { Color::rgb(60, 120, 220) } else { Color::rgb(30, 30, 30) };
        style.outline = *outline;
        style.shadow = *shadow;
        let mut body = TextBody { defaults: None, ..Default::default() };
        body.paragraphs[0].runs.push(Run { text: text.to_string(), style: style.clone() });
        body.paragraphs[0].end_style = style;
        let slide = e.slide;
        let s = e.pres.slide_mut(slide).unwrap().shapes.last_mut().unwrap();
        s.fill = None;
        s.line = None;
        s.text = Some(body);
    }
    let pres = e.pres.clone();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    r.render(&pres, &pres.slides[0], 1100).unwrap().save(&out).unwrap();
    std::fs::write(out.with_extension("pdf"), r.export_pdf(&pres).unwrap()).unwrap();
}
