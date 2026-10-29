//! Native side of ww-ppt: file and font access plus macOS integration.
//! The editor itself runs in the webview (see `crates/ppt-wasm`).

mod automation;

use ppt_fonts::{FontService, ResolvedFont};
use tauri::ipc::Response;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{Emitter, Manager, State};

/// Returns file contents as raw bytes (no JSON/base64 round trip).
#[tauri::command]
fn read_file(path: String) -> Result<Response, String> {
    std::fs::read(&path).map(Response::new).map_err(|e| format!("{path}: {e}"))
}

/// Writes raw bytes (the request body) to the path in the `path` header.
#[tauri::command]
fn write_file(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let path = request.headers().get("path").and_then(|v| v.to_str().ok()).ok_or("missing path header")?;
    let path = percent_decode(path);
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    // Write to a temporary file first so a failed save never truncates the original.
    let tmp = format!("{path}.ww-saving");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{path}: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{path}: {e}"))
}

/// Header values are ASCII; the frontend percent-encodes paths.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tauri::command]
fn list_fonts(fonts: State<'_, FontService>) -> Vec<String> {
    fonts.families()
}

#[tauri::command]
fn resolve_font(fonts: State<'_, FontService>, family: String) -> Option<ResolvedFont> {
    fonts.resolve(&family)
}

/// Paths passed on the command line (e.g. `open -a ww-ppt deck.pptx` in dev).
#[tauri::command]
fn startup_files() -> Vec<String> {
    std::env::args().skip(1).filter(|a| a.to_lowercase().ends_with(".pptx")).collect()
}

fn build_menu(app: &tauri::App) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let item = |id: &str, text: &str, accel: Option<&str>| {
        let b = MenuItemBuilder::with_id(id, text);
        match accel {
            Some(a) => b.accelerator(a).build(app),
            None => b.build(app),
        }
    };
    let app_menu = SubmenuBuilder::new(app, "ww-ppt")
        .item(&PredefinedMenuItem::about(app, Some("關於 ww-ppt"), None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("隱藏 ww-ppt"))?)
        .item(&PredefinedMenuItem::hide_others(app, Some("隱藏其他"))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("結束 ww-ppt"))?)
        .build()?;
    let file = SubmenuBuilder::new(app, "檔案")
        .item(&item("new", "新增簡報", Some("CmdOrCtrl+N"))?)
        .item(&item("open", "開啟…", Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&item("save", "儲存", Some("CmdOrCtrl+S"))?)
        .item(&item("save_as", "另存新檔…", Some("CmdOrCtrl+Shift+S"))?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, Some("關閉視窗"))?)
        .build()?;
    // Undo/redo/select-all go to the editor core instead of WebKit's own handlers.
    // Cut/copy/paste stay predefined so the webview receives clipboard events.
    let edit = SubmenuBuilder::new(app, "編輯")
        .item(&item("undo", "復原", Some("CmdOrCtrl+Z"))?)
        .item(&item("redo", "取消復原", Some("CmdOrCtrl+Shift+Z"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some("剪下"))?)
        .item(&PredefinedMenuItem::copy(app, Some("拷貝"))?)
        .item(&PredefinedMenuItem::paste(app, Some("貼上"))?)
        .item(&item("select_all", "全選", Some("CmdOrCtrl+A"))?)
        .build()?;
    let insert = SubmenuBuilder::new(app, "插入")
        .item(&item("new_slide", "新增投影片", Some("CmdOrCtrl+Shift+N"))?)
        .item(&item("duplicate_slide", "複製投影片", None)?)
        .separator()
        .item(&item("insert_picture", "圖片…", None)?)
        .item(&item("insert_text_box", "文字方塊", None)?)
        .item(&item("insert_rect", "矩形", None)?)
        .item(&item("insert_ellipse", "橢圓", None)?)
        .separator()
        .item(&item("insert_link", "超連結…", Some("CmdOrCtrl+K"))?)
        .build()?;
    let arrange = SubmenuBuilder::new(app, "排列")
        .item(&item("group", "組成群組", Some("CmdOrCtrl+Alt+G"))?)
        .item(&item("ungroup", "取消群組", Some("CmdOrCtrl+Alt+Shift+G"))?)
        .build()?;
    let view = SubmenuBuilder::new(app, "檢視")
        .item(&item("zoom_in", "放大", Some("CmdOrCtrl+="))?)
        .item(&item("zoom_out", "縮小", Some("CmdOrCtrl+-"))?)
        .item(&item("zoom_fit", "符合視窗大小", Some("CmdOrCtrl+0"))?)
        .separator()
        .item(&item("toggle_notes", "備忘稿", None)?)
        .build()?;
    let show = SubmenuBuilder::new(app, "投影片放映")
        .item(&item("show_from_beginning", "從頭播放", Some("CmdOrCtrl+Shift+Enter"))?)
        .item(&item("show_from_current", "從目前的投影片播放", Some("CmdOrCtrl+Enter"))?)
        .build()?;
    let window = SubmenuBuilder::new(app, "視窗")
        .item(&PredefinedMenuItem::minimize(app, Some("縮到最小"))?)
        .item(&PredefinedMenuItem::fullscreen(app, Some("進入全螢幕"))?)
        .build()?;
    MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &insert, &arrange, &view, &show, &window]).build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(FontService::new())
        .setup(|app| {
            let menu = build_menu(app)?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                // The frontend owns all editing commands; forward the menu id.
                let _ = app.emit("menu", event.id().0.as_str());
            });
            if let Some(window) = app.get_webview_window("main") {
                window.set_focus()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_file,
            write_file,
            list_fonts,
            resolve_font,
            startup_files,
            automation::automation_enabled,
            automation::automation_poll,
            automation::automation_log,
            automation::automation_write
        ])
        .run(tauri::generate_context!())
        .expect("error while running ww-ppt");
}
