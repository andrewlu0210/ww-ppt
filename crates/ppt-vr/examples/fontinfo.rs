//! Prints how a font family resolves and which faces get registered.
#![allow(clippy::unwrap_used)] // test and tool code: a failure should stop with a backtrace
use ppt_fonts::FontService;
use ppt_text::TextEngine;

fn main() {
    let fonts = FontService::new();
    for name in std::env::args().skip(1) {
        let Some(r) = fonts.resolve(&name) else {
            println!("{name}: unresolved");
            continue;
        };
        println!("{name} -> {}", r.family);
        let mut engine = TextEngine::new(false);
        for p in &r.paths {
            let names = engine.register_font(std::fs::read(p).unwrap());
            println!("  {p}: {names:?}");
        }
        let coll = &mut engine.font_cx.collection;
        if let Some(fam) = coll.family_by_name(&r.family) {
            for f in fam.fonts() {
                println!("    weight {:?} style {:?}", f.weight(), f.style());
            }
        }
    }
}
