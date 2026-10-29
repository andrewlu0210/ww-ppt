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
    pres.slides.push(Slide { shapes: vec![shape], ..Default::default() });
    (Editor::new(pres), id)
}

fn add_rect(e: &mut Editor, x: i64, y: i64) -> ShapeId {
    let id = e.pres.alloc_id();
    e.pres.slides[0].shapes.push(Shape::new(id, "r", Xfrm::new(x, y, IN, IN)));
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
    e.pres.layouts.push(Layout { name: "Title".into(), placeholders: vec![title], ..Default::default() });

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
    e.pres.slides[0].shapes.push(line);
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
    pres.slides.push(Slide::default());
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
    pres.slides.push(Slide::default());
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
    if let ShapeKind::Table(t) = &mut e.pres.slides[0].shapes[0].kind {
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
    pres.slides.push(Slide { shapes, ..Default::default() });
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
