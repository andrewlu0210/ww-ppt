//! Charts (`c:chartSpace` parts), drawn from the values cached in the chart
//! XML. Charts are display-only: their parts are saved unchanged.

use crate::{Color, Line, Paint, RunStyle};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chart {
    pub title: Option<ChartTitle>,
    pub plots: Vec<Plot>,
    pub axes: Vec<Axis>,
    pub legend: Option<Legend>,
    /// Plot area position as fractions of the chart (`c:manualLayout`), when set.
    pub plot_layout: Option<ManualLayout>,
    /// Whether the manual plot layout is the inner (axes excluded) rectangle.
    pub plot_layout_inner: bool,
    pub fill: Option<Paint>,
    pub border: Option<Line>,
    pub plot_fill: Option<Paint>,
    /// Default text style (`c:chartSpace/c:txPr`).
    pub text: RunStyle,
    /// Automatic series / point colors (theme accents, then shades of them).
    pub palette: Vec<Color>,
}

/// Position as fractions of the chart frame (x, y, w, h).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManualLayout {
    pub x: f64,
    pub y: f64,
    pub w: Option<f64>,
    pub h: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartTitle {
    pub text: String,
    pub style: RunStyle,
    pub layout: Option<ManualLayout>,
    pub overlay: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    pub kind: PlotKind,
    pub series: Vec<Series>,
    /// Axis ids (category, value) for plots with axes.
    pub axes: Option<(u32, u32)>,
    /// Data labels set on the whole group.
    pub labels: Option<DataLabels>,
    /// Each point gets its own color (pie charts, or `varyColors`).
    pub vary_colors: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Grouping {
    Clustered,
    Stacked,
    PercentStacked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlotKind {
    /// Columns (`horizontal == false`) or bars. `gap` and `overlap` are percentages.
    Bar { horizontal: bool, grouping: Grouping, gap: f64, overlap: f64 },
    Line { grouping: Grouping },
    Area { grouping: Grouping },
    /// Pie or doughnut (`hole` > 0, a fraction of the radius).
    Pie { first_angle: f64, hole: f64, three_d: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// `c:idx`: picks the automatic color.
    pub index: usize,
    pub name: String,
    pub categories: Vec<String>,
    pub values: Vec<Option<f64>>,
    pub format: String,
    pub fill: Option<Paint>,
    pub line: Option<Line>,
    /// `true` when the series sets its line to none explicitly.
    pub no_line: bool,
    pub marker: Option<Marker>,
    /// Per-point fill overrides (`c:dPt`), by point index.
    pub point_fills: Vec<(usize, Paint)>,
    pub labels: Option<DataLabels>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub symbol: MarkerSymbol,
    /// Size in points.
    pub size: f64,
    pub fill: Option<Color>,
    pub line: Option<Color>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkerSymbol {
    None,
    Circle,
    Square,
    Diamond,
    Triangle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LabelPosition {
    OutsideEnd,
    InsideEnd,
    Center,
    InsideBase,
    Above,
    Below,
    Left,
    Right,
    BestFit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataLabels {
    pub show_value: bool,
    pub show_percent: bool,
    pub show_category: bool,
    pub show_series: bool,
    pub position: Option<LabelPosition>,
    pub format: Option<String>,
    pub style: RunStyle,
    /// Points whose label is deleted, and per-point offsets (fractions of the chart).
    pub deleted: Vec<usize>,
    pub offsets: Vec<(usize, f64, f64)>,
    /// Labels whose text was typed in (`c:dLbl/c:tx/c:rich`), by point index.
    pub texts: Vec<(usize, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisPos {
    Bottom,
    Left,
    Right,
    Top,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub id: u32,
    /// Category (`c:catAx`/`c:dateAx`) or value (`c:valAx`) axis.
    pub category: bool,
    pub position: AxisPos,
    pub deleted: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major_unit: Option<f64>,
    pub reversed: bool,
    pub format: Option<String>,
    pub gridlines: Option<Line>,
    pub line: Option<Line>,
    pub labels: bool,
    pub style: RunStyle,
    pub cross_axis: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendPos {
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Legend {
    pub position: LegendPos,
    pub layout: Option<ManualLayout>,
    pub overlay: bool,
    pub style: RunStyle,
    /// Entries hidden with `c:legendEntry/c:delete`.
    pub deleted: Vec<usize>,
}

impl Chart {
    /// The automatic color for series (or point) number `i`.
    pub fn auto_color(&self, i: usize) -> Color {
        self.palette.get(i % self.palette.len().max(1)).copied().unwrap_or(Color::rgb(0x44, 0x72, 0xC4))
    }

    pub fn axis(&self, id: u32) -> Option<&Axis> {
        self.axes.iter().find(|a| a.id == id)
    }

    /// Font families used by the chart's text.
    pub fn font_families(&self) -> Vec<String> {
        let mut styles: Vec<&RunStyle> = vec![&self.text];
        styles.extend(self.title.iter().map(|t| &t.style));
        styles.extend(self.legend.iter().map(|l| &l.style));
        styles.extend(self.axes.iter().map(|a| &a.style));
        for p in &self.plots {
            styles.extend(p.labels.iter().map(|l| &l.style));
            styles.extend(p.series.iter().flat_map(|s| s.labels.iter().map(|l| &l.style)));
        }
        styles.iter().flat_map(|s| [s.latin_font.clone(), s.ea_font.clone()]).collect()
    }

    /// Categories of the chart (from the first series that has any).
    pub fn categories(&self) -> Vec<String> {
        self.plots.iter().flat_map(|p| &p.series).find(|s| !s.categories.is_empty()).map(|s| s.categories.clone()).unwrap_or_default()
    }
}

/// Formats a number with an Excel-style format code (the common subset:
/// `General`, `0`, `0.00`, `#,##0`, `0%`, `0.0%`, literal suffixes).
pub fn format_number(v: f64, format: &str) -> String {
    let format = format.split(';').next().unwrap_or("General").trim();
    if format.is_empty() || format.eq_ignore_ascii_case("general") {
        return general(v);
    }
    let percent = format.contains('%');
    let v = if percent { v * 100.0 } else { v };
    // Digits after the decimal point in the numeric part.
    let numeric: String = format.chars().filter(|c| matches!(c, '0' | '#' | '.' | ',')).collect();
    let decimals = numeric.split_once('.').map_or(0, |(_, d)| d.chars().filter(|c| *c == '0' || *c == '#').count());
    let grouped = numeric.contains(',') && numeric.split_once('.').map_or(numeric.as_str(), |(i, _)| i).contains(',');
    let mut s = format!("{:.*}", decimals, v.abs());
    if grouped {
        let (int, frac) = s.split_once('.').map_or((s.clone(), None), |(i, f)| (i.to_string(), Some(f.to_string())));
        let mut out = String::new();
        for (i, ch) in int.chars().enumerate() {
            if i > 0 && (int.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(ch);
        }
        s = match frac {
            Some(f) => format!("{out}.{f}"),
            None => out,
        };
    }
    if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') {
        s.insert(0, '-');
    }
    // Literal text around the number ("0"万, 0.0"%", ...).
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut seen_digit = false;
    let mut in_quote = false;
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quote = !in_quote,
            '0' | '#' | '.' | ',' if !in_quote => seen_digit = true,
            '%' if !in_quote => suffix.push('%'),
            // `_x` pads with the width of x, `*x` repeats x: neither prints x.
            '_' | '*' if !in_quote => {
                chars.next();
            }
            '\\' if !in_quote => {}
            c => {
                if seen_digit {
                    suffix.push(c);
                } else {
                    prefix.push(c);
                }
            }
        }
    }
    format!("{prefix}{s}{suffix}")
}

/// Excel's "General": up to ~10 significant digits, no trailing zeros.
fn general(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.*}", (9 - v.abs().log10().floor() as i32).clamp(0, 9) as usize, v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(format_number(1234.5, "General"), "1234.5");
        assert_eq!(format_number(0.1 + 0.2, "General"), "0.3");
        assert_eq!(format_number(1234.567, "#,##0"), "1,235");
        assert_eq!(format_number(1234567.0, "#,##0.00"), "1,234,567.00");
        assert_eq!(format_number(0.177, "0.0%"), "17.7%");
        assert_eq!(format_number(0.5, "0%"), "50%");
        assert_eq!(format_number(-0.043, "0.0%"), "-4.3%");
        assert_eq!(format_number(3.0, "0\"万\""), "3万");
        assert_eq!(format_number(0.0, "0.0"), "0.0");
        assert_eq!(format_number(40415.0, "#,##0_);(#,##0)"), "40,415");
    }
}
