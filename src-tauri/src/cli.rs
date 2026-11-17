//! 命令列模式：不開視窗，直接處理檔案後結束。給外部工具（例如產生簡報的 skill）驗證用。
//!
//! `ww-ppt --export-pdf <in.pptx> <out.pdf>`：讀入簡報並匯出 PDF，
//! 和「檔案 > 匯出為 PDF」用同一套繪圖程式（`ppt-pdf`），只是在原生端執行。
//! 讀不了的檔案會印出原因並以結束碼 1 結束。

use std::process::ExitCode;

use ppt_offscreen::SlideRenderer;

const USAGE: &str = "usage: ww-ppt --export-pdf <in.pptx> <out.pdf>";

/// 命令列有指定模式時執行並回傳結束碼；沒有的話回傳 `None`，照常啟動 App。
pub fn run_cli() -> Option<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--export-pdf") => export_pdf(&args[1..]),
        Some("--help") => Ok(USAGE.to_string()),
        _ => return None,
    };
    Some(match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    })
}

fn export_pdf(args: &[String]) -> Result<String, String> {
    let [input, output] = args else { return Err(USAGE.to_string()) };
    let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
    let pres = ppt_ooxml::parse_pptx_owned(bytes).map_err(|e| format!("{input}: {e}"))?;
    let mut renderer = SlideRenderer::new()?;
    renderer.ensure_fonts(pres.font_families());
    let pdf = renderer.export_pdf(&pres)?;
    std::fs::write(output, &pdf).map_err(|e| format!("{output}: {e}"))?;
    Ok(format!("{} slides -> {output}", pres.slides.len()))
}
