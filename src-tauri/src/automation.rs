//! Dev-only automation channel for driving the real webview from scripts.
//!
//! Enabled by `WW_PPT_AUTOMATION=<dir>`. The frontend polls `<dir>/commands.txt`
//! for one command per line, appends results to `<dir>/log.txt` and writes
//! canvas captures as `<dir>/<name>.png`.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use tauri::ipc::{InvokeBody, Request};

fn dir() -> Option<PathBuf> {
    std::env::var_os("WW_PPT_AUTOMATION").map(PathBuf::from)
}

#[tauri::command]
pub fn automation_enabled() -> bool {
    dir().is_some()
}

/// Returns pending commands and clears the queue.
#[tauri::command]
pub fn automation_poll() -> Vec<String> {
    let Some(dir) = dir() else { return Vec::new() };
    let path = dir.join("commands.txt");
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    if text.is_empty() {
        return Vec::new();
    }
    let _ = std::fs::write(&path, "");
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

#[tauri::command]
pub fn automation_log(line: String) -> Result<(), String> {
    let Some(dir) = dir() else { return Ok(()) };
    let mut f = OpenOptions::new().create(true).append(true).open(dir.join("log.txt")).map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

/// Saves raw bytes (a PNG) named by the `name` header.
#[tauri::command]
pub fn automation_write(request: Request<'_>) -> Result<(), String> {
    let Some(dir) = dir() else { return Ok(()) };
    let name = request.headers().get("name").and_then(|v| v.to_str().ok()).unwrap_or("capture");
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    std::fs::write(dir.join(format!("{name}.png")), bytes).map_err(|e| e.to_string())
}
