//! Chart drawing: layout (title, legend, plot area), value axes with Excel-like
//! automatic scales, and bar, line, area, pie and doughnut plots.
//!
//! Everything is computed in the chart's local frame (EMU, origin at its
//! top-left) and mapped to the device by one transform.

use std::f64::consts::PI;

use ppt_model::chart::{Axis, AxisPos, Chart, DataLabels, Grouping, LabelPosition, LegendPos, MarkerSymbol, Plot, PlotKind, Series, format_number};
use ppt_model::{Color, EMU_PER_PT, Line as ModelLine, Paint, RunStyle, TextBody, Xfrm};
use ppt_text::{BodyLayout, TextEngine};
use vello::kurbo::{Affine, BezPath, Circle, Line, Point, Rect, Shape as _, Stroke, Vec2};
use vello::peniko::Fill;

use crate::{Canvas, CanvasExt, color, draw_body_layout, fill_paint, stroke_line};
use ppt_geometry::shape_affine;

const PT: f64 = EMU_PER_PT;

/// A measured single- or multi-line label.
struct Label {
    layout: BodyLayout,
    w: f64,
    h: f64,
}

fn measure(engine: &mut TextEngine, text: &str, style: &RunStyle) -> Label {
    let mut body = TextBody { insets: [0; 4], wrap: false, ..Default::default() };
    body.paragraphs[0].end_style = style.clone();
    body.set_plain_text(text);
    let layout = engine.layout_body(&body, 10_000.0);
    let w = layout.layout.lines().map(|l| l.metrics().advance as f64).fold(0.0, f64::max) * PT;
    let h = layout.height as f64 * PT;
    Label { layout, w, h }
}

/// Draws `label` with its anchor point (fractions of its size) at `at`.
fn place(scene: &mut dyn Canvas, tf: Affine, label: &Label, at: Point, ax: f64, ay: f64) {
    let origin = Vec2::new(at.x - label.w * ax, at.y - label.h * ay);
    draw_body_layout(scene, &label.layout, tf * Affine::translate(origin) * Affine::scale(PT));
}

/// Draws `label` rotated by `angle` (radians) around its anchor at `at`.
fn place_rotated(scene: &mut dyn Canvas, tf: Affine, label: &Label, at: Point, ax: f64, ay: f64, angle: f64) {
    let t = tf * Affine::translate(at.to_vec2()) * Affine::rotate(angle) * Affine::translate(Vec2::new(-label.w * ax, -label.h * ay)) * Affine::scale(PT);
    draw_body_layout(scene, &label.layout, t);
}

/// An axis scale: `min..max` in steps of `unit`.
#[derive(Clone, Copy, Debug)]
struct Scale {
    min: f64,
    max: f64,
    unit: f64,
}

impl Scale {
    fn frac(&self, v: f64) -> f64 {
        if self.max == self.min { 0.0 } else { (v - self.min) / (self.max - self.min) }
    }

    fn ticks(&self) -> Vec<f64> {
        let n = ((self.max - self.min) / self.unit).round() as i64;
        (0..=n.clamp(0, 100)).map(|i| self.min + i as f64 * self.unit).collect()
    }
}

/// Excel's automatic scale: zero-based for non-negative data, 5% headroom,
/// and the smallest 1/2/5 step that gives at most ten intervals.
fn auto_scale(lo: f64, hi: f64, axis: Option<&Axis>) -> Scale {
    let (mut lo, mut hi) = (lo.min(hi), lo.max(hi));
    if !lo.is_finite() || !hi.is_finite() {
        (lo, hi) = (0.0, 1.0);
    }
    if lo > 0.0 {
        lo = 0.0;
    }
    if hi < 0.0 {
        hi = 0.0;
    }
    if hi == lo {
        hi = lo + 1.0;
    }
    let fixed_min = axis.and_then(|a| a.min);
    let fixed_max = axis.and_then(|a| a.max);
    let span = (fixed_max.unwrap_or(hi) - fixed_min.unwrap_or(lo)).abs().max(1e-12);
    let head = |v: f64, up: bool| if up { v + span * 0.05 } else { v - span * 0.05 };
    let unit = axis.and_then(|a| a.major_unit).unwrap_or_else(|| {
        let range = (fixed_max.unwrap_or(if hi > 0.0 { head(hi, true) } else { hi }) - fixed_min.unwrap_or(if lo < 0.0 { head(lo, false) } else { lo })).max(1e-12);
        let mut mag = 10f64.powf(range.log10().floor() - 1.0);
        loop {
            for f in [1.0, 2.0, 5.0] {
                if (range / (f * mag)).ceil() <= 10.0 {
                    return f * mag;
                }
            }
            mag *= 10.0;
        }
    });
    let min = fixed_min.unwrap_or_else(|| if lo < 0.0 { (head(lo, false) / unit).floor() * unit } else { 0.0 });
    let max = fixed_max.unwrap_or_else(|| if hi > 0.0 { (head(hi, true) / unit).ceil() * unit } else { 0.0 });
    Scale { min, max: if max <= min { min + unit } else { max }, unit }
}

fn stacked(g: Grouping) -> bool {
    matches!(g, Grouping::Stacked | Grouping::PercentStacked)
}

/// Data range of a plot as it will be drawn (stacks summed, percents 0..1).
fn plot_range(p: &Plot) -> (f64, f64) {
    let grouping = match p.kind {
        PlotKind::Bar { grouping, .. } | PlotKind::Line { grouping } | PlotKind::Area { grouping } => grouping,
        PlotKind::Pie { .. } => return (0.0, 1.0),
    };
    let n = p.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    if grouping == Grouping::PercentStacked {
        return (0.0, 1.0);
    }
    if stacked(grouping) {
        for i in 0..n {
            let (mut pos, mut neg) = (0.0, 0.0);
            for s in &p.series {
                let v = s.values.get(i).copied().flatten().unwrap_or(0.0);
                if v >= 0.0 { pos += v } else { neg += v }
            }
            lo = lo.min(neg);
            hi = hi.max(pos);
        }
    } else {
        for v in p.series.iter().flat_map(|s| s.values.iter().flatten()) {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if lo > hi { (0.0, 1.0) } else { (lo, hi) }
}

fn series_fill(chart: &Chart, s: &Series) -> Paint {
    s.fill.clone().unwrap_or(Paint::Solid(chart.auto_color(s.index)))
}

fn series_line(chart: &Chart, s: &Series) -> Option<ModelLine> {
    if s.no_line {
        return None;
    }
    Some(s.line.unwrap_or(ModelLine::new(chart.auto_color(s.index), 28_575)))
}

/// Text of a data label for value `v` of series `s` (point `i`).
fn label_text(l: &DataLabels, s: &Series, i: usize, v: f64, total: f64) -> Option<String> {
    if l.deleted.contains(&i) {
        return None;
    }
    if let Some((_, text)) = l.texts.iter().find(|t| t.0 == i) {
        return Some(text.clone());
    }
    let mut parts = Vec::new();
    if l.show_series {
        parts.push(s.name.clone());
    }
    if l.show_category
        && let Some(c) = s.categories.get(i)
    {
        parts.push(c.clone());
    }
    if l.show_value {
        parts.push(format_number(v, l.format.as_deref().unwrap_or(&s.format)));
    }
    if l.show_percent && total != 0.0 {
        parts.push(format_number(v / total, l.format.as_deref().filter(|f| f.contains('%')).unwrap_or("0%")));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Labels that apply to a series: its own, else the plot's.
fn labels_of<'a>(p: &'a Plot, s: &'a Series) -> Option<&'a DataLabels> {
    s.labels.as_ref().or(p.labels.as_ref()).filter(|l| l.show_value || l.show_percent || l.show_category || l.show_series)
}

fn offset_of(l: &DataLabels, i: usize, w: f64, h: f64) -> Vec2 {
    l.offsets.iter().find(|o| o.0 == i).map_or(Vec2::ZERO, |o| Vec2::new(o.1 * w, o.2 * h))
}

/// The chart drawn into the frame `xf`.
pub fn draw_chart(scene: &mut dyn Canvas, engine: &mut TextEngine, chart: &Chart, xf: &Xfrm, view: Affine) {
    let (w, h) = (xf.w as f64, xf.h as f64);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let tf = view * shape_affine(xf, false);
    let frame = Rect::new(0.0, 0.0, w, h);
    if let Some(fill) = &chart.fill {
        fill_paint(scene, tf, fill, w, h, &frame);
    }
    if let Some(line) = chart.border {
        stroke_line(scene, tf, &line, &frame.to_path(0.1));
    }
    scene.push_clip_layer(Fill::NonZero, tf, &frame);
    let margin = 7.0 * PT;
    let mut area = frame.inset(-margin);

    // Title.
    if let Some(t) = &chart.title
        && !t.text.is_empty()
    {
        let label = measure(engine, &t.text, &t.style);
        match t.layout {
            Some(m) => place(scene, tf, &label, Point::new(m.x * w, m.y * h), 0.0, 0.0),
            None => place(scene, tf, &label, Point::new(w / 2.0, area.y0), 0.5, 0.0),
        }
        // The automatic plot layout leaves the title's room even when the title was moved.
        if !t.overlay {
            area.y0 += label.h + 4.0 * PT;
        }
    }

    // Legend entries: series, or points for single-series varied-color charts.
    if let Some(legend) = &chart.legend {
        let pie = chart.plots.iter().find(|p| p.vary_colors && p.series.len() == 1);
        let mut entries: Vec<(String, Color, bool)> = match pie {
            Some(p) => {
                let s = &p.series[0];
                s.categories.iter().enumerate().map(|(i, c)| (c.clone(), point_color(chart, s, i), false)).collect()
            }
            None => chart
                .plots
                .iter()
                .flat_map(|p| p.series.iter().map(move |s| (p, s)))
                .map(|(p, s)| {
                    let is_line = matches!(p.kind, PlotKind::Line { .. });
                    let c = if is_line { series_line(chart, s).map(|l| l.color).unwrap_or(chart.auto_color(s.index)) } else { series_fill(chart, s).color() };
                    (s.name.clone(), c, is_line)
                })
                .collect(),
        };
        let mut i = 0;
        entries.retain(|_| {
            i += 1;
            !legend.deleted.contains(&(i - 1))
        });
        let labels: Vec<Label> = entries.iter().map(|e| measure(engine, &e.0, &legend.style)).collect();
        let key = legend.style.size_pt as f64 * 0.7 * PT;
        let gap = legend.style.size_pt as f64 * 0.4 * PT;
        let entry_w = |l: &Label| key + gap + l.w + gap * 2.0;
        let row_h = labels.iter().map(|l| l.h).fold(0.0, f64::max);
        let horizontal = matches!(legend.position, LegendPos::Top | LegendPos::Bottom);
        let total_w: f64 = labels.iter().map(entry_w).sum();
        let col_w = labels.iter().map(entry_w).fold(0.0, f64::max);
        let (lw, lh) = if horizontal { (total_w, row_h) } else { (col_w, row_h * labels.len() as f64) };
        let origin = match (legend.layout, legend.position) {
            (Some(m), _) => Point::new(m.x * w, m.y * h),
            (None, LegendPos::Bottom) => Point::new((w - lw) / 2.0, area.y1 - lh),
            (None, LegendPos::Top) => Point::new((w - lw) / 2.0, area.y0),
            (None, LegendPos::Left) => Point::new(area.x0, (area.y0 + area.y1 - lh) / 2.0),
            (None, LegendPos::Right | LegendPos::TopRight) => Point::new(area.x1 - lw, (area.y0 + area.y1 - lh) / 2.0),
        };
        if legend.layout.is_none() && !legend.overlay {
            match legend.position {
                LegendPos::Bottom => area.y1 -= lh + 4.0 * PT,
                LegendPos::Top => area.y0 += lh + 4.0 * PT,
                LegendPos::Left => area.x0 += lw + 4.0 * PT,
                LegendPos::Right | LegendPos::TopRight => area.x1 -= lw + 4.0 * PT,
            }
        }
        let mut at = origin;
        for (e, l) in entries.iter().zip(&labels) {
            let cy = at.y + row_h / 2.0;
            if e.2 {
                scene.stroke(&Stroke::new(2.0 * PT), tf, color(e.1), None, &Line::new((at.x, cy), (at.x + key, cy)));
            } else {
                scene.fill(Fill::NonZero, tf, color(e.1), None, &Rect::new(at.x, cy - key / 2.0, at.x + key, cy + key / 2.0));
            }
            place(scene, tf, l, Point::new(at.x + key + gap, cy), 0.0, 0.5);
            if horizontal {
                at.x += entry_w(l);
            } else {
                at.y += row_h;
            }
        }
    }

    // Value axes and their scales.
    let mut scales: Vec<(u32, Scale)> = Vec::new();
    for p in &chart.plots {
        let Some((_, val_id)) = p.axes else { continue };
        let (lo, hi) = plot_range(p);
        match scales.iter_mut().find(|s| s.0 == val_id) {
            Some(s) => {
                let merged = (s.1.min.min(lo), s.1.max.max(hi));
                s.1 = Scale { min: merged.0, max: merged.1, unit: 1.0 };
            }
            None => scales.push((val_id, Scale { min: lo, max: hi, unit: 1.0 })),
        }
    }
    let scales: Vec<(u32, Scale)> = scales.into_iter().map(|(id, s)| (id, auto_scale(s.min, s.max, chart.axis(id)))).collect();

    // Plot area: the manual layout, else what is left after room for axis labels.
    let value_label = |engine: &mut TextEngine, axis: &Axis, scale: &Scale, fmt: &str| -> Vec<(f64, Label)> {
        scale.ticks().into_iter().map(|v| (v, measure(engine, &format_number(v, axis.format.as_deref().unwrap_or(fmt)), &axis.style))).collect()
    };
    let first_format = chart.plots.iter().flat_map(|p| &p.series).next().map(|s| s.format.clone()).unwrap_or_else(|| "General".into());
    let mut axis_labels: Vec<(u32, Vec<(f64, Label)>)> = Vec::new();
    for (id, scale) in &scales {
        if let Some(axis) = chart.axis(*id).filter(|a| !a.deleted && a.labels) {
            let fmt = chart.plots.iter().find(|p| p.axes.map(|a| a.1) == Some(*id)).and_then(|p| p.series.first()).map_or(first_format.clone(), |s| s.format.clone());
            axis_labels.push((*id, value_label(engine, axis, scale, &fmt)));
        }
    }
    let categories = chart.categories();
    let cat_axis = chart.plots.iter().find_map(|p| p.axes).and_then(|a| chart.axis(a.0));
    let cat_style = cat_axis.map(|a| a.style.clone()).unwrap_or_else(|| chart.text.clone());
    let cat_labels: Vec<Label> = if cat_axis.is_some_and(|a| !a.deleted && a.labels) { categories.iter().map(|c| measure(engine, c, &cat_style)).collect() } else { Vec::new() };
    let side_width = |pos: AxisPos| {
        axis_labels.iter().filter(|(id, _)| chart.axis(*id).is_some_and(|a| a.position == pos)).flat_map(|(_, l)| l.iter().map(|x| x.1.w)).fold(0.0, f64::max)
    };
    let label_gap = 4.0 * PT;
    let cat_h = cat_labels.iter().map(|l| l.h).fold(0.0, f64::max);
    let plot = match chart.plot_layout {
        Some(m) => {
            let r = Rect::new(m.x * w, m.y * h, m.x * w + m.w.unwrap_or(1.0 - m.x) * w, m.y * h + m.h.unwrap_or(1.0 - m.y) * h);
            if chart.plot_layout_inner {
                r
            } else {
                Rect::new(r.x0 + side_width(AxisPos::Left) + label_gap, r.y0, r.x1 - side_width(AxisPos::Right) - label_gap, r.y1 - cat_h - label_gap)
            }
        }
        None => {
            let has_axes = chart.plots.iter().any(|p| p.axes.is_some());
            if has_axes {
                Rect::new(
                    area.x0 + side_width(AxisPos::Left) + label_gap,
                    area.y0 + cat_h * 0.5,
                    area.x1 - side_width(AxisPos::Right) - label_gap,
                    area.y1 - cat_h - label_gap,
                )
            } else {
                area
            }
        }
    };
    if let Some(fill) = &chart.plot_fill {
        fill_paint(scene, tf, fill, w, h, &plot);
    }

    let horizontal_bars = chart.plots.iter().any(|p| matches!(p.kind, PlotKind::Bar { horizontal: true, .. }));
    let n_cats = chart.plots.iter().flat_map(|p| &p.series).map(|s| s.values.len().max(s.categories.len())).max().unwrap_or(0).max(1);
    // Value → position along the value direction; category band → span along the other.
    let val_pos = |scale: &Scale, v: f64| if horizontal_bars { plot.x0 + scale.frac(v) * plot.width() } else { plot.y1 - scale.frac(v) * plot.height() };
    let band = if horizontal_bars { plot.height() / n_cats as f64 } else { plot.width() / n_cats as f64 };
    let band_start = |i: usize| if horizontal_bars { plot.y1 - (i + 1) as f64 * band } else { plot.x0 + i as f64 * band };

    // Gridlines and value labels.
    for (id, scale) in &scales {
        let Some(axis) = chart.axis(*id) else { continue };
        if let Some(grid) = axis.gridlines {
            for v in scale.ticks() {
                let p = val_pos(scale, v);
                let l = if horizontal_bars { Line::new((p, plot.y0), (p, plot.y1)) } else { Line::new((plot.x0, p), (plot.x1, p)) };
                scene.stroke(&Stroke::new(grid.width.max(1) as f64), tf, color(grid.color), None, &l);
            }
        }
        if let Some(line) = axis.line.filter(|_| !axis.deleted) {
            let x = if axis.position == AxisPos::Right { plot.x1 } else { plot.x0 };
            scene.stroke(&Stroke::new(line.width.max(1) as f64), tf, color(line.color), None, &Line::new((x, plot.y0), (x, plot.y1)));
        }
        if let Some((_, labels)) = axis_labels.iter().find(|(a, _)| a == id) {
            for (v, l) in labels {
                let p = val_pos(scale, *v);
                if horizontal_bars {
                    place(scene, tf, l, Point::new(p, plot.y1 + label_gap), 0.5, 0.0);
                } else if axis.position == AxisPos::Right {
                    place(scene, tf, l, Point::new(plot.x1 + label_gap, p), 0.0, 0.5);
                } else {
                    place(scene, tf, l, Point::new(plot.x0 - label_gap, p), 1.0, 0.5);
                }
            }
        }
    }

    // Category axis line and labels.
    if let Some(axis) = cat_axis.filter(|a| !a.deleted) {
        if let Some(line) = axis.line {
            let l = if horizontal_bars { Line::new((plot.x0, plot.y0), (plot.x0, plot.y1)) } else { Line::new((plot.x0, plot.y1), (plot.x1, plot.y1)) };
            scene.stroke(&Stroke::new(line.width.max(1) as f64), tf, color(line.color), None, &l);
        }
        let crowded = !horizontal_bars && cat_labels.iter().any(|l| l.w > band * 1.05);
        // Too many labels for the width: show every n-th one, rotated like Excel.
        let step = if crowded { ((cat_labels.iter().map(|l| l.h).fold(0.0, f64::max) * 1.6) / band).ceil().max(1.0) as usize } else { 1 };
        for (i, l) in cat_labels.iter().enumerate().step_by(step) {
            let c = band_start(i) + band / 2.0;
            if horizontal_bars {
                place(scene, tf, l, Point::new(plot.x0 - label_gap, c), 1.0, 0.5);
            } else if crowded {
                place_rotated(scene, tf, l, Point::new(c, plot.y1 + label_gap), 1.0, 0.5, -PI / 4.0);
            } else {
                place(scene, tf, l, Point::new(c, plot.y1 + label_gap), 0.5, 0.0);
            }
        }
    }

    // Plots: areas first, then bars, then lines on top.
    let order = |p: &&Plot| match p.kind {
        PlotKind::Area { .. } => 0,
        PlotKind::Pie { .. } | PlotKind::Bar { .. } => 1,
        PlotKind::Line { .. } => 2,
    };
    let mut plots: Vec<&Plot> = chart.plots.iter().collect();
    plots.sort_by_key(order);
    let mut pending_labels: Vec<(Label, Point, f64, f64)> = Vec::new();
    for p in plots {
        if let PlotKind::Pie { first_angle, hole, three_d } = p.kind {
            draw_pie(scene, engine, chart, p, plot, tf, first_angle, hole, three_d, &mut pending_labels, (w, h));
            continue;
        }
        let Some(scale) = p.axes.and_then(|(_, v)| scales.iter().find(|s| s.0 == v)).map(|s| s.1) else { continue };
        let zero = val_pos(&scale, 0.0f64.clamp(scale.min, scale.max));
        let n = p.series.len();
        let totals: Vec<f64> = (0..n_cats).map(|i| p.series.iter().map(|s| s.values.get(i).copied().flatten().unwrap_or(0.0).abs()).sum()).collect();
        let grouping = match p.kind {
            PlotKind::Bar { grouping, .. } | PlotKind::Line { grouping } | PlotKind::Area { grouping } => grouping,
            PlotKind::Pie { .. } => Grouping::Clustered,
        };
        // Stack bases per category: (positive, negative).
        let mut bases = vec![(0.0f64, 0.0f64); n_cats];
        let value_at = |bases: &mut Vec<(f64, f64)>, i: usize, v: f64| -> (f64, f64) {
            let v = if grouping == Grouping::PercentStacked && totals[i] != 0.0 { v / totals[i] } else { v };
            if !stacked(grouping) {
                return (0.0, v);
            }
            let b = &mut bases[i];
            if v >= 0.0 {
                let start = b.0;
                b.0 += v;
                (start, b.0)
            } else {
                let start = b.1;
                b.1 += v;
                (start, b.1)
            }
        };
        match p.kind {
            PlotKind::Bar { gap, overlap, .. } => {
                let (gap, ov) = (gap / 100.0, if stacked(grouping) { 1.0 } else { overlap / 100.0 });
                let slots = if stacked(grouping) { 1.0 } else { n as f64 - (n as f64 - 1.0) * ov };
                let bar = band / (slots + gap);
                for (si, s) in p.series.iter().enumerate() {
                    let fill = series_fill(chart, s);
                    for (i, &total) in totals.iter().enumerate() {
                        let Some(v) = s.values.get(i).copied().flatten() else { continue };
                        let (a, b) = value_at(&mut bases, i, v);
                        let offset = if stacked(grouping) { 0.0 } else { si as f64 * bar * (1.0 - ov) };
                        let start = band_start(i) + gap * bar / 2.0 + offset;
                        let (pa, pb) = (val_pos(&scale, a), val_pos(&scale, b));
                        let rect = if horizontal_bars {
                            Rect::new(pa.min(pb), start, pa.max(pb), start + bar)
                        } else {
                            Rect::new(start, pa.min(pb), start + bar, pa.max(pb))
                        };
                        let fill = s.point_fills.iter().find(|f| f.0 == i).map_or(fill.clone(), |f| f.1.clone());
                        fill_paint(scene, tf, &fill, w, h, &rect);
                        if let Some(line) = s.line.filter(|_| !s.no_line) {
                            stroke_line(scene, tf, &line, &rect.to_path(0.1));
                        }
                        if let Some(l) = labels_of(p, s)
                            && let Some(text) = label_text(l, s, i, v, total)
                        {
                            let label = measure(engine, &text, &l.style);
                            let pos = l.position.unwrap_or(if stacked(grouping) { LabelPosition::Center } else { LabelPosition::OutsideEnd });
                            let up = b >= a;
                            let (anchor, ay) = if horizontal_bars {
                                let x = match pos {
                                    LabelPosition::InsideEnd => pb - if up { 2.0 * PT } else { -2.0 * PT },
                                    LabelPosition::Center => (pa + pb) / 2.0,
                                    LabelPosition::InsideBase => pa + if up { 2.0 * PT } else { -2.0 * PT },
                                    _ => pb + if up { 2.0 * PT } else { -2.0 * PT },
                                };
                                let ax = match pos {
                                    LabelPosition::Center => 0.5,
                                    LabelPosition::InsideEnd => if up { 1.0 } else { 0.0 },
                                    LabelPosition::InsideBase => if up { 0.0 } else { 1.0 },
                                    _ => if up { 0.0 } else { 1.0 },
                                };
                                (Point::new(x, start + bar / 2.0), (ax, 0.5))
                            } else {
                                let (y, ay) = match pos {
                                    LabelPosition::InsideEnd => (pb + if up { 2.0 * PT } else { -2.0 * PT }, if up { 0.0 } else { 1.0 }),
                                    LabelPosition::Center => ((pa + pb) / 2.0, 0.5),
                                    LabelPosition::InsideBase => (pa - if up { 2.0 * PT } else { -2.0 * PT }, if up { 1.0 } else { 0.0 }),
                                    _ => (pb - if up { 2.0 * PT } else { -2.0 * PT }, if up { 1.0 } else { 0.0 }),
                                };
                                (Point::new(start + bar / 2.0, y), (0.5, ay))
                            };
                            pending_labels.push((label, anchor + offset_of(l, i, w, h), ay.0, ay.1));
                        }
                    }
                }
            }
            PlotKind::Line { .. } | PlotKind::Area { .. } => {
                let area_plot = matches!(p.kind, PlotKind::Area { .. });
                // Areas stack from the top series down so lower ones stay visible.
                for s in p.series.iter() {
                    let points: Vec<Option<(Point, f64, f64)>> = (0..n_cats)
                        .map(|i| {
                            let v = s.values.get(i).copied().flatten()?;
                            let (a, b) = value_at(&mut bases, i, v);
                            let x = band_start(i) + band / 2.0;
                            Some((Point::new(x, val_pos(&scale, b)), val_pos(&scale, a), v))
                        })
                        .collect();
                    if area_plot {
                        let mut path = BezPath::new();
                        let valid: Vec<&(Point, f64, f64)> = points.iter().flatten().collect();
                        if let (Some(first), Some(last)) = (valid.first(), valid.last()) {
                            path.move_to((first.0.x, if stacked(grouping) { first.1 } else { zero }));
                            for p in &valid {
                                path.line_to(p.0);
                            }
                            path.line_to((last.0.x, if stacked(grouping) { last.1 } else { zero }));
                            for p in valid.iter().rev().filter(|_| stacked(grouping)) {
                                path.line_to((p.0.x, p.1));
                            }
                            path.close_path();
                            fill_paint(scene, tf, &series_fill(chart, s), w, h, &path);
                        }
                    } else if let Some(line) = series_line(chart, s) {
                        let mut path = BezPath::new();
                        let mut pen = false;
                        for p in &points {
                            match p {
                                Some((pt, _, _)) if pen => path.line_to(*pt),
                                Some((pt, _, _)) => {
                                    path.move_to(*pt);
                                    pen = true;
                                }
                                None => pen = false,
                            }
                        }
                        stroke_line(scene, tf, &line, &path);
                    }
                    if !area_plot {
                        let marker = s.marker.unwrap_or(ppt_model::chart::Marker { symbol: MarkerSymbol::None, size: 5.0, fill: None, line: None });
                        let auto = series_line(chart, s).map(|l| l.color).unwrap_or(chart.auto_color(s.index));
                        for (pt, _, _) in points.iter().flatten() {
                            draw_marker(scene, tf, &marker, *pt, marker.fill.unwrap_or(auto), marker.line.unwrap_or(auto));
                        }
                    }
                    if let Some(l) = labels_of(p, s) {
                        for (i, p) in points.iter().enumerate() {
                            let Some((pt, _, v)) = p else { continue };
                            let Some(text) = label_text(l, s, i, *v, totals[i]) else { continue };
                            let label = measure(engine, &text, &l.style);
                            let (anchor, ax, ay) = match l.position.unwrap_or(LabelPosition::Right) {
                                LabelPosition::Above => (*pt - Vec2::new(0.0, 3.0 * PT), 0.5, 1.0),
                                LabelPosition::Below => (*pt + Vec2::new(0.0, 3.0 * PT), 0.5, 0.0),
                                LabelPosition::Left => (*pt - Vec2::new(4.0 * PT, 0.0), 1.0, 0.5),
                                LabelPosition::Center => (*pt, 0.5, 0.5),
                                _ => (*pt + Vec2::new(4.0 * PT, 0.0), 0.0, 0.5),
                            };
                            pending_labels.push((label, anchor + offset_of(l, i, w, h), ax, ay));
                        }
                    }
                }
            }
            PlotKind::Pie { .. } => {}
        }
    }
    // Data labels on top of everything.
    for (label, at, ax, ay) in &pending_labels {
        place(scene, tf, label, *at, *ax, *ay);
    }
    scene.pop_layer();
}

fn point_color(chart: &Chart, s: &Series, i: usize) -> Color {
    s.point_fills.iter().find(|f| f.0 == i).map(|f| f.1.color()).unwrap_or_else(|| chart.auto_color(i))
}

fn draw_marker(scene: &mut dyn Canvas, tf: Affine, m: &ppt_model::chart::Marker, at: Point, fill: Color, line: Color) {
    let r = m.size * PT / 2.0;
    let path = match m.symbol {
        MarkerSymbol::None => return,
        MarkerSymbol::Circle => Circle::new(at, r).to_path(0.1),
        MarkerSymbol::Square => Rect::new(at.x - r, at.y - r, at.x + r, at.y + r).to_path(0.1),
        MarkerSymbol::Diamond => {
            let mut p = BezPath::new();
            p.move_to((at.x, at.y - r));
            p.line_to((at.x + r, at.y));
            p.line_to((at.x, at.y + r));
            p.line_to((at.x - r, at.y));
            p.close_path();
            p
        }
        MarkerSymbol::Triangle => {
            let mut p = BezPath::new();
            p.move_to((at.x, at.y - r));
            p.line_to((at.x + r, at.y + r));
            p.line_to((at.x - r, at.y + r));
            p.close_path();
            p
        }
    };
    scene.fill(Fill::NonZero, tf, color(fill), None, &path);
    scene.stroke(&Stroke::new(0.75 * PT), tf, color(line), None, &path);
}

/// Pie and doughnut charts; 3D pies are drawn as a tilted disc with a side.
#[allow(clippy::too_many_arguments)]
fn draw_pie(
    scene: &mut dyn Canvas,
    engine: &mut TextEngine,
    chart: &Chart,
    p: &Plot,
    plot: Rect,
    tf: Affine,
    first_angle: f64,
    hole: f64,
    three_d: bool,
    labels: &mut Vec<(Label, Point, f64, f64)>,
    (cw, ch): (f64, f64),
) {
    let Some(s) = p.series.first() else { return };
    let total: f64 = s.values.iter().flatten().filter(|v| **v > 0.0).sum();
    if total <= 0.0 {
        return;
    }
    let tilt = if three_d { 0.55 } else { 1.0 };
    let depth = if three_d { plot.width().min(plot.height() / tilt) * 0.08 } else { 0.0 };
    let r = (plot.width() / 2.0).min((plot.height() - depth) / 2.0 / tilt) * 0.95;
    let c = Point::new(plot.center().x, plot.y0 + (plot.height() - depth) / 2.0);
    let squash = Affine::translate(c.to_vec2()) * Affine::scale_non_uniform(1.0, tilt);
    let slice = |a0: f64, a1: f64, inner: f64| {
        let mut path = BezPath::new();
        let arc = vello::kurbo::Arc::new(Point::ZERO, Vec2::new(r, r), a0, a1 - a0, 0.0);
        if inner > 0.0 {
            path.move_to(Point::new(inner * a0.cos(), inner * a0.sin()));
            path.line_to(Point::new(r * a0.cos(), r * a0.sin()));
            path.extend(arc.append_iter(0.1));
            path.line_to(Point::new(inner * a1.cos(), inner * a1.sin()));
            let back = vello::kurbo::Arc::new(Point::ZERO, Vec2::new(inner, inner), a1, a0 - a1, 0.0);
            path.extend(back.append_iter(0.1));
        } else {
            path.move_to(Point::ZERO);
            path.line_to(Point::new(r * a0.cos(), r * a0.sin()));
            path.extend(arc.append_iter(0.1));
        }
        path.close_path();
        path
    };
    // Angles: clockwise from 12 o'clock.
    let mut angle = (first_angle - 90.0).to_radians();
    let mut slices = Vec::new();
    for (i, v) in s.values.iter().enumerate() {
        let Some(v) = v.filter(|v| *v > 0.0) else { continue };
        let sweep = v / total * 2.0 * PI;
        slices.push((i, v, angle, angle + sweep));
        angle += sweep;
    }
    if three_d {
        // The visible side: the lower half of the rim, darker.
        for &(i, _, a0, a1) in &slices {
            let col = point_color(chart, s, i);
            let dark = Color { r: (col.r as f64 * 0.7) as u8, g: (col.g as f64 * 0.7) as u8, b: (col.b as f64 * 0.7) as u8, a: col.a };
            let side = slice(a0, a1, 0.0);
            scene.fill(Fill::NonZero, tf * Affine::translate((0.0, depth)) * squash, color(dark), None, &side);
        }
    }
    let border = s.line.filter(|_| !s.no_line);
    for &(i, v, a0, a1) in &slices {
        let path = slice(a0, a1, r * hole);
        let fill = s.point_fills.iter().find(|f| f.0 == i).map(|f| f.1.clone()).unwrap_or(Paint::Solid(chart.auto_color(i)));
        let bbox = path.bounding_box();
        fill_paint(scene, tf * squash, &fill, bbox.width().max(1.0), bbox.height().max(1.0), &path);
        if let Some(line) = border {
            scene.stroke(&Stroke::new(line.width.max(1) as f64), tf * squash, color(line.color), None, &path);
        }
        if let Some(l) = labels_of(p, s)
            && let Some(text) = label_text(l, s, i, v, total)
        {
            let label = measure(engine, &text, &l.style);
            let mid = (a0 + a1) / 2.0;
            let outside = matches!(l.position, Some(LabelPosition::OutsideEnd));
            let rr = if outside { r * 1.12 } else if hole > 0.0 { r * (1.0 + hole) / 2.0 } else { r * 0.65 };
            let at = squash * Point::new(rr * mid.cos(), rr * mid.sin());
            let (ax, ay) = if outside { (if mid.cos() >= 0.0 { 0.0 } else { 1.0 }, 0.5) } else { (0.5, 0.5) };
            labels.push((label, at + offset_of(l, i, cw, ch), ax, ay));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_like_scales() {
        let s = auto_scale(0.0, 399.0, None);
        assert_eq!((s.min, s.max, s.unit), (0.0, 450.0, 50.0));
        let s = auto_scale(0.0, 1060.0, None);
        assert_eq!((s.min, s.max, s.unit), (0.0, 1200.0, 200.0));
        let s = auto_scale(-0.25, 0.56, None);
        assert!(s.min < -0.25 && s.max > 0.56 && s.unit > 0.0, "{s:?}");
        assert_eq!(s.ticks().len(), ((s.max - s.min) / s.unit).round() as usize + 1);
    }
}
