// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> std::process::ExitCode {
    // 命令列模式（例如 --export-pdf）不開視窗，處理完就結束
    if let Some(code) = ww_ppt_lib::run_cli() {
        return code;
    }
    ww_ppt_lib::run();
    std::process::ExitCode::SUCCESS
}
