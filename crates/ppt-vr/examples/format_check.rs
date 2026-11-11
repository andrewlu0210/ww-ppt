//! Writes one deck that uses everything the Format Shape pane can set (fills, outlines, shadows,
//! text box settings, size and properties, slide backgrounds), to open in PowerPoint, and a
//! picture of each slide as this program draws it.
//! `cargo run -p ppt-vr --release --example format_check -- out-dir`
//! (the checklist is in `powerpoint-check.md`).

use ppt_editor::{Editor, FillSpec, LineSpec, ShadowSpec, TextBoxSpec, TextEffectSpec, TextOutlineSpec};
use ppt_model::{Anchor, Background, Color, Dash, EndKind, Gradient, GradientKind, LineEnd, Paint, Run, Xfrm};
use ppt_ooxml::{parse_pptx, save_pptx, template_package};
use ppt_text::TextEngine;
use ppt_vr::SlideRenderer;

const IN: f64 = 914_400.0;

fn engine() -> TextEngine {
    let mut engine = TextEngine::new(false);
    engine.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
    engine.register_font(std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc").unwrap());
    engine.set_alias("Calibri", "Helvetica");
    engine.set_fallbacks(vec!["Hiragino Sans GB".into()]);
    engine
}

fn inch(v: f64) -> i64 {
    (v * IN).round() as i64
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color { r, g, b, a }
}

/// Adds a shape of `preset` at (x, y) with size (w, h) in inches, with `text` in it, and selects it.
fn shape(e: &mut Editor, eng: &mut TextEngine, preset: &str, (x, y, w, h): (f64, f64, f64, f64), text: &str) {
    assert!(e.add_shape(eng, preset));
    assert!(e.set_selection_frame(Xfrm::new(inch(x), inch(y), inch(w), inch(h))));
    if !text.is_empty() {
        let style = e.pres.default_run_style();
        let slide = e.slide;
        let s = e.pres.slide_mut(slide).unwrap().shapes.last_mut().unwrap();
        let mut body = ppt_model::TextBody { defaults: None, ..Default::default() };
        body.paragraphs[0].runs.push(Run { text: text.into(), style: ppt_model::RunStyle { color: Color::WHITE, size_pt: 14.0, ..style } });
        s.text = Some(body);
    }
}

fn gradient(stops: Vec<(f32, Color)>, kind: GradientKind) -> Gradient {
    Gradient { stops, kind, rotate_with_shape: true }
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("out dir"));
    std::fs::create_dir_all(out.join("format-check")).unwrap();
    let mut eng = engine();
    let mut e = Editor::new(parse_pptx(&template_package()).unwrap());
    let mut titles: Vec<&str> = Vec::new();
    let blue = Color::rgb(0x1F, 0x4E, 0x9C);
    let orange = Color::rgb(0xE3, 0x6C, 0x0A);
    let green = Color::rgb(0x2E, 0x8B, 0x57);

    // 1. Fills.
    titles.push("填滿");
    let cell = |i: usize| (0.6 + (i % 4) as f64 * 3.1, 1.2 + (i / 4) as f64 * 2.9, 2.6, 2.1);
    shape(&mut e, &mut eng, "rect", cell(0), "1 單色");
    e.set_fill_spec(FillSpec::Solid(orange));
    shape(&mut e, &mut eng, "rect", cell(1), "2 線性 0°");
    e.set_fill_spec(FillSpec::Gradient(gradient(vec![(0.0, blue), (1.0, Color::rgb(0x9D, 0xC3, 0xE6))], GradientKind::Linear { angle: 0.0, scaled: false })));
    shape(&mut e, &mut eng, "rect", cell(2), "3 三色 45°，中間半透明");
    e.set_fill_spec(FillSpec::Gradient(gradient(vec![(0.0, orange), (0.5, rgba(0xFF, 0xFF, 0x00, 90)), (1.0, green)], GradientKind::Linear { angle: 45.0, scaled: false })));
    shape(&mut e, &mut eng, "rect", cell(3), "4 放射");
    e.set_fill_spec(FillSpec::Gradient(gradient(vec![(0.0, Color::rgb(0xFF, 0xD9, 0x66)), (1.0, Color::rgb(0xC0, 0x00, 0x00))], GradientKind::Path { circle: true, focus: [0.5, 0.5, 0.5, 0.5] })));
    // A dark bar behind a half-transparent fill, to see the transparency.
    shape(&mut e, &mut eng, "rect", (cell(4).0, cell(4).1 + 0.8, cell(4).2, 0.5), "");
    e.set_fill_spec(FillSpec::Solid(Color::rgb(0x20, 0x20, 0x20)));
    shape(&mut e, &mut eng, "rect", cell(4), "5 單色 50% 透明");
    e.set_fill_spec(FillSpec::Solid(rgba(0x1F, 0x4E, 0x9C, 128)));
    shape(&mut e, &mut eng, "rect", cell(5), "6 無填滿，有外框");
    e.set_fill_spec(FillSpec::None);
    e.set_line_spec(LineSpec { rgb: Some((0x1F, 0x4E, 0x9C)), width: Some(38_100), ..Default::default() });
    shape(&mut e, &mut eng, "ellipse", cell(6), "7 線性 90°");
    e.set_fill_spec(FillSpec::Gradient(gradient(vec![(0.0, Color::rgb(0xFF, 0xFF, 0xFF)), (1.0, green)], GradientKind::Linear { angle: 90.0, scaled: false })));
    shape(&mut e, &mut eng, "roundRect", cell(7), "8 四色 135°");
    e.set_fill_spec(FillSpec::Gradient(gradient(vec![(0.0, orange), (0.33, rgba(0xFF, 0xFF, 0x00, 255)), (0.66, green), (1.0, blue)], GradientKind::Linear { angle: 135.0, scaled: false })));

    // 2. Outlines.
    e.add_slide(&mut eng, None);
    titles.push("線條");
    let dashes = [Dash::Solid, Dash::Dot, Dash::Dash, Dash::LgDash, Dash::DashDot, Dash::LgDashDot, Dash::LgDashDotDot, Dash::SysDash, Dash::SysDot, Dash::SysDashDot];
    for (i, d) in dashes.iter().enumerate() {
        shape(&mut e, &mut eng, "line", (0.6, 0.6 + i as f64 * 0.42, 4.2, 0.0), "");
        e.set_line_spec(LineSpec { rgb: Some((0x1F, 0x4E, 0x9C)), width: Some(38_100), dash: Some(*d), ..Default::default() });
    }
    let ends = [EndKind::Triangle, EndKind::Stealth, EndKind::Diamond, EndKind::Oval, EndKind::Arrow];
    for (i, k) in ends.iter().enumerate() {
        let size = (i % 3) as u8;
        shape(&mut e, &mut eng, "line", (5.4, 0.6 + i as f64 * 0.7, 3.2, 0.0), "");
        e.set_line_spec(LineSpec {
            rgb: Some((0xC0, 0x00, 0x00)),
            width: Some(38_100),
            head: Some(LineEnd { kind: EndKind::None, width: 1, length: 1 }),
            tail: Some(LineEnd { kind: *k, width: size, length: size }),
            ..Default::default()
        });
    }
    // Both ends, and arrowheads at the start only.
    shape(&mut e, &mut eng, "line", (5.4, 4.2, 3.2, 0.0), "");
    e.set_line_spec(LineSpec { rgb: Some((0, 0x80, 0)), width: Some(38_100), head: Some(LineEnd { kind: EndKind::Triangle, width: 1, length: 1 }), tail: Some(LineEnd { kind: EndKind::Triangle, width: 1, length: 1 }), ..Default::default() });
    shape(&mut e, &mut eng, "line", (5.4, 4.7, 3.2, 0.0), "");
    e.set_line_spec(LineSpec { rgb: Some((0, 0x80, 0)), width: Some(38_100), head: Some(LineEnd { kind: EndKind::Oval, width: 2, length: 2 }), ..Default::default() });
    for (i, w) in [6_350_i64, 12_700, 38_100, 76_200, 152_400].iter().enumerate() {
        shape(&mut e, &mut eng, "line", (9.2, 0.6 + i as f64 * 0.5, 3.4, 0.0), "");
        e.set_line_spec(LineSpec { rgb: Some((0, 0, 0)), width: Some(*w), ..Default::default() });
    }
    shape(&mut e, &mut eng, "rect", (9.2, 3.4, 3.4, 1.6), "外框 50% 透明");
    e.set_fill_spec(FillSpec::Solid(Color::rgb(0xF2, 0xF2, 0xF2)));
    e.set_line_spec(LineSpec { rgb: Some((0xC0, 0, 0)), alpha: Some(128), width: Some(152_400), dash: Some(Dash::Dash), ..Default::default() });
    shape(&mut e, &mut eng, "rect", (0.6, 5.2, 3.0, 1.6), "無外框");
    e.set_line_spec(LineSpec { visible: Some(false), ..Default::default() });

    // 3. Shadows.
    e.add_slide(&mut eng, None);
    titles.push("陰影");
    let cell = |i: usize| (0.8 + (i % 3) as f64 * 4.1, 1.0 + (i / 3) as f64 * 3.1, 3.2, 2.1);
    let shadows: [(&str, ShadowSpec); 6] = [
        ("1 預設", ShadowSpec { visible: Some(true), ..Default::default() }),
        ("2 模糊 20pt，距離 4pt", ShadowSpec { blur: Some(254_000), dist: Some(50_800), ..Default::default() }),
        ("3 向下 12pt，角度 90°", ShadowSpec { dist: Some(152_400), dir: Some(90.0), blur: Some(63_500), ..Default::default() }),
        ("4 紅色 40%，角度 225°", ShadowSpec { rgb: Some((0xC0, 0, 0)), alpha: Some(102), dist: Some(127_000), dir: Some(225.0), blur: Some(76_200), ..Default::default() }),
        ("5 無模糊", ShadowSpec { blur: Some(0), dist: Some(76_200), dir: Some(0.0), ..Default::default() }),
        ("6 無陰影", ShadowSpec { visible: Some(false), ..Default::default() }),
    ];
    for (i, (name, spec)) in shadows.into_iter().enumerate() {
        shape(&mut e, &mut eng, "rect", cell(i), name);
        e.set_fill_spec(FillSpec::Solid(blue));
        e.set_shadow_spec(spec);
    }

    // 4. Text box settings.
    e.add_slide(&mut eng, None);
    titles.push("文字方塊");
    let long = "這是一段比較長的文字，用來看邊界、換行與自動調整大小的效果。This is longer text to see margins, wrapping and resize-to-fit.";
    shape(&mut e, &mut eng, "rect", (0.6, 0.8, 4.0, 2.2), "1 垂直置中 + 左右邊界 0.5 吋。長文字：換行");
    e.set_fill_spec(FillSpec::Solid(blue));
    e.set_text_box_spec(&mut eng, TextBoxSpec { anchor: Some(Anchor::Middle), insets: [Some(inch(0.5)), None, Some(inch(0.5)), None], ..Default::default() });
    shape(&mut e, &mut eng, "rect", (4.9, 0.8, 4.0, 2.2), "2 靠下對齊，上下邊界 0.4 吋");
    e.set_fill_spec(FillSpec::Solid(green));
    e.set_text_box_spec(&mut eng, TextBoxSpec { anchor: Some(Anchor::Bottom), insets: [None, Some(inch(0.4)), None, Some(inch(0.4))], ..Default::default() });
    shape(&mut e, &mut eng, "rect", (9.2, 0.8, 3.4, 2.2), &format!("3 不自動換行：{long}"));
    e.set_fill_spec(FillSpec::Solid(orange));
    e.set_text_box_spec(&mut eng, TextBoxSpec { wrap: Some(false), ..Default::default() });
    shape(&mut e, &mut eng, "rect", (0.6, 3.8, 4.0, 1.0), &format!("4 調整圖形大小以配合文字：{long}"));
    e.set_fill_spec(FillSpec::Solid(blue));
    e.set_text_box_spec(&mut eng, TextBoxSpec { auto_grow: Some(true), ..Default::default() });
    shape(&mut e, &mut eng, "rect", (4.9, 3.8, 4.0, 2.0), "5 預設值（對照組）");
    e.set_fill_spec(FillSpec::Solid(Color::rgb(0x59, 0x59, 0x59)));

    // 5. Size and properties.
    e.add_slide(&mut eng, None);
    titles.push("大小與屬性");
    shape(&mut e, &mut eng, "rect", (0.8, 1.2, 3.2, 2.0), "1 鎖定長寬比、替代文字");
    e.set_fill_spec(FillSpec::Solid(blue));
    e.set_lock_aspect(true);
    e.set_alt_text(&mut eng, "鎖定長寬比的藍色矩形", false);
    shape(&mut e, &mut eng, "rect", (4.6, 1.2, 3.2, 2.0), "2 裝飾用（替代文字）");
    e.set_fill_spec(FillSpec::Solid(green));
    e.set_alt_text(&mut eng, "", true);
    shape(&mut e, &mut eng, "rect", (8.4, 1.2, 3.2, 2.0), "3 旋轉 30°");
    e.set_fill_spec(FillSpec::Solid(orange));
    e.set_selection_frame(Xfrm { rot: 30.0, ..Xfrm::new(inch(8.4), inch(1.2), inch(3.2), inch(2.0)) });
    shape(&mut e, &mut eng, "rect", (0.8, 4.2, 3.2, 2.0), "4 位置鎖定");
    e.set_fill_spec(FillSpec::Solid(Color::rgb(0x59, 0x59, 0x59)));
    e.set_locked(&mut eng, true);

    // 6 and 7. Backgrounds.
    e.add_slide(&mut eng, None);
    titles.push("背景（線性漸層，三色）");
    shape(&mut e, &mut eng, "rect", (4.2, 2.8, 4.8, 1.6), "背景：線性 90°，三色");
    e.set_fill_spec(FillSpec::Solid(rgba(0x20, 0x20, 0x20, 200)));
    e.set_background(&mut eng, Some(Background::Paint(Paint::Gradient(std::sync::Arc::new(gradient(vec![(0.0, blue), (0.5, Color::rgb(0xFF, 0xFF, 0xFF)), (1.0, orange)], GradientKind::Linear { angle: 90.0, scaled: false }))))));
    e.add_slide(&mut eng, None);
    titles.push("背景（放射漸層）");
    shape(&mut e, &mut eng, "rect", (4.2, 2.8, 4.8, 1.6), "背景：放射");
    e.set_fill_spec(FillSpec::Solid(rgba(0x20, 0x20, 0x20, 200)));
    e.set_background(&mut eng, Some(Background::Paint(Paint::Gradient(std::sync::Arc::new(gradient(vec![(0.0, Color::rgb(0xFF, 0xFF, 0xFF)), (1.0, blue)], GradientKind::Path { circle: true, focus: [0.5, 0.5, 0.5, 0.5] }))))));

    // 8. Text outline and shadow (Text Options in the pane).
    e.add_slide(&mut eng, None);
    titles.push("文字效果");
    let effect_cell = |i: usize| (0.6 + (i % 2) as f64 * 6.2, 1.0 + (i / 2) as f64 * 1.9, 5.8, 1.6);
    let text_effect = |e: &mut Editor, eng: &mut TextEngine, i: usize, label: &str, hollow: bool, spec: TextEffectSpec| {
        shape(e, eng, "rect", effect_cell(i), label);
        e.set_fill_spec(FillSpec::Solid(Color::rgb(0xF2, 0xF2, 0xF2)));
        e.apply_text_style(eng, move |s| {
            s.size_pt = 32.0;
            s.bold = true;
            s.color = if hollow { rgba(0, 0, 0, 0) } else { Color::rgb(0x26, 0x26, 0x26) };
        });
        e.set_text_effect_spec(eng, spec);
    };
    let outline = |rgb: (u8, u8, u8), width_pt: f64| TextOutlineSpec { visible: Some(true), rgb: Some(rgb), width: Some((width_pt * 12_700.0) as i64), ..Default::default() };
    let shadow = |blur_pt: f64, dist_pt: f64, angle: f64, alpha: u8| ShadowSpec { visible: Some(true), rgb: Some((0, 0, 0)), alpha: Some(alpha), blur: Some((blur_pt * 12_700.0) as i64), dist: Some((dist_pt * 12_700.0) as i64), dir: Some(angle) };
    text_effect(&mut e, &mut eng, 0, "1 外框 紅 2pt", false, TextEffectSpec { outline: outline((0xC0, 0, 0), 2.0), ..Default::default() });
    text_effect(&mut e, &mut eng, 1, "2 空心字 藍外框", true, TextEffectSpec { outline: outline((0x1F, 0x4E, 0x9C), 1.5), ..Default::default() });
    text_effect(&mut e, &mut eng, 2, "3 陰影 硬 右下", false, TextEffectSpec { shadow: shadow(0.0, 4.0, 45.0, 160), ..Default::default() });
    text_effect(&mut e, &mut eng, 3, "4 陰影 模糊 6pt", false, TextEffectSpec { shadow: shadow(6.0, 5.0, 90.0, 140), ..Default::default() });
    text_effect(&mut e, &mut eng, 4, "5 外框加陰影", false, TextEffectSpec { outline: outline((0xE3, 0x6C, 0x0A), 1.0), shadow: shadow(3.0, 4.0, 45.0, 150) });
    text_effect(&mut e, &mut eng, 5, "6 對照組", false, TextEffectSpec::default());

    let bytes = save_pptx(&e.pres).unwrap();
    std::fs::write(out.join("format-check.pptx"), &bytes).unwrap();
    // As this program draws the saved deck.
    let pres = parse_pptx(&bytes).unwrap();
    let mut r = SlideRenderer::new().unwrap();
    r.ensure_fonts(pres.font_families());
    for (n, title) in titles.iter().enumerate() {
        r.render(&pres, &pres.slides[n], 960).unwrap().save(out.join("format-check").join(format!("{:02}-{title}.png", n + 1))).unwrap();
    }
    println!("| 投影片 | 內容 |");
    println!("|---|---|");
    for (n, title) in titles.iter().enumerate() {
        println!("| {} | {title} |", n + 1);
    }
}
