use super::*;
use ppt_model::{Layout, Placeholder, Presentation, Slide};

const TOL: Tolerance = Tolerance { handle: 50_000.0, rotate_offset: 200_000.0, snap: 30_000.0 };
const IN: i64 = 914_400;

fn engine() -> TextEngine {
    let mut engine = TextEngine::new(false);
    engine.register_font(std::fs::read("/System/Library/Fonts/Helvetica.ttc").unwrap());
    engine.register_font(std::fs::read("/System/Library/Fonts/Hiragino Sans GB.ttc").unwrap());
    engine.set_alias("Calibri", "Helvetica");
    engine.set_fallbacks(vec!["Hiragino Sans GB".into()]);
    engine
}

/// One slide with a 1in × 1in rectangle at (1in, 1in).
fn editor() -> (Editor, ShapeId) {
    let mut pres = Presentation::default();
    let id = pres.alloc_id();
    let mut shape = Shape::new(id, "rect", Xfrm::new(IN, IN, IN, IN));
    shape.fill = Some(Paint::Solid(Color::BLACK));
    pres.slides.push(std::sync::Arc::new(Slide { shapes: vec![shape], ..Default::default() }));
    (Editor::new(pres), id)
}

fn add_rect(e: &mut Editor, x: i64, y: i64) -> ShapeId {
    let id = e.pres.alloc_id();
    e.pres.slide_mut(0).unwrap().shapes.push(Shape::new(id, "r", Xfrm::new(x, y, IN, IN)));
    id
}

fn ev(x: f64, y: f64) -> PointerEvent {
    PointerEvent { x, y, clicks: 1, ..Default::default() }
}

fn xfrm(e: &Editor, id: ShapeId) -> Xfrm {
    e.pres.shape(0, id).unwrap().xfrm
}

/// Drag with smart guides disabled (alt), so positions are exact.
fn drag(e: &mut Editor, eng: &mut TextEngine, from: (f64, f64), to: (f64, f64), shift: bool) {
    e.pointer_down(eng, &ev(from.0, from.1), &TOL);
    e.pointer_move(eng, &PointerEvent { shift, alt: true, ..ev(to.0, to.1) }, &TOL);
    e.pointer_up(eng, &ev(to.0, to.1));
}

#[test]
fn click_selects_and_drag_moves_with_undo() {
    let (mut e, id) = editor();
    let mut eng = engine();
    drag(&mut e, &mut eng, (1_300_000.0, 1_300_000.0), (1_400_000.0, 1_350_000.0), false);
    assert_eq!(e.selection, vec![id]);
    assert_eq!((xfrm(&e, id).x, xfrm(&e, id).y), (1_014_400, 964_400));
    assert!(e.undo(&mut eng));
    assert_eq!((xfrm(&e, id).x, xfrm(&e, id).y), (IN, IN));
    assert!(e.redo(&mut eng));
    assert_eq!(xfrm(&e, id).x, 1_014_400);
}

#[test]
fn click_on_empty_area_deselects_and_plain_click_has_no_undo() {
    let (mut e, _) = editor();
    let mut eng = engine();
    e.pointer_down(&mut eng, &ev(1_300_000.0, 1_300_000.0), &TOL);
    e.pointer_up(&mut eng, &ev(1_300_000.0, 1_300_000.0));
    assert!(!e.can_undo());
    e.pointer_down(&mut eng, &ev(5_000_000.0, 5_000_000.0), &TOL);
    e.pointer_up(&mut eng, &ev(5_000_000.0, 5_000_000.0));
    assert!(e.selection.is_empty());
}

#[test]
fn shift_click_and_marquee_build_multi_selection() {
    let (mut e, a) = editor();
    let mut eng = engine();
    let b = add_rect(&mut e, 3 * IN, IN);
    let pa = (1.5 * IN as f64, 1.5 * IN as f64);
    let pb = (3.5 * IN as f64, 1.5 * IN as f64);
    e.pointer_down(&mut eng, &ev(pa.0, pa.1), &TOL);
    e.pointer_up(&mut eng, &ev(pa.0, pa.1));
    e.pointer_down(&mut eng, &PointerEvent { shift: true, ..ev(pb.0, pb.1) }, &TOL);
    e.pointer_up(&mut eng, &ev(pb.0, pb.1));
    assert_eq!(e.selection, vec![a, b]);
    // Dragging one moves both.
    drag(&mut e, &mut eng, pa, (pa.0 + 100_000.0, pa.1), false);
    assert_eq!(xfrm(&e, a).x, IN + 100_000);
    assert_eq!(xfrm(&e, b).x, 3 * IN + 100_000);

    // Marquee selects only fully enclosed shapes.
    e.pointer_down(&mut eng, &ev(10.0, 10.0), &TOL);
    e.pointer_move(&mut eng, &ev(2.5 * IN as f64, 2.5 * IN as f64), &TOL);
    assert!(e.marquee.is_some());
    e.pointer_up(&mut eng, &ev(2.5 * IN as f64, 2.5 * IN as f64));
    assert_eq!(e.selection, vec![a]);
    assert!(e.marquee.is_none());
}

#[test]
fn smart_guides_snap_to_other_shapes() {
    let (mut e, a) = editor();
    let mut eng = engine();
    let b = add_rect(&mut e, 3 * IN, 3 * IN);
    // Land `a`'s left edge 20 000 EMU short of `b`'s left edge: it snaps onto it.
    let start = (1.5 * IN as f64, 1.5 * IN as f64);
    e.pointer_down(&mut eng, &ev(start.0, start.1), &TOL);
    e.pointer_move(&mut eng, &ev(start.0 + 2.0 * IN as f64 - 20_000.0, start.1 + 500_000.0), &TOL);
    assert_eq!(xfrm(&e, a).x, xfrm(&e, b).x);
    assert!(e.guides.iter().any(|g| g.vertical && g.pos == 3.0 * IN as f64));
    e.pointer_up(&mut eng, &ev(0.0, 0.0));
    assert!(e.guides.is_empty());
}

#[test]
fn resize_from_corner_and_keep_aspect() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    drag(&mut e, &mut eng, (2.0 * IN as f64, 2.0 * IN as f64), (3.0 * IN as f64, 2.5 * IN as f64), false);
    let x = xfrm(&e, id);
    assert_eq!((x.x, x.y, x.w, x.h), (IN, IN, 2 * IN, 3 * IN / 2));
    e.pointer_down(&mut eng, &ev(IN as f64, IN as f64), &TOL);
    e.pointer_move(&mut eng, &PointerEvent { shift: true, ..ev(457_200.0, 800_000.0) }, &TOL);
    let x = xfrm(&e, id);
    assert!(((x.w as f64 / x.h as f64) - 4.0 / 3.0).abs() < 1e-3);
    assert_eq!((x.x + x.w, x.y + x.h), (3 * IN, 5 * IN / 2), "opposite corner stays fixed");
}

#[test]
fn resizing_a_rotated_shape_keeps_the_opposite_corner() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.pres.shape_mut(0, id).unwrap().xfrm.rot = 30.0;
    e.selection = vec![id];
    let before = xfrm(&e, id);
    let fixed = before.to_slide(0.0, 0.0);
    let se = before.to_slide(before.w as f64, before.h as f64);
    e.pointer_down(&mut eng, &ev(se.0, se.1), &TOL);
    e.pointer_move(&mut eng, &ev(se.0 + 300_000.0, se.1 + 100_000.0), &TOL);
    let after = xfrm(&e, id);
    let moved = after.to_slide(0.0, 0.0);
    assert!((moved.0 - fixed.0).abs() < 2.0 && (moved.1 - fixed.1).abs() < 2.0, "{fixed:?} vs {moved:?}");
    assert!(after.w > before.w);
}

#[test]
fn rotate_handle_snaps_with_shift() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    let (cx, cy) = xfrm(&e, id).center();
    e.pointer_down(&mut eng, &ev(cx, IN as f64 - TOL.rotate_offset), &TOL);
    e.pointer_move(&mut eng, &PointerEvent { shift: true, ..ev(cx + 1_000_000.0, cy + 30_000.0) }, &TOL);
    assert_eq!(xfrm(&e, id).rot, 90.0);
}

#[test]
fn double_click_edits_text_with_ime_and_undo() {
    let (mut e, id) = editor();
    let mut eng = engine();
    let p = ev(1_300_000.0, 1_300_000.0);
    e.pointer_down(&mut eng, &p, &TOL);
    e.pointer_up(&mut eng, &p);
    e.pointer_down(&mut eng, &PointerEvent { clicks: 2, ..p }, &TOL);
    assert!(e.is_editing_text());

    assert!(e.insert_text(&mut eng, "Hi "));
    e.compose_update(&mut eng, "ㄋㄧˇ", "ㄋㄧˇ".len());
    assert!(e.is_composing());
    assert!(!e.text_view().unwrap().compose.is_empty());
    e.compose_update(&mut eng, "你", 3);
    e.compose_end(&mut eng, "你好");
    assert!(!e.is_composing());
    e.key_down(&mut eng, "Enter", Modifiers::default());
    e.insert_text(&mut eng, "x");
    // ⌘Z while editing undoes typing inside the box first.
    e.key_down(&mut eng, "z", Modifiers { meta: true, ..Default::default() });
    assert!(e.is_editing_text());
    e.insert_text(&mut eng, "line2");
    assert!(e.key_down(&mut eng, "Escape", Modifiers::default()));
    assert!(!e.is_editing_text());

    let body = e.pres.shape(0, id).unwrap().text.clone().unwrap();
    assert_eq!(body.plain_text(), "Hi 你好\nline2");
    assert!(e.undo(&mut eng));
    assert!(e.pres.shape(0, id).unwrap().text.as_ref().unwrap().is_empty());
}

#[test]
fn formatting_text_selection_and_whole_shape() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    e.insert_text(&mut eng, "hello world");
    e.key_down(&mut eng, "ArrowLeft", Modifiers { alt: true, shift: true, ..Default::default() });
    e.key_down(&mut eng, "b", Modifiers { meta: true, ..Default::default() });
    e.end_text_edit(&mut eng);
    let runs = &e.pres.shape(0, id).unwrap().text.as_ref().unwrap().paragraphs[0].runs;
    assert_eq!(runs.len(), 2);
    assert!(runs[1].style.bold && runs[1].text == "world");

    // Not editing: the toggle applies to all text; the first character is not bold, so all become bold.
    e.toggle_style(&mut eng, Toggle::Bold);
    let runs = &e.pres.shape(0, id).unwrap().text.as_ref().unwrap().paragraphs[0].runs;
    assert_eq!(runs.len(), 1);
    assert!(runs[0].style.bold);
    e.key_down(&mut eng, "]", Modifiers { meta: true, ..Default::default() });
    assert_eq!(e.text_format().unwrap().style.size_pt, 20.0);
    e.apply_paragraph(&mut eng, |p| p.align = Align::Right);
    assert_eq!(e.text_format().unwrap().align, Align::Right);
    e.toggle_bullets(&mut eng);
    assert!(e.text_format().unwrap().bullet);
}

#[test]
fn shape_fill_and_outline() {
    let (mut e, id) = editor();
    e.selection = vec![id];
    e.set_fill(Some(Color::rgb(1, 2, 3)));
    e.set_line_color(Some(Color::rgb(9, 9, 9)));
    e.set_line_width(25_400);
    let s = e.pres.shape(0, id).unwrap();
    assert_eq!(s.fill, Some(Paint::Solid(Color::rgb(1, 2, 3))));
    assert_eq!(s.line, Some(Line::new(Color::rgb(9, 9, 9), 25_400)));
    e.set_line_color(None);
    assert_eq!(e.pres.shape(0, id).unwrap().line, None);
}

#[test]
fn typing_on_a_selected_shape_replaces_its_text() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.pres.shape_mut(0, id).unwrap().text = Some({
        let mut b = TextBody::default();
        b.set_plain_text("old");
        b
    });
    e.selection = vec![id];
    e.insert_text(&mut eng, "n");
    e.insert_text(&mut eng, "ew");
    e.end_text_edit(&mut eng);
    assert_eq!(e.pres.shape(0, id).unwrap().text.as_ref().unwrap().plain_text(), "new");
}

#[test]
fn click_in_edited_text_places_caret() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.pres.shape_mut(0, id).unwrap().text = Some({
        let mut b = TextBody { insets: [0; 4], ..Default::default() };
        b.paragraphs[0].end_style.latin_font = "Helvetica".into();
        b.set_plain_text("abcdef");
        b
    });
    e.selection = vec![id];
    e.key_down(&mut eng, "Enter", Modifiers::default());
    let p = ev(IN as f64 + 1_000.0, IN as f64 + 40_000.0);
    e.pointer_down(&mut eng, &p, &TOL);
    e.pointer_up(&mut eng, &p);
    e.insert_text(&mut eng, "X");
    e.end_text_edit(&mut eng);
    assert_eq!(e.pres.shape(0, id).unwrap().text.as_ref().unwrap().plain_text(), "Xabcdef");
}

#[test]
fn new_text_box_grows_with_its_text_and_empty_one_is_removed() {
    let (mut e, _) = editor();
    let mut eng = engine();
    e.add_text_box(&mut eng);
    let id = e.selection[0];
    let one_line = xfrm(&e, id).h;
    e.insert_text(&mut eng, "第一行");
    e.key_down(&mut eng, "Enter", Modifiers::default());
    e.insert_text(&mut eng, "第二行");
    assert!(xfrm(&e, id).h > one_line + 200_000);
    for _ in 0..4 {
        e.key_down(&mut eng, "Backspace", Modifiers::default());
    }
    assert_eq!(xfrm(&e, id).h, one_line);

    e.add_text_box(&mut eng);
    assert_eq!(e.pres.slides[0].shapes.len(), 3);
    e.key_down(&mut eng, "Escape", Modifiers::default());
    assert_eq!(e.pres.slides[0].shapes.len(), 2);
}

#[test]
fn copy_paste_duplicate_and_delete() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    assert_eq!(e.copy(), 1);
    e.paste(&mut eng);
    assert_eq!(e.pres.slides[0].shapes.len(), 2);
    let pasted = e.selection[0];
    assert_ne!(pasted, id);
    assert_eq!(xfrm(&e, pasted).x, IN + PASTE_OFFSET);
    e.key_down(&mut eng, "d", Modifiers { meta: true, ..Default::default() });
    assert_eq!(e.pres.slides[0].shapes.len(), 3);
    e.key_down(&mut eng, "a", Modifiers { meta: true, ..Default::default() });
    e.key_down(&mut eng, "Backspace", Modifiers::default());
    assert!(e.pres.slides[0].shapes.is_empty());
    e.undo(&mut eng);
    assert_eq!(e.pres.slides[0].shapes.len(), 3);
}

#[test]
fn z_order_and_alignment() {
    let (mut e, a) = editor();
    let b = add_rect(&mut e, 4 * IN, 2 * IN);
    e.selection = vec![a];
    assert!(e.reorder(ZOrder::Front));
    assert_eq!(e.pres.slides[0].shapes.last().unwrap().id, a);
    assert!(!e.reorder(ZOrder::Forward), "already in front");
    e.reorder(ZOrder::Backward);
    assert_eq!(e.pres.slides[0].shapes[0].id, a);

    e.selection = vec![a, b];
    e.align(AlignTo::Top);
    assert_eq!(xfrm(&e, a).y, xfrm(&e, b).y);
    e.selection = vec![b];
    e.align(AlignTo::Center);
    assert_eq!(xfrm(&e, b).x + IN / 2, e.pres.slide_width / 2);
}

#[test]
fn slide_add_duplicate_move_delete() {
    let (mut e, _) = editor();
    let mut eng = engine();
    let mut title = Shape::new(ShapeId(0), "Title", Xfrm::new(0, 0, IN, IN));
    title.placeholder = Some(Placeholder { kind: "title".into(), idx: 0 });
    title.text = Some(TextBody::default());
    std::sync::Arc::make_mut(&mut e.pres.layouts).push(Layout { name: "Title".into(), placeholders: vec![title], ..Default::default() });

    e.add_slide(&mut eng, Some(0));
    assert_eq!(e.pres.slides.len(), 2);
    assert_eq!(e.slide, 1);
    assert_eq!(e.pres.slides[1].shapes[0].placeholder.as_ref().unwrap().kind, "title");
    e.duplicate_slide(&mut eng, 1);
    assert_eq!(e.pres.slides.len(), 3);
    assert_ne!(e.pres.slides[1].shapes[0].id, e.pres.slides[2].shapes[0].id);
    e.move_slide(&mut eng, 0, 2);
    assert_eq!(e.pres.slides[2].shapes[0].name, "rect");
    e.delete_slide(&mut eng, 2);
    assert_eq!(e.pres.slides.len(), 2);
    e.undo(&mut eng);
    assert_eq!(e.pres.slides.len(), 3);
}

#[test]
fn delete_and_nudge() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    e.key_down(&mut eng, "ArrowRight", Modifiers { shift: true, ..Default::default() });
    assert_eq!(xfrm(&e, id).x, IN + 91_440);
    e.key_down(&mut eng, "Backspace", Modifiers::default());
    assert!(e.pres.slides[0].shapes.is_empty());
    e.key_down(&mut eng, "z", Modifiers { meta: true, ..Default::default() });
    assert_eq!(e.pres.slides[0].shapes.len(), 1);
}

#[test]
fn revision_tracks_document_changes() {
    let (mut e, id) = editor();
    let r0 = e.revision();
    e.selection = vec![id];
    e.nudge(10, 0);
    assert!(e.revision() > r0);
}

#[test]
fn line_hit_test_uses_distance_to_segment() {
    let (mut e, _) = editor();
    let id = e.pres.alloc_id();
    let mut line = Shape::new(id, "line", Xfrm::new(3_000_000, 3_000_000, 1_000_000, 1_000_000));
    line.geometry = Geometry::preset("line");
    e.pres.slide_mut(0).unwrap().shapes.push(line);
    assert_eq!(e.hit_test(3_500_000.0, 3_500_000.0, 10_000.0), Some(id));
    assert_eq!(e.hit_test(3_900_000.0, 3_100_000.0, 10_000.0), None);
}

#[test]
fn utf16_offsets() {
    assert_eq!(utf16_to_byte_offset("你好a", 1), 3);
    assert_eq!(utf16_to_byte_offset("你好a", 3), 7);
    assert_eq!(utf16_to_byte_offset("😀a", 2), 4);
}

fn table_editor(eng: &mut TextEngine, rows: usize, cols: usize) -> (Editor, ShapeId) {
    let mut pres = Presentation::default();
    pres.slides.push(std::sync::Arc::new(Slide::default()));
    let mut e = Editor::new(pres);
    assert!(e.insert_table(eng, rows, cols));
    e.end_text_edit(eng);
    let id = e.pres.slides[0].shapes[0].id;
    (e, id)
}

fn table(e: &Editor, id: ShapeId) -> &ppt_model::Table {
    match &e.pres.shape(0, id).unwrap().kind {
        ShapeKind::Table(t) => t,
        _ => panic!("not a table"),
    }
}

fn cell_text(e: &Editor, id: ShapeId, r: usize, c: usize) -> String {
    table(e, id).rows[r].cells[c].text.plain_text()
}

/// Center of cell (r, c) of an unrotated table, from the editor's own frame.
fn cell_center(e: &Editor, id: ShapeId, r: usize, c: usize) -> (f64, f64) {
    let shape = e.pres.shape(0, id).unwrap();
    let t = table(e, id);
    let heights = e.table_heights[&id].clone();
    let f = t.cell_frame(&heights, &shape.xfrm, r, c);
    ((f.x + f.w / 2) as f64, (f.y + f.h / 2) as f64)
}

#[test]
fn insert_table_edits_first_cell_and_tab_moves_on() {
    let mut eng = engine();
    let mut pres = Presentation::default();
    pres.slides.push(std::sync::Arc::new(Slide::default()));
    let mut e = Editor::new(pres);
    e.insert_table(&mut eng, 2, 2);
    let id = e.pres.slides[0].shapes[0].id;
    assert_eq!(e.text.as_ref().map(|t| t.cell), Some(Some((0, 0))));
    e.insert_text(&mut eng, "標題");
    e.key_down(&mut eng, "Tab", Modifiers::default());
    assert_eq!(e.text.as_ref().unwrap().cell, Some((0, 1)));
    e.insert_text(&mut eng, "B");
    e.key_down(&mut eng, "Tab", Modifiers { shift: true, ..Default::default() });
    assert_eq!(e.text.as_ref().unwrap().cell, Some((0, 0)));
    // Tab selects the cell's text, so typing replaces it.
    e.insert_text(&mut eng, "A");
    e.key_down(&mut eng, "Escape", Modifiers::default());
    assert_eq!((cell_text(&e, id, 0, 0), cell_text(&e, id, 0, 1)), ("A".to_string(), "B".to_string()));
}

#[test]
fn tab_in_last_cell_adds_a_row() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 2);
    let h0 = xfrm(&e, id).h;
    let (x, y) = cell_center(&e, id, 1, 1);
    e.pointer_down(&mut eng, &ev(x, y), &TOL);
    e.pointer_up(&mut eng, &ev(x, y));
    assert_eq!(e.text.as_ref().unwrap().cell, Some((1, 1)));
    e.key_down(&mut eng, "Tab", Modifiers::default());
    assert_eq!(table(&e, id).rows.len(), 3);
    assert_eq!(e.text.as_ref().unwrap().cell, Some((2, 0)));
    assert!(xfrm(&e, id).h > h0, "the frame follows the new row");
    e.undo(&mut eng);
    assert_eq!(table(&e, id).rows.len(), 2);
}

#[test]
fn rows_grow_while_typing_and_undo_restores_the_cell() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 2);
    let h0 = xfrm(&e, id).h;
    let (x, y) = cell_center(&e, id, 0, 0);
    e.pointer_down(&mut eng, &ev(x, y), &TOL);
    e.pointer_up(&mut eng, &ev(x, y));
    e.insert_text(&mut eng, "一\n二\n三\n四");
    assert!(xfrm(&e, id).h > h0);
    e.end_text_edit(&mut eng);
    assert_eq!(cell_text(&e, id, 0, 0), "一\n二\n三\n四");
    e.undo(&mut eng);
    assert_eq!(cell_text(&e, id, 0, 0), "");
    assert_eq!(xfrm(&e, id).h, h0);
}

#[test]
fn dragging_a_column_line_trades_width() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 3);
    let cols = table(&e, id).cols.clone();
    let shape = xfrm(&e, id);
    let line_x = (shape.x + cols[0]) as f64;
    let mid_y = (shape.y + shape.h / 2) as f64;
    drag(&mut e, &mut eng, (line_x, mid_y), (line_x + 200_000.0, mid_y), false);
    let after = &table(&e, id).cols;
    assert_eq!(after[0], cols[0] + 200_000);
    assert_eq!(after[1], cols[1] - 200_000);
    assert_eq!(after.iter().sum::<i64>(), cols.iter().sum::<i64>());
    assert!(e.text.is_none(), "a grid line drag is not a click into a cell");
}

#[test]
fn insert_and_delete_rows_and_columns() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 2);
    let w0 = xfrm(&e, id).w;
    let (x, y) = cell_center(&e, id, 0, 0);
    e.pointer_down(&mut eng, &ev(x, y), &TOL);
    e.pointer_up(&mut eng, &ev(x, y));
    e.insert_text(&mut eng, "左上");
    assert!(e.insert_table_col(&mut eng, true));
    assert_eq!(table(&e, id).cols.len(), 3);
    assert!(xfrm(&e, id).w > w0);
    assert_eq!(e.text.as_ref().unwrap().cell, Some((0, 1)));
    assert!(e.insert_table_row(&mut eng, false));
    assert_eq!(table(&e, id).rows.len(), 3);
    assert_eq!(cell_text(&e, id, 1, 0), "左上");
    assert!(e.delete_table_row(&mut eng));
    assert!(e.delete_table_col(&mut eng));
    assert_eq!((table(&e, id).rows.len(), table(&e, id).cols.len()), (2, 2));
    assert_eq!(cell_text(&e, id, 0, 0), "左上");
}

#[test]
fn merged_cells_follow_row_and_column_edits() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 3, 3);
    // Merge (0,0)-(1,0) vertically and (2,0)-(2,1) horizontally.
    if let ShapeKind::Table(t) = &mut e.pres.slide_mut(0).unwrap().shapes[0].kind {
        t.rows[0].cells[0].row_span = 2;
        t.rows[1].cells[0].v_merge = true;
        t.rows[2].cells[0].grid_span = 2;
        t.rows[2].cells[1].h_merge = true;
    }
    e.refresh_tables(&mut eng);
    // A click on the covered cell edits the merged one.
    let (x, y) = cell_center(&e, id, 0, 0);
    e.pointer_down(&mut eng, &ev(x, y), &TOL);
    e.pointer_up(&mut eng, &ev(x, y));
    assert_eq!(e.text.as_ref().unwrap().cell, Some((0, 0)));
    // A row inserted inside the vertical merge extends it.
    e.end_text_edit(&mut eng);
    e.begin_cell_edit(&mut eng, id, 0, 1);
    e.insert_table_row(&mut eng, true);
    let t = table(&e, id);
    assert_eq!(t.rows[0].cells[0].row_span, 3);
    assert!(t.rows[1].cells[0].v_merge);
    // Deleting the first column (from a one-column cell) moves the horizontal merge's text along.
    e.end_text_edit(&mut eng);
    e.begin_cell_edit(&mut eng, id, 3, 0);
    e.insert_text(&mut eng, "合併");
    e.end_text_edit(&mut eng);
    e.begin_cell_edit(&mut eng, id, 0, 0);
    e.delete_table_col(&mut eng);
    let t = table(&e, id);
    assert_eq!(t.cols.len(), 2);
    assert_eq!(t.rows[3].cells[0].grid_span, 1);
    assert_eq!(t.rows[3].cells[0].text.plain_text(), "合併");
    assert!(t.visible_cells().iter().all(|&(r, c)| !t.rows[r].cells[c].is_covered()));
}

/// Three 1in squares at x = 1in, 3in, 5in; the first two grouped.
fn group_editor() -> (Editor, [ShapeId; 3], ShapeId) {
    let mut pres = Presentation::default();
    let mut ids = [ShapeId(0); 3];
    let mut shapes = Vec::new();
    for (i, x) in [IN, 3 * IN, 5 * IN].into_iter().enumerate() {
        let id = pres.alloc_id();
        ids[i] = id;
        let mut s = Shape::new(id, "sq", Xfrm::new(x, IN, IN, IN));
        s.fill = Some(Paint::Solid(Color::BLACK));
        shapes.push(s);
    }
    let g = pres.alloc_id();
    shapes[0].group = Some(g);
    shapes[1].group = Some(g);
    pres.slides.push(std::sync::Arc::new(Slide { shapes, ..Default::default() }));
    (Editor::new(pres), ids, g)
}

fn click(e: &mut Editor, eng: &mut TextEngine, x: f64, y: f64, clicks: u32) {
    e.pointer_down(eng, &PointerEvent { clicks, ..ev(x, y) }, &TOL);
    e.pointer_up(eng, &ev(x, y));
}

#[test]
fn clicking_a_member_selects_and_moves_the_whole_group() {
    let (mut e, [a, b, c], g) = group_editor();
    let mut eng = engine();
    click(&mut e, &mut eng, 1.5 * IN as f64, 1.5 * IN as f64, 1);
    assert_eq!(e.selection, vec![a, b]);
    assert_eq!(e.selection_group(), Some(g));
    let frame = e.selection_frame().unwrap();
    assert_eq!((frame.x, frame.w), (IN, 3 * IN));
    drag(&mut e, &mut eng, (1.5 * IN as f64, 1.5 * IN as f64), (1.5 * IN as f64 + 100_000.0, 1.5 * IN as f64), false);
    assert_eq!((xfrm(&e, a).x, xfrm(&e, b).x, xfrm(&e, c).x), (IN + 100_000, 3 * IN + 100_000, 5 * IN));
    e.undo(&mut eng);
    assert_eq!(xfrm(&e, b).x, 3 * IN);
}

#[test]
fn second_click_enters_the_group_and_double_click_edits_text() {
    let (mut e, [a, b, _], g) = group_editor();
    let mut eng = engine();
    let (bx, by) = (3.5 * IN as f64, 1.5 * IN as f64);
    click(&mut e, &mut eng, bx, by, 1);
    click(&mut e, &mut eng, bx, by, 1);
    assert_eq!((e.selection.clone(), e.entered_group), (vec![b], Some(g)));
    // Inside the group, dragging moves just the member.
    drag(&mut e, &mut eng, (bx, by), (bx + 100_000.0, by), false);
    assert_eq!((xfrm(&e, a).x, xfrm(&e, b).x), (IN, 3 * IN + 100_000));
    // Clicking elsewhere leaves the group.
    click(&mut e, &mut eng, 9.0 * IN as f64, 5.0 * IN as f64, 1);
    assert_eq!(e.entered_group, None);
    // A double click on a member edits its text directly.
    click(&mut e, &mut eng, 1.5 * IN as f64, 1.5 * IN as f64, 1);
    e.pointer_down(&mut eng, &PointerEvent { clicks: 2, ..ev(1.5 * IN as f64, 1.5 * IN as f64) }, &TOL);
    e.pointer_up(&mut eng, &ev(1.5 * IN as f64, 1.5 * IN as f64));
    assert_eq!(e.text.as_ref().map(|t| t.shape), Some(a));
}

#[test]
fn resizing_a_group_scales_its_members() {
    let (mut e, [a, b, _], _) = group_editor();
    let mut eng = engine();
    click(&mut e, &mut eng, 1.5 * IN as f64, 1.5 * IN as f64, 1);
    // Drag the east handle of the 3in-wide group 3in to the right: everything doubles in x.
    let east = e.handles(&TOL).into_iter().find(|h| h.0 == Handle::E).unwrap().1;
    assert!(e.handles(&TOL).iter().all(|h| h.0 != Handle::Rotate));
    drag(&mut e, &mut eng, east, (east.0 + 3.0 * IN as f64, east.1), false);
    assert_eq!((xfrm(&e, a).x, xfrm(&e, a).w), (IN, 2 * IN));
    assert_eq!((xfrm(&e, b).x, xfrm(&e, b).w), (5 * IN, 2 * IN));
}

#[test]
fn group_and_ungroup_with_undo() {
    let (mut e, [a, b, c], g) = group_editor();
    let mut eng = engine();
    e.selection = vec![a, b];
    assert!(e.key_down(&mut eng, "g", Modifiers { meta: true, alt: true, shift: true, ..Default::default() }));
    assert!(e.pres.slides[0].shapes.iter().all(|s| s.group.is_none()));
    assert_eq!(e.selection, vec![a, b]);
    // Group the third shape with the first: members become contiguous at the top member's depth.
    e.selection = vec![a, c];
    assert!(e.key_down(&mut eng, "g", Modifiers { meta: true, alt: true, ..Default::default() }));
    let order: Vec<ShapeId> = e.pres.slides[0].shapes.iter().map(|s| s.id).collect();
    assert_eq!(order, vec![b, a, c]);
    let g2 = e.pres.shape(0, a).unwrap().group;
    assert!(g2.is_some() && g2 != Some(g) && e.pres.shape(0, c).unwrap().group == g2);
    e.undo(&mut eng);
    e.undo(&mut eng);
    assert_eq!(e.pres.shape(0, b).unwrap().group, Some(g));
}

#[test]
fn copies_get_their_own_groups() {
    let (mut e, [a, b, _], g) = group_editor();
    let mut eng = engine();
    click(&mut e, &mut eng, 1.5 * IN as f64, 1.5 * IN as f64, 1);
    e.copy();
    e.paste(&mut eng);
    let pasted: Vec<&Shape> = e.pres.slides[0].shapes.iter().skip(3).collect();
    assert_eq!(pasted.len(), 2);
    assert!(pasted[0].group.is_some() && pasted[0].group == pasted[1].group && pasted[0].group != Some(g));
    // One member copied from inside the group comes out ungrouped.
    e.selection = vec![a];
    e.entered_group = Some(g);
    e.duplicate_selected();
    assert_eq!(e.pres.slides[0].shapes.last().unwrap().group, None);
    let _ = b;
}

#[test]
fn marquee_takes_a_group_only_when_all_of_it_is_inside() {
    let (mut e, [a, b, c], _) = group_editor();
    let mut eng = engine();
    // Covers the second and third squares, not the first.
    drag(&mut e, &mut eng, (2.5 * IN as f64, 0.5 * IN as f64), (6.5 * IN as f64, 2.5 * IN as f64), false);
    assert_eq!(e.selection, vec![c]);
    drag(&mut e, &mut eng, (0.5 * IN as f64, 0.5 * IN as f64), (6.5 * IN as f64, 2.5 * IN as f64), false);
    assert_eq!(e.selection, vec![a, b, c]);
}

#[test]
fn numbering_toggles_and_switches_scheme() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    e.insert_text(&mut eng, "一\n二");
    e.key_down(&mut eng, "a", Modifiers { meta: true, ..Default::default() });
    e.toggle_numbering(&mut eng, "arabicPeriod");
    assert_eq!(e.text_format().unwrap().numbering.as_deref(), Some("arabicPeriod"));
    assert!(!e.text_format().unwrap().bullet);
    e.toggle_numbering(&mut eng, "romanUcPeriod");
    assert_eq!(e.text_format().unwrap().numbering.as_deref(), Some("romanUcPeriod"));
    e.toggle_numbering(&mut eng, "romanUcPeriod");
    assert_eq!(e.text_format().unwrap().numbering, None);
    // Bullets replace numbering.
    e.toggle_numbering(&mut eng, "arabicPeriod");
    e.toggle_bullets(&mut eng);
    assert!(e.text_format().unwrap().bullet);
    assert_eq!(e.text_format().unwrap().numbering, None);
}

fn styled_table_editor(eng: &mut TextEngine, rows: usize, cols: usize) -> (Editor, ShapeId) {
    use ppt_model::table::{CellFill, StylePart, TableStyle};
    let mut pres = Presentation::default();
    pres.slides.push(std::sync::Arc::new(Slide::default()));
    let style = |id: &str, header: Color| {
        std::sync::Arc::new(TableStyle {
            id: id.into(),
            name: id.into(),
            first_row: StylePart { fill: Some(CellFill::Solid(header)), color: Some(Color::WHITE), bold: Some(true), ..Default::default() },
            ..Default::default()
        })
    };
    pres.table_styles = vec![style("blue", Color::rgb(0, 0, 200)), style("plain", Color::rgb(250, 250, 250))];
    pres.table_style = Some(pres.table_styles[0].clone());
    pres.table_styles[1] = std::sync::Arc::new(TableStyle { first_row: StylePart::default(), ..(*pres.table_styles[1]).clone() });
    let mut e = Editor::new(pres);
    e.insert_table(eng, rows, cols);
    e.end_text_edit(eng);
    let id = e.pres.slides[0].shapes[0].id;
    (e, id)
}

#[test]
fn dragging_across_cells_selects_a_block_and_merges_it() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 3, 3);
    for (r, c, s) in [(0, 0, "甲"), (0, 1, "乙"), (1, 1, "丙")] {
        e.begin_cell_edit(&mut eng, id, r, c);
        e.insert_text(&mut eng, s);
        e.end_text_edit(&mut eng);
    }
    let (a, b) = (cell_center(&e, id, 0, 0), cell_center(&e, id, 1, 1));
    e.pointer_down(&mut eng, &ev(a.0, a.1), &TOL);
    e.pointer_move(&mut eng, &PointerEvent { alt: true, ..ev(b.0, b.1) }, &TOL);
    e.pointer_up(&mut eng, &ev(b.0, b.1));
    assert!(e.text.is_none());
    assert_eq!(e.cell_range_frames().len(), 4);
    assert!(e.can_merge_cells());
    assert!(e.merge_cells(&mut eng));
    let t = table(&e, id);
    assert_eq!((t.rows[0].cells[0].grid_span, t.rows[0].cells[0].row_span), (2, 2));
    assert!(t.rows[0].cells[1].h_merge && t.rows[1].cells[0].v_merge && t.rows[1].cells[1].h_merge && t.rows[1].cells[1].v_merge);
    assert_eq!(cell_text(&e, id, 0, 0), "甲\n乙\n丙");
    assert_eq!(e.text.as_ref().unwrap().cell, Some((0, 0)));
    e.undo(&mut eng);
    assert_eq!(table(&e, id).rows[0].cells[0].grid_span, 1);
}

#[test]
fn shift_click_extends_over_merged_cells() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 3, 3);
    if let ShapeKind::Table(t) = &mut e.pres.slide_mut(0).unwrap().shapes[0].kind {
        t.rows[1].cells[1].grid_span = 2;
        t.rows[1].cells[2].h_merge = true;
    }
    e.refresh_tables(&mut eng);
    e.begin_cell_edit(&mut eng, id, 0, 0);
    let (x, y) = cell_center(&e, id, 1, 1);
    e.pointer_down(&mut eng, &PointerEvent { shift: true, ..ev(x, y) }, &TOL);
    e.pointer_up(&mut eng, &ev(x, y));
    // (0,0)..(1,1) grows to take the whole merged cell (1,1)-(1,2).
    assert_eq!(e.cell_range_frames().len(), 5);
}

#[test]
fn splitting_cells_adds_grid_lines_and_undoes_merges() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 2);
    let w = table(&e, id).cols.clone();
    e.begin_cell_edit(&mut eng, id, 0, 0);
    e.insert_text(&mut eng, "原文");
    assert!(e.split_cell(&mut eng, 1, 2));
    let t = table(&e, id);
    assert_eq!(t.cols.len(), 3);
    assert_eq!(t.cols[0] + t.cols[1], w[0]);
    // The other row spans the new grid line.
    assert_eq!(t.rows[1].cells[0].grid_span, 2);
    assert!(t.rows[1].cells[1].h_merge);
    assert_eq!(cell_text(&e, id, 0, 0), "原文");
    // Merge the two halves back, then split the merged cell into rows instead.
    e.end_text_edit(&mut eng);
    e.cell_range = Some(CellRange { shape: id, anchor: (0, 0), focus: (0, 1) });
    e.merge_cells(&mut eng);
    assert!(e.split_cell(&mut eng, 2, 1));
    let t = table(&e, id);
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.rows[0].cells[0].grid_span, 2);
    assert_eq!(t.rows[1].cells[0].grid_span, 2);
    assert!(t.visible_cells().iter().all(|&(r, c)| !t.rows[r].cells[c].is_covered()));
}

#[test]
fn cell_fill_borders_and_alignment() {
    use ppt_model::table::{BOTTOM, CellFill, LEFT, RIGHT, TOP};
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 3, 3);
    e.cell_range = Some(CellRange { shape: id, anchor: (0, 0), focus: (1, 1) });
    e.selection = vec![id];
    assert!(e.set_cell_fill(&mut eng, Some(Color::rgb(1, 2, 3))));
    let red = Line::new(Color::rgb(255, 0, 0), 25_400);
    assert!(e.set_cell_borders(&mut eng, BorderSet::Outside, red));
    let t = table(&e, id);
    assert_eq!(t.rows[1].cells[1].fill, CellFill::Solid(Color::rgb(1, 2, 3)));
    assert_eq!(t.rows[2].cells[2].fill, CellFill::Inherit);
    assert_eq!(t.rows[0].cells[0].borders[TOP], Some(Some(red)));
    assert_eq!(t.rows[0].cells[0].borders[RIGHT], None, "an inside edge is untouched");
    // The cell to the right of the block gets the facing edge too.
    assert_eq!(t.rows[1].cells[2].borders[LEFT], Some(Some(red)));
    assert_eq!(t.rows[2].cells[0].borders[TOP], Some(Some(red)));
    assert!(e.set_cell_borders(&mut eng, BorderSet::None, red));
    assert_eq!(table(&e, id).rows[1].cells[1].borders[BOTTOM], Some(None));
    assert!(e.set_cell_anchor(&mut eng, Anchor::Middle));
    assert_eq!(table(&e, id).rows[0].cells[1].text.anchor, Anchor::Middle);
}

#[test]
fn switching_style_and_options_restyles_header_text() {
    let mut eng = engine();
    let (mut e, id) = styled_table_editor(&mut eng, 2, 2);
    e.begin_cell_edit(&mut eng, id, 0, 0);
    e.insert_text(&mut eng, "標題");
    e.end_text_edit(&mut eng);
    let header = |e: &Editor| table(e, id).rows[0].cells[0].text.paragraphs[0].runs[0].style.clone();
    assert!(header(&e).bold && header(&e).color == Color::WHITE);
    e.selection = vec![id];
    assert!(e.set_table_style(&mut eng, "plain"));
    assert_eq!(table(&e, id).style.as_ref().unwrap().id, "plain");
    assert!(!header(&e).bold && header(&e).color != Color::WHITE, "white header text would vanish");
    assert!(e.set_table_style(&mut eng, "blue"));
    assert!(e.set_table_flag(&mut eng, "firstRow", false));
    assert!(!table(&e, id).flags.first_row);
    assert!(!header(&e).bold);
}

#[test]
fn bold_applies_to_every_selected_cell() {
    let mut eng = engine();
    let (mut e, id) = table_editor(&mut eng, 2, 2);
    for (r, c) in [(0, 0), (1, 1)] {
        e.begin_cell_edit(&mut eng, id, r, c);
        e.insert_text(&mut eng, "字");
        e.end_text_edit(&mut eng);
    }
    e.cell_range = Some(CellRange { shape: id, anchor: (0, 0), focus: (1, 1) });
    e.selection = vec![id];
    assert!(e.toggle_style(&mut eng, Toggle::Bold));
    let bold = |r: usize, c: usize| table(&e, id).rows[r].cells[c].text.style_at(0).bold;
    assert!(bold(0, 0) && bold(1, 1));
    assert!(e.key_down(&mut eng, "Delete", Modifiers::default()));
    assert_eq!(cell_text(&e, id, 1, 1), "");
}

#[test]
fn notes_typing_is_one_undo_step_per_session() {
    let mut eng = engine();
    let (mut e, _) = editor();
    e.begin_notes_edit(&mut eng);
    assert!(e.set_notes("a"));
    assert!(e.set_notes("ab"));
    assert!(!e.set_notes("ab"));
    e.begin_notes_edit(&mut eng);
    assert!(e.set_notes("abc"));
    assert_eq!(e.notes(0), "abc");
    assert!(e.undo(&mut eng));
    assert_eq!(e.notes(0), "ab");
    assert!(e.undo(&mut eng));
    assert_eq!(e.notes(0), "");
    assert!(e.redo(&mut eng));
    assert_eq!(e.notes(0), "ab");
}

#[test]
fn transitions_apply_to_one_or_all_slides_with_undo() {
    use ppt_model::transition::{Dir, Transition, TransitionKind};
    let mut eng = engine();
    let (mut e, _) = editor();
    let blank = e.pres.slide_from_layout(0);
    e.pres.slides.push(std::sync::Arc::new(blank));
    let push = Transition { kind: TransitionKind::Push(Dir::Up), ..Default::default() };
    assert!(e.set_transition(&mut eng, Some(push.clone()), false));
    assert!(!e.set_transition(&mut eng, Some(push.clone()), false));
    assert_eq!((e.pres.slides[0].transition.as_ref(), e.pres.slides[1].transition.as_ref()), (Some(&push), None));
    assert!(e.set_transition(&mut eng, Some(push.clone()), true));
    assert_eq!(e.pres.slides[1].transition.as_ref(), Some(&push));
    assert!(e.undo(&mut eng));
    assert_eq!(e.pres.slides[1].transition, None);
}

#[test]
fn animations_are_added_edited_and_follow_shapes() {
    use ppt_model::anim::{Player, Trigger, effect_duration, effect_elements};
    let mut eng = engine();
    let (mut e, a) = editor();
    let b = add_rect(&mut e, 3 * IN, IN);
    assert!(!e.add_animation("entr", 10, None), "nothing selected");
    e.selection = vec![a, b];
    assert!(e.add_animation("entr", 10, None));
    let list = e.animations();
    assert_eq!(list.iter().map(|x| (Trigger::of(x), effect_elements(x))).collect::<Vec<_>>(), vec![(Trigger::Click, vec![a]), (Trigger::With, vec![b])]);
    // A second effect on the same shape gets its own build group.
    e.selection = vec![a];
    assert!(e.add_animation("exit", 2, Some(8)));
    assert_eq!(e.animations()[2].grp_id, Some(1));
    assert_eq!(Player::new(e.pres.slides[0].timing.as_deref().unwrap()).clicks(), 2);

    assert!(e.set_animation_trigger(2, Trigger::After));
    assert!(e.set_animation_timing(2, Some(250.0), Some(1000.0)));
    assert_eq!((e.animations()[2].delay_ms, effect_duration(e.animations()[2])), (250.0, 1000.0));
    // Another direction keeps the duration; another effect takes its own.
    assert!(e.change_animation(2, "exit", 2, Some(1)));
    assert_eq!((e.animations()[2].preset_subtype, effect_duration(e.animations()[2])), (Some(1), 1000.0));
    assert!(e.change_animation(2, "exit", 10, None));
    assert_eq!(effect_duration(e.animations()[2]), 500.0);
    assert_eq!(Trigger::of(e.animations()[2]), Trigger::After);

    assert!(e.move_animation(2, 0));
    assert_eq!(e.animations()[0].preset_class, "exit");
    assert!(e.remove_animation(0));
    assert_eq!(e.animations().len(), 2);

    // Deleting a shape drops its effects; undo brings them back.
    e.selection = vec![b];
    assert!(e.delete_selected());
    assert_eq!(e.animations().len(), 1);
    assert!(e.undo(&mut eng));
    assert_eq!(e.animations().len(), 2);
    // Grouping drops the members' effects (only the group can be animated).
    e.selection = vec![a, b];
    assert!(e.group_selected());
    assert!(e.animations().is_empty() && e.pres.slides[0].timing.is_none());
    assert!(e.add_animation("entr", 1, None));
    let g = e.pres.shape(0, a).unwrap().group.unwrap();
    assert_eq!(effect_elements(e.animations()[0]), vec![g]);

    // A duplicated slide animates its own shapes.
    assert!(e.duplicate_slide(&mut eng, 0));
    let copy = &e.pres.slides[1];
    let el = effect_elements(copy.timing.as_deref().unwrap().effects()[0])[0];
    assert!(el != g && copy.shapes.iter().any(|s| s.group == Some(el)));
}

#[test]
fn find_and_replace_across_slides_and_notes() {
    let mut eng = engine();
    let (mut e, a) = editor();
    let set_text = |e: &mut Editor, slide: usize, id: ShapeId, text: &str| {
        let shape = e.pres.slide_mut(slide).unwrap().shapes.iter_mut().find(|s| s.id == id).unwrap();
        let mut body = TextBody::default();
        body.set_plain_text(text);
        shape.text = Some(body);
    };
    set_text(&mut e, 0, a, "營收 Revenue\nrevenue growth");
    let mut second = Slide::default();
    let b = e.pres.alloc_id();
    second.shapes.push(Shape::new(b, "b", Xfrm::new(0, 0, IN, IN)));
    second.notes = "talk about REVENUE".into();
    e.pres.slides.push(std::sync::Arc::new(second));
    set_text(&mut e, 1, b, "no match");
    let o = FindOptions::default();

    assert_eq!(e.find_all("revenue", o).len(), 3);
    assert_eq!(e.find_all("revenue", FindOptions { match_case: true, ..o }).len(), 1);
    // Next from nothing selected: the first match, selected in a text edit.
    let (i, all) = e.find_next("revenue", o, false, None).unwrap();
    assert_eq!(i, 0);
    assert!(e.select_match(&mut eng, &all[i]));
    assert_eq!(e.text.as_ref().unwrap().rich.selected_text().as_deref(), Some("Revenue"));
    let (i, all) = e.find_next("revenue", o, false, None).unwrap();
    assert_eq!(i, 1);
    e.select_match(&mut eng, &all[i]);
    let (i, all) = e.find_next("revenue", o, false, None).unwrap();
    assert_eq!((i, all[i].slide, all[i].place), (2, 1, FindPlace::Notes));
    // From the notes match, going on wraps to the first; backwards from the first goes to the last.
    e.select_match(&mut eng, &all[2]);
    assert_eq!(e.slide, 1);
    assert_eq!(e.find_next("revenue", o, false, Some(all[2].range.clone())).map(|r| r.0), Some(0));
    e.select_match(&mut eng, &all[0]);
    assert_eq!(e.find_next("revenue", o, true, None).map(|r| r.0), Some(2));

    // Replace the selected match through the editor, then all the rest at once.
    let m = all[0].clone();
    assert!(e.replace_match(&mut eng, &m, "revenue", "營業額", o));
    e.end_text_edit(&mut eng);
    assert_eq!(e.pres.slides[0].shapes[0].text.as_ref().unwrap().plain_text(), "營收 營業額\nrevenue growth");
    assert_eq!(e.replace_all(&mut eng, "revenue", "sales", o), 2);
    assert_eq!(e.pres.slides[0].shapes[0].text.as_ref().unwrap().plain_text(), "營收 營業額\nsales growth");
    assert_eq!(e.pres.slides[1].notes, "talk about sales");
    assert!(e.undo(&mut eng));
    assert_eq!(e.pres.slides[1].notes, "talk about REVENUE");
    assert!(e.find_next("nothing", o, false, None).is_none());
}

#[test]
fn layouts_apply_and_reset_and_save() {
    let mut eng = engine();
    let pres = ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap();
    let mut e = Editor::new(pres);
    // The template's first slide is a title slide (title + subtitle).
    let name = |e: &Editor, i: usize| e.pres.layouts[i].name.clone();
    let content = (0..e.pres.layouts.len()).find(|&i| name(&e, i).contains("內容") || name(&e, i).contains("Content")).unwrap();
    let title_layout = e.pres.slides[0].layout.unwrap();
    let title = e.pres.slides[0].shapes.iter().position(|s| s.placeholder.as_ref().is_some_and(|p| p.kind.contains("itle") && p.kind != "subTitle")).unwrap();
    let id = e.pres.slides[0].shapes[title].id;
    {
        let body = e.pres.slide_mut(0).unwrap().shapes[title].text.as_mut().unwrap();
        body.set_plain_text("標題");
    }
    let before_xfrm = e.pres.shape(0, id).unwrap().xfrm;
    assert!(e.apply_layout(&mut eng, content, false));
    let shape = e.pres.shape(0, id).expect("the title carries over");
    assert_eq!(shape.text.as_ref().unwrap().plain_text(), "標題");
    let layout_title = e.pres.layouts[content].placeholders.iter().find(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "title")).unwrap().clone();
    assert_eq!(shape.xfrm, layout_title.xfrm);
    assert_ne!(shape.xfrm, before_xfrm);
    // Formatting the old layout gave it follows the new one.
    let size = |s: &Shape| s.text.as_ref().unwrap().styles_in(0..3)[0].size_pt;
    assert_eq!(size(shape), layout_title.text.as_ref().unwrap().defaults_for(0).unwrap().style.size_pt);
    assert_eq!(e.pres.slides[0].layout, Some(content));
    // The new layout's body placeholder was added; the empty subtitle went away.
    assert!(e.pres.slides[0].shapes.iter().all(|s| s.placeholder.as_ref().is_none_or(|p| p.kind != "subTitle")));

    // Overridden formatting survives a layout change but not a reset.
    e.pres.slide_mut(0).unwrap().shapes.iter_mut().find(|s| s.id == id).unwrap().text.as_mut().unwrap().apply_style(0..3, |s| s.bold = true);
    e.pres.slide_mut(0).unwrap().shapes.iter_mut().find(|s| s.id == id).unwrap().xfrm.x += 100_000;
    assert!(e.reset_slide(&mut eng));
    let shape = e.pres.shape(0, id).unwrap();
    assert!(!shape.text.as_ref().unwrap().styles_in(0..3)[0].bold);
    assert_eq!(shape.xfrm, layout_title.xfrm);

    // Saved, the slide points at the new layout; undo goes back.
    let re = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&e.pres).unwrap()).unwrap();
    assert_eq!(re.slides[0].layout, Some(content));
    assert_eq!(re.slides[0].shapes.iter().find(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == "標題")).map(|s| s.xfrm), Some(layout_title.xfrm));
    assert!(e.undo(&mut eng) && e.undo(&mut eng));
    assert_eq!(e.pres.slides[0].layout, Some(title_layout));
}

#[test]
fn a_placeholder_taken_over_by_another_type_saves_as_it() {
    let mut eng = engine();
    let deck = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&deck).unwrap());
    e.set_slide(&mut eng, 1);
    let title_slide = (0..e.pres.layouts.len()).find(|&i| e.pres.layouts[i].placeholders.iter().any(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "ctrTitle"))).unwrap();
    assert!(e.apply_layout(&mut eng, title_slide, false));
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let xml = zip_part(&saved, "ppt/slides/slide2.xml");
    assert!(xml.contains(r#"<p:ph type="subTitle" idx="1"/>"#) && xml.contains(r#"<p:ph type="ctrTitle"/>"#), "{xml}");
}

fn zip_part(bytes: &[u8], name: &str) -> String {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn shape_format_frame_rotation_flips_shape_and_shadow() {
    let (mut e, a) = editor();
    e.selection = vec![a];
    assert!(e.set_selection_frame(Xfrm { rot: 30.0, ..Xfrm::new(2 * IN, IN, 3 * IN, IN / 2) }));
    assert_eq!(xfrm(&e, a), Xfrm { rot: 30.0, ..Xfrm::new(2 * IN, IN, 3 * IN, IN / 2) });
    assert!(e.flip_selection(true));
    assert!(xfrm(&e, a).flip_h && (xfrm(&e, a).rot - 330.0).abs() < 1e-9);
    assert!(e.rotate_selection(90.0));
    assert!((xfrm(&e, a).rot - 60.0).abs() < 1e-9);

    // A group mirrors and turns about its own center.
    let b = add_rect(&mut e, 4 * IN, IN);
    e.pres.shape_mut(0, a).unwrap().xfrm = Xfrm::new(IN, IN, IN, IN);
    e.selection = vec![a, b];
    assert!(e.group_selected());
    assert!(e.flip_selection(true));
    assert_eq!((xfrm(&e, a).x, xfrm(&e, b).x), (4 * IN, IN));
    assert!(e.rotate_selection(90.0));
    let (ca, cb) = (xfrm(&e, a).center(), xfrm(&e, b).center());
    assert!((ca.0 - cb.0).abs() < 2.0 && (ca.1 - cb.1).abs() > IN as f64, "members now stack vertically: {ca:?} {cb:?}");
    // Resizing a group scales its members.
    let f = e.selection_frame().unwrap();
    assert!(e.set_selection_frame(Xfrm { h: f.h * 2, ..f }));
    assert_eq!(xfrm(&e, a).h, 2 * IN);

    e.selection = vec![a];
    e.entered_group = e.pres.shape(0, a).unwrap().group;
    assert!(e.change_shape("ellipse"));
    assert_eq!(e.pres.shape(0, a).unwrap().geometry, Geometry::preset("ellipse"));
    let shadow = ppt_model::Shadow { color: Color::BLACK, blur: 50_800, dist: 38_100, dir: 45.0 };
    assert!(e.set_shadow(Some(shadow)));
    assert!(!e.set_shadow(Some(shadow)));
    assert_eq!(e.selection_shadow(), Some(Some(shadow)));
}

#[test]
fn cropping_keeps_the_image_scale() {
    let mut eng = engine();
    let (mut e, _) = editor();
    assert!(e.add_picture(&mut eng, "img".into(), std::sync::Arc::new(Vec::new()), 400, 200));
    let id = e.selection[0];
    let before = xfrm(&e, id);
    // Trimming a quarter from the left and right halves the width, centered as before.
    assert!(e.set_crop(ppt_model::Crop { left: 0.25, right: 0.25, top: 0.0, bottom: 0.0 }));
    let x = xfrm(&e, id);
    assert_eq!((x.w, x.h), ((before.w as f64 / 2.0).round() as i64, before.h));
    assert_eq!(x.x, before.x + (before.w as f64 / 4.0).round() as i64);
    // The whole image keeps its place in crop mode.
    assert!(e.set_crop_mode(true));
    let (_, full) = e.crop_view().unwrap();
    assert_eq!((full.x, full.w), (before.x, before.w));
    // Dragging the right handle outwards uncovers what was trimmed, no further.
    e.crop_drag(id, x, ppt_model::Crop { left: 0.25, right: 0.25, top: 0.0, bottom: 0.0 }, Handle::E, Xfrm { w: x.w * 4, ..x });
    let c = e.selected_crop().unwrap();
    assert_eq!((c.left, c.right), (0.25, 0.0));
    assert_eq!(xfrm(&e, id).w, (before.w as f64 * 0.75).round() as i64);
    assert!(e.reset_picture(Some((400, 200))));
    assert_eq!(e.selected_crop(), Some(ppt_model::Crop::default()));
    assert_eq!(xfrm(&e, id).h, (xfrm(&e, id).w as f64 / 2.0).round() as i64);
    assert!(e.crop.is_none());
}

#[test]
fn header_and_footer_add_fields_and_skip_title_slides() {
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    let content = (0..e.pres.layouts.len()).find(|&i| !e.pres.layouts[i].placeholders.iter().any(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "ctrTitle"))).unwrap();
    assert!(e.add_slide(&mut eng, Some(content)));
    let hf = HeaderFooter { date: Some(DateText::Fixed("2026/10/30".into())), slide_number: true, footer: Some("機密".into()), skip_title: true };
    assert!(e.set_header_footer(&mut eng, &hf, true));
    let kinds = |e: &Editor, i: usize| e.pres.slides[i].shapes.iter().filter_map(|s| s.placeholder.as_ref().map(|p| p.kind.clone())).filter(|k| matches!(k.as_str(), "dt" | "ftr" | "sldNum")).collect::<Vec<_>>();
    assert!(kinds(&e, 0).is_empty(), "the title slide is skipped");
    assert_eq!(kinds(&e, 1), vec!["dt", "ftr", "sldNum"]);
    e.set_slide(&mut eng, 1);
    assert_eq!(e.header_footer(), HeaderFooter { skip_title: false, ..hf.clone() });
    assert!(!e.set_header_footer(&mut eng, &hf, true), "nothing to change");

    // Saved: the slide number is a field, the footer plain text.
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let xml = zip_part(&saved, "ppt/slides/slide2.xml");
    assert!(xml.contains(r#"type="slidenum""#) && xml.contains("<a:t>2</a:t>") && xml.contains("<a:t>機密</a:t>"), "{xml}");
    let re = ppt_ooxml::parse_pptx(&saved).unwrap();
    let num = re.slides[1].shapes.iter().find(|s| s.placeholder.as_ref().is_some_and(|p| p.kind == "sldNum")).unwrap();
    let run = &num.text.as_ref().unwrap().paragraphs[0].runs[0];
    assert_eq!(run.style.field.as_ref().map(|f| f.kind.as_str()), Some("slidenum"));
    // Turning them off removes them again.
    assert!(e.set_header_footer(&mut eng, &HeaderFooter::default(), true));
    assert!(kinds(&e, 1).is_empty());
}

#[test]
fn shapes_are_drawn_by_dragging_or_clicking() {
    let mut eng = engine();
    let (mut e, _) = editor();
    let n = e.pres.slides[0].shapes.len();
    e.set_draw_tool(&mut eng, Some("star5"));
    assert_eq!(e.hover_cursor(&ev(0.0, 0.0), &TOL), CursorIcon::Crosshair);
    e.pointer_down(&mut eng, &ev(3.0 * IN as f64, 2.0 * IN as f64), &TOL);
    e.pointer_move(&mut eng, &ev(IN as f64, 3.0 * IN as f64), &TOL);
    e.pointer_up(&mut eng, &ev(IN as f64, 3.0 * IN as f64));
    let s = e.pres.slides[0].shapes.last().unwrap();
    assert_eq!((s.geometry.clone(), s.xfrm), (Geometry::preset("star5"), Xfrm::new(IN, 2 * IN, 2 * IN, IN)));
    assert!(e.draw_tool.is_none(), "one shape per pick");
    // A click places an arrow line at a default length.
    e.set_draw_tool(&mut eng, Some(ARROW_LINE));
    e.pointer_down(&mut eng, &ev(IN as f64, IN as f64), &TOL);
    e.pointer_up(&mut eng, &ev(IN as f64, IN as f64));
    let l = e.pres.slides[0].shapes.last().unwrap();
    assert!(l.geometry.is_line() && l.xfrm.w == IN && l.line.unwrap().tail.kind == ppt_model::EndKind::Triangle);
    assert!(e.undo(&mut eng) && e.undo(&mut eng));
    assert_eq!(e.pres.slides[0].shapes.len(), n);
}

#[test]
fn slides_are_copied_cut_and_pasted_within_and_across_decks() {
    use ppt_model::ShapeKind;
    let mut eng = engine();
    let png = std::sync::Arc::new(std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/icons/32x32.png")).unwrap());
    let mut a = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    assert!(a.add_slide(&mut eng, None));
    assert!(a.add_picture(&mut eng, "from-a/pic.png".into(), png.clone(), 32, 32));
    assert!(a.set_slide_hidden(&mut eng, 1, true));
    a.pres.slide_mut(1).unwrap().notes = "notes".into();
    let clip = a.copy_slide(1).unwrap();
    assert!(a.copy_slide(9).is_none());

    // Same deck: an identical slide after slide 1, with shapes of its own.
    let n = a.pres.slides.len();
    assert_eq!(a.paste_slide(&mut eng, &clip, Some(0), true), 0);
    assert_eq!(a.pres.slides.len(), n + 1);
    assert_eq!(a.slide, 1);
    let (src, pasted) = (&a.pres.slides[2], &a.pres.slides[1]);
    assert_eq!((pasted.shapes.len(), pasted.hidden, pasted.notes.as_str()), (src.shapes.len(), true, "notes"));
    assert!(pasted.shapes.iter().all(|s| src.shapes.iter().all(|o| o.id != s.id)));
    assert!(a.undo(&mut eng) && a.pres.slides.len() == n);

    // Another deck: the layout of the same name, the picture comes along, and `None` pastes at the end.
    let mut b = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    let before = b.pres.slides.len();
    assert_eq!(b.paste_slide(&mut eng, &clip, None, false), 0);
    assert_eq!(b.pres.slides.len(), before + 1);
    let slide = b.pres.slides.last().unwrap();
    assert_eq!(slide.layout, clip.slide.layout);
    assert!(slide.source.is_none() && slide.shapes.iter().all(|s| s.source.is_none()));
    let pic = slide.shapes.iter().find_map(|s| if let ShapeKind::Picture { media: Some(k), .. } = &s.kind { Some(k.clone()) } else { None }).unwrap();
    assert_eq!(*b.pres.media.get(&pic).unwrap(), *png);
    // It is written and read back.
    let saved = ppt_ooxml::save_pptx(&b.pres).unwrap();
    let re = ppt_ooxml::parse_pptx(&saved).unwrap();
    assert_eq!(re.slides.len(), before + 1);
    assert!(re.slides.last().unwrap().hidden);
    assert!(b.undo(&mut eng) && b.pres.slides.len() == before);

    // A different picture under the same key is kept apart.
    let mut c = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    c.pres.media.insert("from-a/pic.png".into(), std::sync::Arc::new(vec![1, 2, 3]));
    assert_eq!(c.paste_slide(&mut eng, &clip, None, false), 0);
    let key = c.pres.slides.last().unwrap().shapes.iter().find_map(|s| if let ShapeKind::Picture { media: Some(k), .. } = &s.kind { Some(k.clone()) } else { None }).unwrap();
    assert_ne!(key, "from-a/pic.png");
    assert_eq!(*c.pres.media.get("from-a/pic.png").unwrap(), vec![1, 2, 3]);
    assert_eq!(*c.pres.media.get(&key).unwrap(), *png);
}

#[test]
fn slides_are_hidden_and_shown_again_and_saved() {
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    assert!(e.add_slide(&mut eng, None));
    assert!(e.set_slide_hidden(&mut eng, 1, true));
    assert!(!e.set_slide_hidden(&mut eng, 1, true), "unchanged");
    assert!(!e.set_slide_hidden(&mut eng, 9, true), "no such slide");
    assert!(e.pres.slides[1].hidden && !e.pres.slides[0].hidden);
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    assert!(zip_part(&saved, "ppt/slides/slide2.xml").contains(r#" show="0""#));
    let mut re = Editor::new(ppt_ooxml::parse_pptx(&saved).unwrap());
    assert!(re.pres.slides[1].hidden && !re.pres.slides[0].hidden);
    // Showing it again rewrites the attribute of the loaded slide.
    assert!(re.set_slide_hidden(&mut eng, 1, false));
    let again = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&re.pres).unwrap()).unwrap();
    assert!(again.slides.iter().all(|s| !s.hidden));
    assert!(re.undo(&mut eng) && re.pres.slides[1].hidden);
}

#[test]
fn backgrounds_are_set_reset_applied_to_all_and_saved() {
    use ppt_model::Background;
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    assert!(e.add_slide(&mut eng, None));
    e.set_slide(&mut eng, 0);
    let (_, own) = e.background().unwrap();
    assert!(!own, "the template's slides follow their layout");
    let red = Background::Paint(Paint::Solid(Color::rgb(0xC0, 0, 0)));
    assert!(e.set_background(&mut eng, Some(red.clone())));
    assert!(!e.set_background(&mut eng, Some(red.clone())), "unchanged");
    assert!(e.set_hide_background_graphics(&mut eng, true));
    assert_eq!(e.pres.slide_background(&e.pres.slides[1]), &Background::WHITE);
    assert!(e.apply_background_to_all(&mut eng));
    assert!(e.pres.slides.iter().all(|s| s.background.as_ref() == Some(&red) && s.hide_background_graphics));

    // A picture on slide 2; both are written as `p:bg` and read back.
    e.set_slide(&mut eng, 1);
    let png = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/icons/32x32.png")).unwrap();
    assert!(e.set_background_picture(&mut eng, "new/bg.png".into(), std::sync::Arc::new(png)));
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let xml1 = zip_part(&saved, "ppt/slides/slide1.xml");
    assert!(xml1.contains(r#"showMasterSp="0""#) && xml1.contains(r#"<p:bg><p:bgPr><a:solidFill><a:srgbClr val="C00000"/>"#), "{xml1}");
    let re = ppt_ooxml::parse_pptx(&saved).unwrap();
    assert_eq!(re.slides[0].background, Some(red));
    assert!(re.slides[0].hide_background_graphics);
    let Some(Background::Picture { media }) = &re.slides[1].background else { panic!("picture background") };
    assert!(media.starts_with("ppt/media/") && re.media.contains_key(media));

    // 重設背景: back to the layout's, and the `p:bg` goes away.
    assert!(e.set_background(&mut eng, None));
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    assert!(!zip_part(&saved, "ppt/slides/slide2.xml").contains("<p:bg>"));
}

#[test]
fn theme_variants_and_slide_size_restyle_the_deck() {
    use ppt_ooxml::design;
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    let before = design::theme_info(&e.pres).unwrap();
    let green = design::COLOR_SCHEMES.iter().find(|c| c.name == "Green").unwrap();
    let pres = design::set_color_scheme(&e.pres, green).unwrap();
    e.replace_document(&mut eng, pres);
    let after = design::theme_info(&e.pres).unwrap();
    assert_eq!(after.color_scheme, "Green");
    assert_eq!(after.colors[4], Color::rgb(0x54, 0x9E, 0x39), "accent1");
    assert_eq!(after.font_scheme, before.font_scheme);

    let arial = design::FONT_SCHEMES.iter().find(|f| f.name == "Arial").unwrap();
    let pres = design::set_font_scheme(&e.pres, arial).unwrap();
    e.replace_document(&mut eng, pres);
    let t = design::theme_info(&e.pres).unwrap();
    assert_eq!((t.major.as_str(), t.minor.as_str()), ("Arial", "Arial"));
    assert_eq!(e.pres.minor_font.0, "Arial");

    // Widescreen → 4:3, ensure fit: title placeholders shrink by 3/4 and stay on the slide.
    let (w, h) = (e.pres.slide_width, e.pres.slide_height);
    let title = e.pres.layouts[0].placeholders[0].clone();
    let pres = design::set_slide_size(&e.pres, 9_144_000, 6_858_000, design::Fit::EnsureFit).unwrap();
    e.replace_document(&mut eng, pres);
    assert_eq!((e.pres.slide_width, e.pres.slide_height), (9_144_000, 6_858_000));
    let s = 9_144_000.0 / w as f64;
    let t2 = &e.pres.layouts[0].placeholders[0];
    assert!((t2.xfrm.w as f64 - title.xfrm.w as f64 * s).abs() < 2.0, "{:?} vs {:?}", t2.xfrm, title.xfrm);
    assert!(t2.xfrm.y as f64 >= title.xfrm.y as f64 * s, "centered vertically ({h})");
    let size = |sh: &Shape| sh.text.as_ref().unwrap().paragraphs[0].end_style.size_pt as f64;
    assert!((size(t2) - (size(&title) * s * 10.0).round() / 10.0).abs() < 0.11, "{} vs {}", size(t2), size(&title));
    // One undo step back to the original size.
    assert!(e.undo(&mut eng));
    assert_eq!(e.pres.slide_width, w);
}

#[test]
fn guides_are_dragged_snapped_to_and_dropped_off_the_slide() {
    let mut eng = engine();
    let (mut e, id) = editor();
    assert!(e.add_guide(&mut eng, true));
    let center = e.pres.slide_width / 2;
    assert_eq!(e.pres.view.guides, vec![ppt_model::DrawGuide { vertical: true, pos: center }]);
    assert!(e.pres.view.show_guides);
    // Grabbed away from shapes and moved to 3in; the label shows the distance from the center.
    let y = 5.0 * IN as f64;
    assert!(e.pointer_down(&mut eng, &ev(center as f64 + 10_000.0, y), &TOL));
    e.pointer_move(&mut eng, &ev(3.0 * IN as f64, y), &TOL);
    let label = e.guide_label().unwrap();
    assert_eq!(label, format!("{:.2}", (center as f64 - 3.0 * IN as f64) / 360_000.0));
    e.pointer_up(&mut eng, &ev(3.0 * IN as f64, y));
    assert_eq!(e.pres.view.guides[0].pos, 3 * IN);
    // A shape dragged near the guide snaps its left edge to it.
    e.selection = vec![id];
    e.pointer_down(&mut eng, &ev(1.5 * IN as f64, 1.5 * IN as f64), &TOL);
    e.pointer_move(&mut eng, &ev(3.5 * IN as f64 + 10_000.0, 1.5 * IN as f64), &TOL);
    e.pointer_up(&mut eng, &ev(3.5 * IN as f64 + 10_000.0, 1.5 * IN as f64));
    assert_eq!(xfrm(&e, id).x, 3 * IN);
    // Dropped off the slide, the guide goes away; undo brings it back.
    e.selection.clear();
    e.pointer_down(&mut eng, &ev(3.0 * IN as f64, y), &TOL);
    e.pointer_move(&mut eng, &ev(-IN as f64, y), &TOL);
    e.pointer_up(&mut eng, &ev(-IN as f64, y));
    assert!(e.pres.view.guides.is_empty());
    assert!(e.undo(&mut eng));
    assert_eq!(e.pres.view.guides.len(), 1);
}

#[test]
fn snap_to_grid_moves_edges_onto_grid_lines() {
    let mut eng = engine();
    let (mut e, id) = editor();
    assert!(e.set_snap_to_grid(&mut eng, true));
    assert!(e.set_grid_spacing(&mut eng, 360_000));
    let w = e.pres.slide_width as f64;
    // Far from every smart guide: the left edge lands on a grid line through the center.
    e.selection = vec![id];
    e.pointer_down(&mut eng, &ev(1.5 * IN as f64, 1.5 * IN as f64), &TOL);
    e.pointer_move(&mut eng, &ev(2.5 * IN as f64 + 123_456.0, 2.5 * IN as f64 + 77_777.0), &TOL);
    e.pointer_up(&mut eng, &ev(2.5 * IN as f64 + 123_456.0, 2.5 * IN as f64 + 77_777.0));
    let x = xfrm(&e, id);
    let off = (x.x as f64 - w / 2.0).rem_euclid(360_000.0);
    assert!(off < 1.0 || off > 359_999.0, "{x:?}");
}

#[test]
fn new_guides_step_off_existing_ones() {
    let mut eng = engine();
    let (mut e, _) = editor();
    assert!(e.add_guide(&mut eng, false));
    assert!(e.add_guide(&mut eng, false));
    let g = &e.pres.view.guides;
    assert_eq!(g.len(), 2);
    assert!((g[1].pos - g[0].pos - 360_000).abs() < 1_600, "{g:?}");
}

#[test]
fn keyboard_tabs_through_groups_steps_in_and_out_rotates_and_nudges() {
    let mut eng = engine();
    let (mut e, a) = editor();
    let b = add_rect(&mut e, 3 * IN, IN);
    let c = add_rect(&mut e, 5 * IN, IN);
    let no = Modifiers::default();
    // Group b and c: Tab treats the group as one stop.
    e.selection = vec![b, c];
    assert!(e.group_selected());
    e.selection.clear();
    assert!(e.key_down(&mut eng, "Tab", no));
    assert_eq!(e.selection, vec![a]);
    assert!(e.key_down(&mut eng, "Tab", no));
    assert!(e.selection_group().is_some() && e.selection.len() == 2);
    assert!(e.key_down(&mut eng, "Tab", no));
    assert_eq!(e.selection, vec![a], "wraps around");
    assert!(e.key_down(&mut eng, "Tab", Modifiers { shift: true, ..no }));
    // Enter steps into the group; Tab then cycles its members; Esc goes back to the group.
    assert!(e.key_down(&mut eng, "Enter", no));
    assert_eq!(e.selection, vec![b]);
    assert!(e.key_down(&mut eng, "Tab", no));
    assert_eq!(e.selection, vec![c]);
    assert!(e.key_down(&mut eng, "Tab", no));
    assert_eq!(e.selection, vec![b], "stays inside the group");
    assert!(e.key_down(&mut eng, "Escape", no));
    assert!(e.selection_group().is_some() && e.entered_group.is_none());
    assert!(e.key_down(&mut eng, "Escape", no));
    assert!(e.selection.is_empty());

    // Option+→ rotates 15°, Option+Shift+← 1° back.
    e.selection = vec![a];
    assert!(e.key_down(&mut eng, "ArrowRight", Modifiers { alt: true, ..no }));
    assert!(e.key_down(&mut eng, "ArrowLeft", Modifiers { alt: true, shift: true, ..no }));
    assert!((xfrm(&e, a).rot - 14.0).abs() < 1e-9);
    // With snap to grid an arrow moves one grid unit; ⌘ moves finely past it.
    assert!(e.set_snap_to_grid(&mut eng, true));
    let x0 = xfrm(&e, a).x;
    assert!(e.key_down(&mut eng, "ArrowRight", no));
    assert_eq!(xfrm(&e, a).x - x0, e.pres.view.grid_spacing);
    assert!(e.key_down(&mut eng, "ArrowLeft", Modifiers { meta: true, ..no }));
    assert_eq!(xfrm(&e, a).x - x0, e.pres.view.grid_spacing - IN / 400);
}

#[test]
fn selection_pane_lists_hides_renames_and_reorders() {
    let mut eng = engine();
    let (mut e, a) = editor();
    let b = add_rect(&mut e, 3 * IN, IN);
    let c = add_rect(&mut e, 5 * IN, IN);
    e.selection = vec![b, c];
    assert!(e.group_selected());
    let g = e.pres.shape(0, b).unwrap().group.unwrap();
    // Top-most first; group members under their header.
    let rows: Vec<(ShapeId, u8)> = e.pane_items().iter().map(|i| (i.id, i.depth)).collect();
    assert_eq!(rows, vec![(g, 0), (c, 1), (b, 1), (a, 0)]);
    // Selecting a member enters its group; the header selects the whole group.
    assert!(e.select_in_pane(&mut eng, c, false));
    assert_eq!((e.selection.clone(), e.entered_group), (vec![c], Some(g)));
    assert!(e.select_in_pane(&mut eng, g, false));
    assert!(e.selection_group() == Some(g) && e.entered_group.is_none());
    // Hiding the group hides both members, deselects them, and they no longer hit.
    assert!(e.select_in_pane(&mut eng, c, false));
    assert!(e.set_shape_hidden(&mut eng, g, true));
    assert_eq!(e.entered_group, None, "a hidden group is left");
    assert!(e.selection.is_empty() && e.pane_items()[0].hidden);
    assert_eq!(e.hit_test(3.5 * IN as f64, 1.5 * IN as f64, 0.0), None);
    assert!(!e.select_in_pane(&mut eng, b, false), "hidden shapes are not selectable");
    assert!(e.set_all_hidden(&mut eng, false));
    // Rename, and move `a` up to the front.
    assert!(e.rename_shape(&mut eng, a, "標誌"));
    assert!(!e.rename_shape(&mut eng, a, "  "));
    assert!(e.move_in_pane(&mut eng, a, true));
    assert_eq!(e.pane_items()[0].name, "標誌");
}

#[test]
fn alt_text_hidden_and_decorative_marks_are_saved() {
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    let id = e.pres.slides[0].shapes[0].id;
    e.selection = vec![id];
    assert!(e.set_alt_text(&mut eng, " 公司標誌 ", true));
    assert_eq!(e.alt_text(), Some(("公司標誌".to_string(), true)));
    assert!(e.rename_shape(&mut eng, id, "主標題"));
    let second = e.pres.slides[0].shapes[1].id;
    assert!(e.set_shape_hidden(&mut eng, second, true));
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let xml = zip_part(&saved, "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"name="主標題""#) && xml.contains(r#"descr="公司標誌""#) && xml.contains(r#"hidden="1""#), "{xml}");
    assert!(xml.contains("adec:decorative"));
    let re = ppt_ooxml::parse_pptx(&saved).unwrap();
    let s0 = &re.slides[0].shapes[0];
    assert_eq!((s0.name.as_str(), s0.access.alt_text.as_str(), s0.access.decorative), ("主標題", "公司標誌", true));
    assert!(re.slides[0].shapes[1].access.hidden, "hidden shapes load as shapes, not raw XML");
    // Grouped members keep their marks too (the group is not copied verbatim then).
    e.selection = e.pres.slides[0].shapes.iter().take(2).map(|s| s.id).collect();
    assert!(e.set_shape_hidden(&mut eng, second, false));
    e.selection = e.pres.slides[0].shapes.iter().take(2).map(|s| s.id).collect();
    assert!(e.group_selected());
    let re = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&e.pres).unwrap()).unwrap();
    let saved = ppt_ooxml::save_pptx(&re).unwrap();
    let mut again = Editor::new(ppt_ooxml::parse_pptx(&saved).unwrap());
    let member = again.pres.slides[0].shapes[1].id;
    assert!(again.set_shape_hidden(&mut eng, member, true));
    let xml = zip_part(&ppt_ooxml::save_pptx(&again.pres).unwrap(), "ppt/slides/slide1.xml");
    assert!(xml.contains("<p:grpSp>") && xml.contains(r#"hidden="1""#), "{xml}");
    // Clearing the marks removes them again.
    e.selection = vec![id];
    assert!(e.set_alt_text(&mut eng, "", false));
    let xml = zip_part(&ppt_ooxml::save_pptx(&e.pres).unwrap(), "ppt/slides/slide1.xml");
    assert!(!xml.contains("descr=") && !xml.contains("decorative"), "{xml}");
}

/// The test7 sample with the slide of its first vList2 SmartArt showing and that SmartArt selected.
fn smartart_editor(eng: &mut TextEngine, layout: &str) -> (Editor, ShapeId) {
    let deck = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/vr/test7.pptx")).unwrap();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&deck).unwrap());
    let (slide, id) = e
        .pres
        .slides
        .iter()
        .enumerate()
        .find_map(|(i, s)| s.shapes.iter().find(|sh| matches!(&sh.kind, ShapeKind::Diagram { data: Some(d), .. } if d.template.0.layout.ends_with(layout))).map(|sh| (i, sh.id)))
        .unwrap();
    e.set_slide(eng, slide);
    e.selection = vec![id];
    (e, id)
}

#[test]
fn smartart_text_pane_edits_with_undo() {
    let mut eng = engine();
    let (mut e, id) = smartart_editor(&mut eng, "/vList2");
    let info = e.diagram_info().unwrap();
    assert!(info.restructure && !info.nest);
    let first = info.rows[0].text.clone();
    // Typing into one row is one undo step.
    assert!(e.diagram_set_text(&mut eng, 0, "A"));
    assert!(e.diagram_set_text(&mut eng, 0, "AB"));
    assert_eq!(e.diagram_info().unwrap().rows[0].text, "AB");
    let n = info.rows.len();
    assert_eq!(e.diagram_insert(&mut eng, 0, ppt_model::diagram::Insert::After), Some(1));
    assert_eq!(e.diagram_info().unwrap().rows.len(), n + 1);
    let ShapeKind::Diagram { shapes, .. } = &e.pres.shape(e.slide, id).unwrap().kind else { panic!() };
    assert_eq!(shapes.len(), info.rows.len() + 1);
    assert!(e.undo(&mut eng));
    assert_eq!(e.diagram_info().unwrap().rows.len(), n);
    assert!(e.undo(&mut eng));
    assert_eq!(e.diagram_info().unwrap().rows[0].text, first);
    // Levels are fixed in this layout.
    assert!(!e.diagram_shift(&mut eng, 1, true));
}

/// Slide point at the middle of drawn shape `k` of SmartArt `id`.
fn smartart_point(e: &Editor, id: ShapeId, k: usize) -> (f64, f64) {
    let shape = e.pres.shape(e.slide, id).unwrap();
    let ShapeKind::Diagram { shapes, w, h, .. } = &shape.kind else { panic!() };
    let r = &shapes[k].xfrm;
    let (sx, sy) = (shape.xfrm.w as f64 / *w as f64, shape.xfrm.h as f64 / *h as f64);
    (shape.xfrm.x as f64 + (r.x + r.w / 2) as f64 * sx, shape.xfrm.y as f64 + (r.y + r.h / 2) as f64 * sy)
}

#[test]
fn double_clicking_a_smartart_node_edits_its_text_in_place() {
    let mut eng = engine();
    let (mut e, id) = smartart_editor(&mut eng, "/vList2");
    let before = e.diagram_info().unwrap().rows[2].text.clone();
    let (x, y) = smartart_point(&e, id, 2);
    e.selection.clear();
    click(&mut e, &mut eng, x, y, 2);
    // Editing on the canvas, not in the text pane.
    assert_eq!(e.diagram_requests, 0);
    assert_eq!(e.selection, vec![id]);
    let edit = e.text.as_ref().expect("editing");
    assert_eq!(edit.diagram, Some((2, 2)));
    assert_eq!(edit.rich.body.plain_text(), before);
    e.text.as_mut().unwrap().rich.select_all();
    assert!(e.insert_text(&mut eng, "直接編輯"));
    // The node and its drawing change when editing ends, as one undo step.
    assert_eq!(e.diagram_info().unwrap().rows[2].text, before);
    e.end_text_edit(&mut eng);
    assert!(e.text.is_none());
    assert_eq!(e.diagram_info().unwrap().rows[2].text, "直接編輯");
    let ShapeKind::Diagram { shapes, .. } = &e.pres.shape(e.slide, id).unwrap().kind else { panic!() };
    assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == "直接編輯")));
    assert!(e.undo(&mut eng));
    assert_eq!(e.diagram_info().unwrap().rows[2].text, before);
    // Leaving the text as it was changes nothing.
    let undo_depth = e.undo.len();
    click(&mut e, &mut eng, x, y, 2);
    assert!(e.text.is_some());
    e.end_text_edit(&mut eng);
    assert_eq!(e.undo.len(), undo_depth);
}

#[test]
fn double_clicking_text_that_shows_children_asks_for_the_text_pane() {
    let mut eng = engine();
    let (mut e, id) = smartart_editor(&mut eng, "/hList1");
    let shape = e.pres.shape(e.slide, id).unwrap();
    let ShapeKind::Diagram { shapes, data: Some(d), .. } = &shape.kind else { panic!() };
    // A shape with text that is a node's but not only its own.
    let k = (0..shapes.len()).find(|&k| shapes[k].text.is_some() && d.row_of_shape(k).is_some() && d.row_of_own_text(k).is_none()).expect("a list shape");
    let want = d.row_of_shape(k);
    let (x, y) = smartart_point(&e, id, k);
    e.selection.clear();
    click(&mut e, &mut eng, x, y, 2);
    assert!(e.text.is_none());
    assert_eq!(e.diagram_requests, 1);
    assert_eq!(e.diagram_focus, want);
    assert_eq!(e.selection, vec![id]);
}

#[test]
fn find_and_replace_reach_smartart_nodes() {
    let mut eng = engine();
    let (mut e, id) = smartart_editor(&mut eng, "/vList2");
    let opts = FindOptions::default();
    let row = e.diagram_info().unwrap().rows[2].text.clone();
    let word: String = row.chars().take(2).collect();
    let found = e.find_all(&word, opts);
    let m = found.iter().find(|m| m.place == FindPlace::Node(id, 2)).cloned().expect("node text is searchable");
    // Showing it opens the text pane at that node with the match selected.
    e.selection.clear();
    assert!(e.select_match(&mut eng, &m));
    assert_eq!((e.selection.clone(), e.diagram_focus, e.diagram_range.clone()), (vec![id], Some(2), Some(m.range.clone())));
    let requests = e.diagram_requests;
    assert!(requests > 0);
    // Replacing goes through the node tree and the drawing, as one undo step.
    assert_eq!(e.current_match(None), Some(m.clone()));
    assert!(e.replace_match(&mut eng, &m, &word, "替換", opts));
    let now = e.diagram_info().unwrap().rows[2].text.clone();
    assert_eq!(now, row.replacen(&word, "替換", 1));
    let ShapeKind::Diagram { shapes, .. } = &e.pres.shape(e.slide, id).unwrap().kind else { panic!() };
    assert!(shapes.iter().any(|s| s.text.as_ref().is_some_and(|t| t.plain_text() == now)));
    assert!(e.undo(&mut eng));
    assert_eq!(e.diagram_info().unwrap().rows[2].text, row);
    // Replace all covers every node, saved through the data model.
    let n = e.replace_all(&mut eng, &word, "替換", opts);
    assert!(n >= 1);
    assert!(e.find_all(&word, opts).iter().all(|m| !matches!(m.place, FindPlace::Node(..))));
    let back = ppt_ooxml::parse_pptx(&ppt_ooxml::save_pptx(&e.pres).unwrap()).unwrap();
    // Shape ids are reassigned on opening: look for the SmartArt that has the text.
    let saved = back.slides[e.slide].shapes.iter().filter_map(|s| match &s.kind {
        ShapeKind::Diagram { data: Some(d), .. } => Some(d.rows()),
        _ => None,
    });
    assert!(saved.flatten().any(|r| r.text.contains("替換")));
}

#[test]
fn inserting_a_smartart_selects_it_and_undoes() {
    let mut eng = engine();
    let deck = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/m0-demo.pptx")).unwrap();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&deck).unwrap());
    let shapes = e.pres.slides[0].shapes.len();
    let package = e.pres.package.clone();
    for kind in ppt_ooxml::smartart_kinds() {
        assert!(e.insert_smartart(&mut eng, kind), "{kind}");
        let id = *e.selection.last().unwrap();
        let info = e.diagram_info().expect("selected SmartArt has a text pane");
        assert!(info.restructure && info.rows.len() >= 3, "{kind}");
        assert!(matches!(&e.pres.shape(e.slide, id).unwrap().kind, ShapeKind::Diagram { data: Some(_), .. }));
    }
    assert!(!e.insert_smartart(&mut eng, "noSuchLayout"));
    assert_eq!(e.pres.slides[0].shapes.len(), shapes + ppt_ooxml::smartart_kinds().len());
    // Undo takes the last one away and brings back the deck's package of before it.
    let n = ppt_ooxml::smartart_kinds().len();
    for _ in 0..n {
        assert!(e.undo(&mut eng));
    }
    assert_eq!(e.pres.slides[0].shapes.len(), shapes);
    assert!(std::sync::Arc::ptr_eq(e.pres.package.as_ref().unwrap(), package.as_ref().unwrap()));
    // Redo puts them back, with their parts: the deck saves with six SmartArts.
    for _ in 0..n {
        assert!(e.redo(&mut eng));
    }
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let back = ppt_ooxml::parse_pptx(&saved).unwrap();
    assert_eq!(back.slides[0].shapes.iter().filter(|s| matches!(&s.kind, ShapeKind::Diagram { data: Some(_), .. })).count(), n);
}

#[test]
fn smartart_nodes_are_spell_checked_where_they_are_drawn() {
    let mut eng = engine();
    let (mut e, id) = smartart_editor(&mut eng, "/vList2");
    let rows = e.diagram_info().unwrap().rows;
    // Every node drawn in a shape of its own is a text to check.
    let nodes: Vec<(FindPlace, String)> = e.slide_texts().into_iter().filter(|(p, _)| matches!(p, FindPlace::Node(i, _) if *i == id)).collect();
    assert_eq!(nodes.len(), rows.len());
    assert_eq!(nodes[2], (FindPlace::Node(id, 2), rows[2].text.clone()));
    // Its characters are laid out inside the node's shape, in slide space.
    let (layout, tf) = e.text_geometry(&mut eng, FindPlace::Node(id, 2)).expect("geometry");
    let rects = crate::range_rects(&layout, 0..rows[2].text.len().min(3));
    assert!(!rects.is_empty());
    let (x, y) = smartart_point(&e, id, 2);
    let at = tf * ppt_geometry::kurbo::Point::new(rects[0].x0, (rects[0].y0 + rects[0].y1) / 2.0);
    let shape = e.pres.shape(e.slide, id).unwrap();
    let ShapeKind::Diagram { shapes, .. } = &shape.kind else { panic!() };
    let half_h = shapes[2].xfrm.h as f64 / 2.0 * shape.xfrm.h as f64 / shape.xfrm.h as f64;
    assert!((at.y - y).abs() <= half_h, "inside the node's box vertically: {} vs {y}", at.y);
    assert!(at.x <= x, "the word starts left of the box's middle");
    // A correction goes through the node: one undo step.
    let m = FindMatch { slide: e.slide, place: FindPlace::Node(id, 2), range: 0..rows[2].text.chars().next().unwrap().len_utf8() };
    assert!(e.replace_range(&mut eng, &m, "更正"));
    assert!(e.diagram_info().unwrap().rows[2].text.starts_with("更正"));
    assert!(e.undo(&mut eng));
    assert_eq!(e.diagram_info().unwrap().rows[2].text, rows[2].text);
    // While the node is edited on the canvas, its live text is the one checked.
    click(&mut e, &mut eng, x, y, 2);
    e.text.as_mut().unwrap().rich.select_all();
    assert!(e.insert_text(&mut eng, "live"));
    assert!(e.slide_texts().contains(&(FindPlace::Node(id, 2), "live".to_string())));
    let (layout, _) = e.text_geometry(&mut eng, FindPlace::Node(id, 2)).expect("live geometry");
    assert!(!crate::range_rects(&layout, 0..4).is_empty());
}

#[test]
fn format_painter_copies_shape_and_text_format_in_one_undo_step() {
    let (mut e, src) = editor();
    let dst = add_rect(&mut e, 3 * IN, IN);
    let mut eng = engine();
    // Source: a filled shape with an outline, bold red text, centered.
    e.selection = vec![src];
    e.set_line_color(Some(Color::rgb(0, 0, 255)));
    e.pres.slide_mut(0).unwrap().shapes[0].text = Some({
        let mut b = TextBody::default();
        b.set_plain_text("abc");
        b
    });
    e.apply_text_style(&mut eng, |s| {
        s.bold = true;
        s.color = Color::rgb(255, 0, 0);
    });
    e.apply_paragraph(&mut eng, |p| p.align = Align::Center);
    assert!(e.copy_format());

    // Target: another shape with text; painting needs a target first.
    e.pres.slide_mut(0).unwrap().shapes[1].text = Some({
        let mut b = TextBody::default();
        b.set_plain_text("xyz");
        b
    });
    e.selection.clear();
    assert!(!e.paste_format(&mut eng));
    e.selection = vec![dst];
    let undo_len = e.undo.len();
    assert!(e.paste_format(&mut eng));
    assert_eq!(e.undo.len(), undo_len + 1);
    let shape = e.pres.shape(0, dst).unwrap();
    assert!(matches!(shape.fill, Some(Paint::Solid(c)) if c == Color::BLACK));
    assert_eq!(shape.line.map(|l| l.color), Some(Color::rgb(0, 0, 255)));
    let text = shape.text.as_ref().unwrap();
    assert!(text.style_at(0).bold);
    assert_eq!(text.style_at(0).color, Color::rgb(255, 0, 0));
    assert_eq!(text.paragraphs[0].align, Align::Center);

    assert!(e.undo(&mut eng));
    let shape = e.pres.shape(0, dst).unwrap();
    assert!(shape.fill.is_none() && shape.line.is_none());
    assert!(!shape.text.as_ref().unwrap().style_at(0).bold);
}

#[test]
fn right_click_selects_the_object_under_the_pointer() {
    let (mut e, a) = editor();
    let b = add_rect(&mut e, 3 * IN, IN);
    let mut eng = engine();
    let at = |x: i64, y: i64| PointerEvent { x: x as f64, y: y as f64, shift: false, alt: false, meta: false, clicks: 0 };
    // On the second shape: it becomes the selection.
    e.selection = vec![a];
    let (bx, by) = (3 * IN + IN / 2, IN + IN / 2);
    assert_eq!(e.context_target(&mut eng, &at(bx, by), &TOL), ContextTarget::Object);
    assert_eq!(e.selection, vec![b]);
    // Again on the same shape: the selection stays.
    assert_eq!(e.context_target(&mut eng, &at(bx, by), &TOL), ContextTarget::Object);
    assert_eq!(e.selection, vec![b]);
    // On empty space: nothing is selected.
    assert_eq!(e.context_target(&mut eng, &at(0, 0), &TOL), ContextTarget::Blank);
    assert!(e.selection.is_empty());
}

#[test]
fn locked_shapes_cannot_move_resize_or_rotate_and_the_lock_is_saved() {
    let mut eng = engine();
    let mut e = Editor::new(ppt_ooxml::parse_pptx(&ppt_ooxml::template_package()).unwrap());
    let id = e.pres.slides[0].shapes[0].id;
    e.selection = vec![id];
    let at = e.pres.slides[0].shapes[0].xfrm;
    assert!(!e.selection_all_locked());
    assert!(!e.handles(&TOL).is_empty());

    let undo_len = e.undo.len();
    assert!(e.set_locked(&mut eng, true));
    assert_eq!(e.undo.len(), undo_len + 1);
    assert!(e.selection_all_locked());
    assert!(!e.set_locked(&mut eng, true), "already locked");
    // Nothing that moves, resizes or turns the shape works any more.
    assert!(e.handles(&TOL).is_empty());
    assert!(!e.nudge(10, 10));
    assert!(!e.rotate_selection(90.0));
    assert!(!e.flip_selection(true));
    assert!(!e.set_selection_frame(Xfrm { x: at.x + 5, ..at }));
    assert!(!e.align(AlignTo::Left));
    assert_eq!(e.pres.slides[0].shapes[0].xfrm, at);
    // Dragging it does nothing either.
    let (cx, cy) = (at.x + at.w / 2, at.y + at.h / 2);
    let ev = |x: i64, y: i64| PointerEvent { x: x as f64, y: y as f64, shift: false, alt: false, meta: false, clicks: 1 };
    e.pointer_down(&mut eng, &ev(cx, cy), &TOL);
    e.pointer_move(&mut eng, &ev(cx + IN, cy + IN), &TOL);
    e.pointer_up(&mut eng, &ev(cx + IN, cy + IN));
    assert_eq!(e.pres.slides[0].shapes[0].xfrm, at);

    // The lock survives saving, and a second save keeps it.
    let saved = ppt_ooxml::save_pptx(&e.pres).unwrap();
    let xml = zip_part(&saved, "ppt/slides/slide1.xml");
    assert!(xml.contains(r#"noMove="1""#) && xml.contains(r#"noResize="1""#), "{xml}");
    let mut re = Editor::new(ppt_ooxml::parse_pptx(&saved).unwrap());
    assert!(re.pres.slides[0].shapes[0].access.locked);
    // Unlocking removes the lock attributes again (a placeholder keeps its own noGrp).
    re.selection = vec![re.pres.slides[0].shapes[0].id];
    assert!(re.set_locked(&mut eng, false));
    let xml = zip_part(&ppt_ooxml::save_pptx(&re.pres).unwrap(), "ppt/slides/slide1.xml");
    assert!(!xml.contains("noMove") && !xml.contains("noResize"), "{xml}");
    assert!(re.nudge(10, 10));
}

#[test]
fn fill_and_outline_are_set_in_full_and_undone() {
    use ppt_model::{Dash, EndKind, Gradient, GradientKind, LineEnd};
    let (mut e, id) = editor();
    e.selection = vec![id];
    // A gradient fill with a half-clear stop.
    let gradient = Gradient { stops: vec![(0.0, Color::rgb(255, 0, 0)), (1.0, Color { r: 0, g: 0, b: 255, a: 128 })], kind: GradientKind::Linear { angle: 90.0, scaled: false }, rotate_with_shape: true };
    assert!(e.set_fill_spec(FillSpec::Gradient(gradient.clone())));
    assert_eq!(e.pres.shape(0, id).unwrap().fill, Some(Paint::Gradient(std::sync::Arc::new(gradient))));
    // Doing the same again changes nothing and is not an undo step.
    let before = e.undo.len();
    assert!(!e.set_fill_spec(FillSpec::Gradient(Gradient { stops: vec![(0.0, Color::rgb(255, 0, 0)), (1.0, Color { r: 0, g: 0, b: 255, a: 128 })], kind: GradientKind::Linear { angle: 90.0, scaled: false }, rotate_with_shape: true })));
    assert_eq!(e.undo.len(), before);
    assert!(e.set_fill_spec(FillSpec::Solid(Color { r: 1, g: 2, b: 3, a: 100 })));
    assert_eq!(e.pres.shape(0, id).unwrap().fill, Some(Paint::Solid(Color { r: 1, g: 2, b: 3, a: 100 })));
    assert!(e.set_fill_spec(FillSpec::None));
    assert_eq!(e.pres.shape(0, id).unwrap().fill, None);

    // Outline: giving a width gives a shape without one a black outline; a rectangle takes no arrowheads.
    assert!(e.set_line_spec(LineSpec { width: Some(38_100), dash: Some(Dash::Dash), head: Some(LineEnd { kind: EndKind::Arrow, width: 2, length: 2 }), ..Default::default() }));
    let line = e.pres.shape(0, id).unwrap().line.unwrap();
    assert_eq!((line.color, line.width, line.dash, line.head), (Color::BLACK, 38_100, Dash::Dash, LineEnd::NONE));
    // Color and opacity change separately.
    assert!(e.set_line_spec(LineSpec { rgb: Some((10, 20, 30)), ..Default::default() }));
    assert!(e.set_line_spec(LineSpec { alpha: Some(64), ..Default::default() }));
    assert_eq!(e.pres.shape(0, id).unwrap().line.unwrap().color, Color { r: 10, g: 20, b: 30, a: 64 });
    assert!(e.set_line_spec(LineSpec { visible: Some(false), ..Default::default() }));
    assert_eq!(e.pres.shape(0, id).unwrap().line, None);
    // Every step undoes.
    while e.can_undo() {
        e.undo(&mut engine());
    }
    assert_eq!(e.pres.shape(0, id).unwrap().fill, Some(Paint::Solid(Color::BLACK)));
}

#[test]
fn a_line_takes_arrowheads_and_the_pane_reads_back_mixed_values() {
    use ppt_model::{EndKind, LineEnd};
    let (mut e, id) = editor();
    let line_id = e.pres.alloc_id();
    let mut line = Shape::new(line_id, "line", Xfrm::new(0, 0, IN, IN));
    line.geometry = Geometry::preset("line");
    line.line = Some(Line::new(Color::BLACK, 12_700));
    e.pres.slide_mut(0).unwrap().shapes.push(line);
    e.selection = vec![line_id];
    let arrow = LineEnd { kind: EndKind::Triangle, width: 1, length: 1 };
    assert!(e.set_line_spec(LineSpec { tail: Some(arrow), ..Default::default() }));
    assert_eq!(e.pres.shape(0, line_id).unwrap().line.unwrap().tail, arrow);
    let info = e.format_info().unwrap();
    assert!(info.can_arrows && !info.can_fill);
    // A rectangle and a line together: the outlines differ.
    e.selection = vec![id, line_id];
    let info = e.format_info().unwrap();
    assert!(info.can_fill && info.can_arrows);
    assert_eq!(info.line, None);
    assert_eq!(info.fill, Some(Some(Paint::Solid(Color::BLACK))));
}

#[test]
fn the_shadow_is_set_in_detail() {
    let (mut e, id) = editor();
    e.selection = vec![id];
    assert!(e.pres.shape(0, id).unwrap().shadow.is_none());
    // Touching a detail gives a shape without a shadow the default one with that change.
    assert!(e.set_shadow_spec(ShadowSpec { blur: Some(127_000), ..Default::default() }));
    let s = e.pres.shape(0, id).unwrap().shadow.unwrap();
    assert_eq!((s.blur, s.dist, s.dir, s.color.a), (127_000, 38_100, 45.0, 102));
    assert!(e.set_shadow_spec(ShadowSpec { rgb: Some((200, 0, 0)), alpha: Some(200), dist: Some(0), dir: Some(-90.0), ..Default::default() }));
    let s = e.pres.shape(0, id).unwrap().shadow.unwrap();
    assert_eq!((s.color, s.dist, s.dir), (Color { r: 200, g: 0, b: 0, a: 200 }, 0, 270.0));
    let info = e.format_info().unwrap();
    assert!(info.can_shadow);
    assert_eq!(info.shadow, Some(Some(s)));
    // The same again is not a step; removing it is.
    assert!(!e.set_shadow_spec(ShadowSpec { dist: Some(0), ..Default::default() }));
    assert!(e.set_shadow_spec(ShadowSpec { visible: Some(false), ..Default::default() }));
    assert!(e.pres.shape(0, id).unwrap().shadow.is_none());
    // A shape without a shadow stays without one when only "hidden" is asked.
    assert!(!e.set_shadow_spec(ShadowSpec { visible: Some(false), ..Default::default() }));
}

#[test]
fn lock_aspect_keeps_proportions_while_resizing() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.selection = vec![id];
    // A 1in square made 2in × 1in at its bottom right corner would be 2:1; locked it stays 1:1.
    assert!(e.set_lock_aspect(true));
    assert!(e.pres.shape(0, id).unwrap().access.lock_aspect);
    assert!(!e.set_lock_aspect(true), "same value, no step");
    e.pointer_down(&mut eng, &ev(2.0 * IN as f64, 2.0 * IN as f64), &TOL);
    e.pointer_move(&mut eng, &ev(4.0 * IN as f64, 2.0 * IN as f64), &TOL);
    e.pointer_up(&mut eng, &ev(4.0 * IN as f64, 2.0 * IN as f64));
    let x = xfrm(&e, id);
    assert_eq!(x.w, x.h, "{x:?}");
    // The right edge handle scales the height too (the box grows about its middle).
    let (cy_before, w_before) = (x.y + x.h / 2, x.w);
    let right_mid = (x.x + x.w) as f64;
    e.pointer_down(&mut eng, &ev(right_mid, cy_before as f64), &TOL);
    e.pointer_move(&mut eng, &ev(right_mid + IN as f64, cy_before as f64), &TOL);
    e.pointer_up(&mut eng, &ev(right_mid + IN as f64, cy_before as f64));
    let y = xfrm(&e, id);
    assert!(y.w > w_before && y.w == y.h, "{y:?}");
    assert!((y.y + y.h / 2 - cy_before).abs() <= 1, "centered vertically");
}

#[test]
fn the_text_box_settings_are_set_and_an_auto_fit_shape_follows_its_text() {
    let mut eng = engine();
    let (mut e, id) = editor();
    use ppt_model::{Paragraph, Run};
    let para = |t: &str| Paragraph { runs: vec![Run { text: t.into(), ..Default::default() }], ..Default::default() };
    e.pres.slide_mut(0).unwrap().shapes[0].text = Some(TextBody { paragraphs: vec![para("line one"), para("line two"), para("line three")], defaults: e.pres.text_defaults.clone(), ..Default::default() });
    e.selection = vec![id];
    let spec = TextBoxSpec { anchor: Some(Anchor::Middle), insets: [Some(91_440), None, Some(0), None], wrap: Some(false), ..Default::default() };
    assert!(e.set_text_box_spec(&mut eng, spec.clone()));
    let body = e.pres.shape(0, id).unwrap().text.clone().unwrap();
    assert_eq!((body.anchor, body.insets[0], body.insets[2], body.wrap), (Anchor::Middle, 91_440, 0, false));
    assert!(!e.set_text_box_spec(&mut eng, spec), "same values, no step");
    // "Resize shape to fit text" resizes right away.
    let before_h = xfrm(&e, id).h;
    assert!(e.set_text_box_spec(&mut eng, TextBoxSpec { auto_grow: Some(true), ..Default::default() }));
    assert_ne!(xfrm(&e, id).h, before_h);
    let info = e.format_info().unwrap();
    assert_eq!(info.text_box.unwrap().auto_grow, Some(true));
    // One undo for that step, then the earlier one.
    e.undo(&mut eng);
    assert_eq!(xfrm(&e, id).h, before_h);
    assert!(!e.pres.shape(0, id).unwrap().text.clone().unwrap().auto_grow);
}

#[test]
fn text_outline_and_shadow_apply_to_the_selected_text() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.pres.slide_mut(0).unwrap().shapes[0].text = Some({
        let mut b = TextBody::default();
        b.set_plain_text("abcdef");
        b
    });
    e.selection = vec![id];
    assert_eq!(e.text_effect_info(), Some(TextEffectInfo { outline: Some(None), shadow: Some(None) }));

    // Touching one detail gives the text the default outline or shadow with that change; one undo step.
    let undo_len = e.undo.len();
    let spec = TextEffectSpec { outline: TextOutlineSpec { rgb: Some((255, 0, 0)), width: Some(25_400), ..Default::default() }, shadow: ShadowSpec { blur: Some(127_000), ..Default::default() } };
    assert!(e.set_text_effect_spec(&mut eng, spec.clone()));
    assert_eq!(e.undo.len(), undo_len + 1);
    let style = e.pres.shape(0, id).unwrap().text.as_ref().unwrap().style_at(1);
    let outline = style.outline.unwrap();
    assert_eq!((outline.color.r, outline.width), (255, 25_400));
    assert_eq!(style.shadow.unwrap().blur, 127_000);
    // The same again is not a step.
    assert!(!e.set_text_effect_spec(&mut eng, spec));
    assert_eq!(e.undo.len(), undo_len + 1);

    // Only what the spec names changes, so other text formatting stays.
    assert!(e.apply_text_style(&mut eng, |s| s.bold = true));
    assert!(e.set_text_effect_spec(&mut eng, TextEffectSpec { outline: TextOutlineSpec { alpha: Some(100), ..Default::default() }, ..Default::default() }));
    let style = e.pres.shape(0, id).unwrap().text.as_ref().unwrap().style_at(1);
    assert!(style.bold && style.outline.unwrap().color.a == 100 && style.shadow.is_some());

    // Undo restores the text without effects.
    assert!(e.undo(&mut eng) && e.undo(&mut eng) && e.undo(&mut eng));
    let style = e.pres.shape(0, id).unwrap().text.as_ref().unwrap().style_at(1);
    assert!(style.outline.is_none() && style.shadow.is_none());
}

#[test]
fn text_effects_of_a_text_selection_leave_the_rest_and_report_mixed_values() {
    let (mut e, id) = editor();
    let mut eng = engine();
    e.pres.slide_mut(0).unwrap().shapes[0].text = Some({
        let mut b = TextBody::default();
        b.set_plain_text("abcdef");
        b
    });
    e.selection = vec![id];
    e.begin_text_edit(&mut eng, id, false);
    e.text.as_mut().unwrap().rich.select_range(1..3);
    assert!(e.set_text_effect_spec(&mut eng, TextEffectSpec { shadow: ShadowSpec { visible: Some(true), ..Default::default() }, ..Default::default() }));
    // The selection has a shadow; widening it to everything shows the values differ.
    assert!(matches!(e.text_effect_info(), Some(TextEffectInfo { shadow: Some(Some(_)), .. })));
    e.text.as_mut().unwrap().rich.select_range(0..6);
    assert_eq!(e.text_effect_info().unwrap().shadow, None);
    let body = &e.text.as_ref().unwrap().rich.body;
    assert!(body.style_at(1).shadow.is_none() && body.style_at(2).shadow.is_some() && body.style_at(4).shadow.is_none());
}

#[test]
fn format_painter_carries_text_outline_and_shadow() {
    let (mut e, src) = editor();
    let dst = add_rect(&mut e, 3 * IN, IN);
    let mut eng = engine();
    for (i, text) in ["abc", "xyz"].into_iter().enumerate() {
        e.pres.slide_mut(0).unwrap().shapes[i].text = Some({
            let mut b = TextBody::default();
            b.set_plain_text(text);
            b
        });
    }
    e.selection = vec![src];
    assert!(e.set_text_effect_spec(
        &mut eng,
        TextEffectSpec { outline: TextOutlineSpec { rgb: Some((0, 128, 0)), width: Some(19_050), ..Default::default() }, shadow: ShadowSpec { visible: Some(true), ..Default::default() } },
    ));
    assert!(e.copy_format());
    e.selection = vec![dst];
    assert!(e.paste_format(&mut eng));
    let style = e.pres.shape(0, dst).unwrap().text.as_ref().unwrap().style_at(1);
    assert_eq!(style.outline.map(|o| (o.color.g, o.width)), Some((128, 19_050)));
    assert!(style.shadow.is_some());
    // Painting from text without effects clears them again.
    e.selection = vec![src];
    assert!(e.set_text_effect_spec(&mut eng, TextEffectSpec { outline: TextOutlineSpec { visible: Some(false), ..Default::default() }, shadow: ShadowSpec { visible: Some(false), ..Default::default() } }));
    assert!(e.copy_format());
    e.selection = vec![dst];
    assert!(e.paste_format(&mut eng));
    let style = e.pres.shape(0, dst).unwrap().text.as_ref().unwrap().style_at(1);
    assert!(style.outline.is_none() && style.shadow.is_none());
}

#[test]
fn undo_snapshots_share_untouched_slides() {
    let (mut e, id) = editor();
    e.pres.slides.push(std::sync::Arc::new(Slide::default()));
    e.selection = vec![id];
    assert!(e.nudge(IN, 0));
    let snap = &e.undo.last().unwrap().pres;
    // The edited slide was copied; the other one is still shared with the snapshot.
    assert!(!std::sync::Arc::ptr_eq(&snap.slides[0], &e.pres.slides[0]));
    assert!(std::sync::Arc::ptr_eq(&snap.slides[1], &e.pres.slides[1]));
    assert!(std::sync::Arc::ptr_eq(&snap.layouts, &e.pres.layouts));
    assert_eq!(snap.slides[0].shapes[0].xfrm.x, IN);
    assert_eq!(xfrm(&e, id).x, 2 * IN);

    let mut engine = engine();
    assert!(e.undo(&mut engine));
    assert_eq!(xfrm(&e, id).x, IN);
}
