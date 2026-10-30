//! Dev tool: `cargo run -p ppt-ooxml --example resave -- in.pptx out.pptx [--edit]`.
//! Re-saves a deck; with `--edit` it also changes text, moves a shape and adds a slide;
//! with `--anim` it edits the first animated slide's effects and adds a slide with new ones.

use std::sync::Arc;

use ppt_model::anim::{Target, Timing, Trigger, build_effect, effect_duration, set_effect_duration};
use ppt_model::{Color, Geometry, Paint, Shape, Xfrm};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let mut pres = match input.as_str() {
        "template" => ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap(),
        path => ppt_ooxml::parse_pptx(&std::fs::read(path).unwrap()).unwrap(),
    };
    if args.iter().any(|a| a == "--edit") {
        if let Some(body) = pres.slides[0].shapes.iter_mut().find_map(|s| s.text.as_mut()) {
            let len = body.len();
            let style = body.style_at(0);
            body.delete(0..len);
            body.insert(0, "已由 ww-ppt 修改", &style);
        }
        if let Some(s) = pres.slides[0].shapes.last_mut() {
            s.xfrm.y += 300_000;
        }
        let layout = pres.layouts.len().min(2).saturating_sub(1);
        let mut slide = pres.slide_from_layout(layout);
        let id = pres.alloc_id();
        let mut rect = Shape::new(id, "rect", Xfrm::new(1_000_000, 1_000_000, 3_000_000, 1_500_000));
        rect.geometry = Geometry::preset("roundRect");
        rect.fill = Some(Paint::Solid(Color::rgb(0xED, 0x7D, 0x31)));
        slide.shapes.push(rect);
        if let Some(body) = slide.shapes.iter_mut().find_map(|s| s.text.as_mut()) {
            let style = body.style_at(0);
            body.insert(0, "新投影片標題", &style);
        }
        pres.slides.push(slide);
    }
    if args.iter().any(|a| a == "--anim") {
        edit_animations(&mut pres);
    }
    std::fs::write(output, ppt_ooxml::save_pptx(&pres).unwrap()).unwrap();
    println!("saved {output} ({} slides)", pres.slides.len());
}

fn edit_animations(pres: &mut ppt_model::Presentation) {
    let target = |id| Target { element: id, shapes: vec![id], paragraphs: None };
    if let Some(slide) = pres.slides.iter_mut().find(|s| s.timing.is_some()) {
        let mut t = slide.timing.as_deref().unwrap().clone();
        let mut effects: Vec<_> = t.effects().into_iter().cloned().collect();
        // Kept XML with a new trigger, and with stretched times.
        effects[1].node_type = Trigger::After.node_type().into();
        effects[1].delay_ms = 500.0;
        let d = effect_duration(&effects[2]);
        set_effect_duration(&mut effects[2], d * 2.0);
        // Replaced by a built-in effect on the same shape.
        let el = ppt_model::anim::effect_elements(&effects[3])[0];
        let mut fly = build_effect("entr", 2, Some(8), &target(el)).unwrap();
        fly.grp_id = effects[3].grp_id;
        effects[3] = fly;
        effects.pop();
        // A new emphasis with the first effect.
        let first = ppt_model::anim::effect_elements(&effects[0])[0];
        let mut spin = build_effect("emph", 8, None, &target(first)).unwrap();
        spin.node_type = Trigger::With.node_type().into();
        spin.grp_id = Some(1);
        effects.insert(1, spin);
        t.set_effects(effects);
        slide.timing = Some(Arc::new(t));
    }
    // A new slide with new effects.
    let layout = pres.layouts.len().min(2).saturating_sub(1);
    let mut slide = pres.slide_from_layout(layout);
    let id = pres.alloc_id();
    let mut rect = Shape::new(id, "rect", Xfrm::new(1_000_000, 2_000_000, 3_000_000, 1_500_000));
    rect.fill = Some(Paint::Solid(Color::rgb(0x44, 0x72, 0xC4)));
    slide.shapes.push(rect);
    let mut effects = Vec::new();
    for (i, (class, preset, sub)) in [("entr", 10, None), ("entr", 22, Some(8)), ("emph", 6, None), ("exit", 2, Some(4))].into_iter().enumerate() {
        let el = if i == 1 { slide.shapes[0].id } else { id };
        let mut e = build_effect(class, preset, sub, &target(el)).unwrap();
        e.grp_id = Some(if el == id { i as u32 } else { 0 });
        if i == 2 {
            e.node_type = Trigger::After.node_type().into();
        }
        effects.push(e);
    }
    let mut t = Timing::new();
    t.set_effects(effects);
    slide.timing = Some(Arc::new(t));
    pres.slides.push(slide);
}
