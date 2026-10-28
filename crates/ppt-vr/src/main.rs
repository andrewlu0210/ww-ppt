//! Visual regression runner.
//!
//! For every `<name>.pptx` with a `<name>.pdf` next to it in the samples
//! directory (the PDF exported from PowerPoint is the reference), each slide is
//! rendered and compared with the matching PDF page. The deck is also saved and
//! reopened, and the resaved slides must render the same as the original.
//!
//! ```sh
//! cargo run -p ppt-vr --release                 # compare with samples/vr/baseline.json
//! cargo run -p ppt-vr --release -- --update     # accept the current scores as the baseline
//! ```
//!
//! Output goes to `target/vr/<name>/`: `page-NN.png` shows reference | ours |
//! mismatches, and `report.md` lists the scores. Needs `pdftoppm` (poppler).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use image::RgbaImage;
use ppt_vr::{SlideRenderer, compare, side_by_side};

/// A page may score this many points (percent of mismatched pixels) worse than its baseline.
const TOLERANCE: f64 = 0.5;
/// Resaved slides must render (nearly) identically to the original.
const ROUND_TRIP_LIMIT: f64 = 0.05;

struct Args {
    samples: PathBuf,
    out: PathBuf,
    width: u32,
    update: bool,
    only: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("workspace root").to_path_buf();
    let mut args = Args { samples: root.join("samples/vr"), out: root.join("target/vr"), width: 1280, update: false, only: None };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--samples" => args.samples = value()?.into(),
            "--out" => args.out = value()?.into(),
            "--width" => args.width = value()?.parse().map_err(|e| format!("--width: {e}"))?,
            "--only" => args.only = Some(value()?),
            "--update" => args.update = true,
            _ => return Err(format!("unknown argument {a}")),
        }
    }
    Ok(args)
}

/// Baseline scores per deck, one per page.
type Baseline = BTreeMap<String, Vec<f64>>;

/// Rasterizes the reference PDF to `width`x`height` pages (cached by modification time).
fn reference_pages(pdf: &Path, dir: &Path, width: u32, height: u32) -> Result<Vec<RgbaImage>, String> {
    let stamp = dir.join(format!("ref-{width}.stamp"));
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if modified(&stamp) < modified(pdf) || modified(&stamp).is_none() {
        let status = Command::new("pdftoppm")
            .args(["-png", "-scale-to-x", &width.to_string(), "-scale-to-y", &height.to_string()])
            .arg(pdf)
            .arg(dir.join(format!("ref-{width}")))
            .status()
            .map_err(|e| format!("pdftoppm: {e} (install poppler)"))?;
        if !status.success() {
            return Err(format!("pdftoppm failed on {}", pdf.display()));
        }
        std::fs::write(&stamp, "").map_err(|e| e.to_string())?;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&format!("ref-{width}-")) && n.ends_with(".png")))
        .collect();
    // pdftoppm pads page numbers to the page count's width, so lexical order is page order.
    files.sort();
    files.iter().map(|p| image::open(p).map(|i| i.to_rgba8()).map_err(|e| format!("{}: {e}", p.display()))).collect()
}

struct PageResult {
    score: f64,
    round_trip: f64,
    baseline: Option<f64>,
}

fn run_deck(r: &mut SlideRenderer, pptx: &Path, pdf: &Path, dir: &Path, width: u32, baseline: Option<&Vec<f64>>) -> Result<Vec<PageResult>, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(pptx).map_err(|e| e.to_string())?;
    let pres = ppt_ooxml::parse_pptx(&bytes).map_err(|e| e.to_string())?;
    let resaved = ppt_ooxml::save_pptx(&pres).map_err(|e| format!("save: {e}"))?;
    let reopened = ppt_ooxml::parse_pptx(&resaved).map_err(|e| format!("reopen: {e}"))?;
    if reopened.slides.len() != pres.slides.len() {
        return Err(format!("resaved deck has {} slides, expected {}", reopened.slides.len(), pres.slides.len()));
    }
    r.ensure_fonts(pres.font_families());

    let height = ((width as f64) * pres.slide_height as f64 / pres.slide_width as f64).round() as u32;
    let refs = reference_pages(pdf, dir, width, height)?;
    if refs.len() != pres.slides.len() {
        return Err(format!("{} has {} pages but the deck has {} slides", pdf.display(), refs.len(), pres.slides.len()));
    }

    let mut results = Vec::new();
    for (i, (slide, expected)) in pres.slides.iter().zip(&refs).enumerate() {
        let ours = r.render(&pres, slide, width)?;
        if ours.dimensions() != expected.dimensions() {
            return Err(format!("page {}: reference is {:?}, ours {:?}", i + 1, expected.dimensions(), ours.dimensions()));
        }
        let (diff, vis) = compare(expected, &ours);
        let again = r.render(&reopened, &reopened.slides[i], width)?;
        let (rt, _) = compare(&ours, &again);
        side_by_side(&[expected, &ours, &vis]).save(dir.join(format!("page-{:02}.png", i + 1))).map_err(|e| e.to_string())?;
        results.push(PageResult { score: diff.score, round_trip: rt.score, baseline: baseline.and_then(|b| b.get(i).copied()) });
    }
    Ok(results)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let baseline_path = args.samples.join("baseline.json");
    let mut baseline: Baseline = std::fs::read_to_string(&baseline_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();

    let mut decks: Vec<(String, PathBuf, PathBuf)> = std::fs::read_dir(&args.samples)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pptx"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            let pdf = p.with_extension("pdf");
            pdf.exists().then_some((name, p, pdf))
        })
        .filter(|(name, ..)| args.only.as_ref().is_none_or(|o| o == name))
        .collect();
    decks.sort();
    if decks.is_empty() {
        eprintln!("no <name>.pptx + <name>.pdf pairs in {}", args.samples.display());
        return ExitCode::FAILURE;
    }

    let mut r = match SlideRenderer::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("renderer: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut failed = false;
    for (name, pptx, pdf) in decks {
        let dir = args.out.join(&name);
        let results = match run_deck(&mut r, &pptx, &pdf, &dir, args.width, baseline.get(&name)) {
            Ok(res) => res,
            Err(e) => {
                println!("{name}: ERROR {e}");
                failed = true;
                continue;
            }
        };
        let mut report = format!("# {name}\n\n| 頁 | 差異 % | 基準 % | 存檔後差異 % | 結果 |\n|---|---|---|---|---|\n");
        println!("{name}:");
        for (i, p) in results.iter().enumerate() {
            let verdict = if p.round_trip > ROUND_TRIP_LIMIT {
                failed = true;
                "存檔後畫面改變"
            } else if p.baseline.is_some_and(|b| p.score > b + TOLERANCE) && !args.update {
                failed = true;
                "退步"
            } else if p.baseline.is_some_and(|b| p.score < b - TOLERANCE) {
                "進步"
            } else {
                "ok"
            };
            let base = p.baseline.map_or("-".to_string(), |b| format!("{b:.2}"));
            println!("  page {:2}  diff {:6.2}%  baseline {:>6}  round-trip {:5.2}%  {verdict}", i + 1, p.score, base, p.round_trip);
            report += &format!("| {} | {:.2} | {base} | {:.2} | {verdict} |\n", i + 1, p.score, p.round_trip);
        }
        let mean = results.iter().map(|p| p.score).sum::<f64>() / results.len() as f64;
        println!("  mean diff {mean:.2}%  ({})", dir.display());
        report += &format!("\n平均差異 {mean:.2}%\n");
        let _ = std::fs::write(dir.join("report.md"), report);
        if args.update {
            baseline.insert(name, results.iter().map(|p| (p.score * 100.0).round() / 100.0).collect());
        }
    }
    if args.update {
        match serde_json::to_string_pretty(&baseline) {
            Ok(json) if std::fs::write(&baseline_path, format!("{json}\n")).is_ok() => println!("baseline written to {}", baseline_path.display()),
            _ => {
                eprintln!("could not write {}", baseline_path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
