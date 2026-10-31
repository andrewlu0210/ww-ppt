//! Native side of Want-Point: file and font access plus macOS integration.
//! The editor itself runs in the webview (see `crates/ppt-wasm`).

mod automation;
mod recovery;
mod spell;
mod video;

use ppt_fonts::{FontService, ResolvedFont};
use tauri::ipc::Response;
use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use std::sync::Mutex;
use tauri::{Emitter, Manager, RunEvent, State};

/// Returns file contents as raw bytes (no JSON/base64 round trip).
#[tauri::command]
fn read_file(path: String) -> Result<Response, String> {
    std::fs::read(&path).map(Response::new).map_err(|e| format!("{path}: {e}"))
}

/// A font file for the editor: large collections come trimmed to the weights
/// slides use (see `ppt_fonts::trim_collection`).
#[tauri::command]
fn read_font(path: String) -> Result<Response, String> {
    let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    Ok(Response::new(ppt_fonts::trim_collection(&data).unwrap_or(data)))
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
pub(crate) fn percent_decode(s: &str) -> String {
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

/// The deck being presented, handed from the main window to the audience window.
#[derive(Default)]
struct ShowDeck(std::sync::Mutex<Vec<u8>>);

#[tauri::command]
fn set_show_deck(deck: State<'_, ShowDeck>, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    *deck.0.lock().map_err(|e| e.to_string())? = bytes.clone();
    Ok(())
}

#[tauri::command]
fn get_show_deck(deck: State<'_, ShowDeck>) -> Result<Response, String> {
    Ok(Response::new(deck.0.lock().map_err(|e| e.to_string())?.clone()))
}

#[tauri::command]
fn list_fonts(fonts: State<'_, FontService>) -> Vec<String> {
    fonts.families()
}

#[tauri::command]
fn resolve_font(fonts: State<'_, FontService>, family: String) -> Option<ResolvedFont> {
    fonts.resolve(&family)
}

/// Files macOS asked us to open (Finder double-click, Open With, dropping on
/// the Dock icon). Held until the frontend is ready; after that each request
/// is sent as an `open-files` event.
#[derive(Default)]
struct OpenRequests {
    ready: bool,
    pending: Vec<String>,
}

fn is_pptx(path: &str) -> bool {
    path.to_lowercase().ends_with(".pptx")
}

/// Paths passed on the command line (e.g. `npm run tauri dev -- -- -- deck.pptx`)
/// plus any open requests that arrived before the frontend was ready.
#[tauri::command]
fn startup_files(requests: State<'_, Mutex<OpenRequests>>) -> Vec<String> {
    let mut requests = requests.lock().unwrap();
    requests.ready = true;
    std::env::args().skip(1).filter(|a| is_pptx(a)).chain(requests.pending.drain(..)).collect()
}

fn on_open_urls(app: &tauri::AppHandle, urls: &[tauri::Url]) {
    let paths: Vec<String> = urls
        .iter()
        .filter_map(|u| u.to_file_path().ok())
        .filter_map(|p| p.to_str().map(str::to_owned))
        .filter(|p| is_pptx(p))
        .collect();
    if paths.is_empty() {
        return;
    }
    let state = app.state::<Mutex<OpenRequests>>();
    let mut requests = state.lock().unwrap();
    if requests.ready {
        let _ = app.emit("open-files", paths);
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
    } else {
        requests.pending.extend(paths);
    }
}

/// The app's display name: "旺-Point" when the system's first language is Chinese.
fn app_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        let langs = objc2_foundation::NSLocale::preferredLanguages();
        if langs.firstObject().is_some_and(|l| l.to_string().starts_with("zh")) {
            return "旺-Point";
        }
    }
    "Want-Point"
}

fn build_menu(app: &tauri::App) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let name = app_name();
    let item = |id: &str, text: &str, accel: Option<&str>| {
        let b = MenuItemBuilder::with_id(id, text);
        match accel {
            Some(a) => b.accelerator(a).build(app),
            None => b.build(app),
        }
    };
    let about = AboutMetadata { name: Some(name.into()), ..Default::default() };
    let app_menu = SubmenuBuilder::new(app, name)
        .item(&PredefinedMenuItem::about(app, Some(&format!("關於 {name}")), Some(about))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some(&format!("隱藏 {name}")))?)
        .item(&PredefinedMenuItem::hide_others(app, Some("隱藏其他"))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(&format!("結束 {name}")))?)
        .build()?;
    let file = SubmenuBuilder::new(app, "檔案")
        .item(&item("new", "新增簡報", Some("CmdOrCtrl+N"))?)
        .item(&item("open", "開啟…", Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&item("save", "儲存", Some("CmdOrCtrl+S"))?)
        .item(&item("save_as", "另存新檔…", Some("CmdOrCtrl+Shift+S"))?)
        .separator()
        .item(&item("export_pdf", "匯出為 PDF…", Some("CmdOrCtrl+Shift+E"))?)
        .item(&item("export_video", "匯出為視訊…", None)?)
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
        .separator()
        .item(&item("find", "尋找…", Some("CmdOrCtrl+F"))?)
        .item(&item("replace", "取代…", Some("CmdOrCtrl+Alt+F"))?)
        .item(&item("find_next", "尋找下一個", Some("CmdOrCtrl+G"))?)
        .item(&item("find_previous", "尋找上一個", Some("CmdOrCtrl+Shift+G"))?)
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
        .item(&item("show_presenter", "簡報者檢視", Some("Alt+Enter"))?)
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
        .manage(ShowDeck::default())
        .manage(Mutex::new(OpenRequests::default()))
        .manage(video::VideoExport::default())
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
            read_font,
            write_file,
            list_fonts,
            resolve_font,
            startup_files,
            set_show_deck,
            get_show_deck,
            video::export_video,
            recovery::recovery_write,
            recovery::recovery_clear,
            recovery::recovery_list,
            recovery::recovery_read,
            video::cancel_video_export,
            spell::spell_check,
            spell::spell_guesses,
            spell::spell_ignore,
            spell::spell_learn,
            automation::automation_enabled,
            automation::automation_poll,
            automation::automation_log,
            automation::automation_write
        ])
        .build(tauri::generate_context!())
        .expect("error while building Want-Point")
        .run(|app, event| {
            if let RunEvent::Opened { urls } = &event {
                on_open_urls(app, urls);
            }
        });
}
