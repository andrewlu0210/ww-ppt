//! Native side of Want-Point: file and font access plus macOS integration.
//! The editor itself runs in the webview (see `crates/ppt-wasm`).

mod automation;
mod recovery;
mod spell;
mod video;

use ppt_fonts::{FontService, ResolvedFont};
use ppt_model::{Lang, lang, set_lang, tr};
use tauri::ipc::Response;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
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
    // A poisoned lock still holds a usable list: keep going rather than panic again.
    let mut requests = requests.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut requests = state.lock().unwrap_or_else(|e| e.into_inner());
    if requests.ready {
        let _ = app.emit("open-files", paths);
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_focus();
        }
    } else {
        requests.pending.extend(paths);
    }
}

/// The interface language, chosen from the system's preferred languages the
/// same way the frontend chooses (`ui/src/i18n`).
fn detect_lang() -> Lang {
    #[cfg(target_os = "macos")]
    {
        // A language picked in Preferences wins over the system's.
        if let Some(lang) = std::fs::read_to_string(language_pref_path()).ok().and_then(|v| Lang::from_tag(v.trim())) {
            return lang;
        }
        let langs: Vec<String> = objc2_foundation::NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect();
        Lang::detect(langs.iter().map(String::as_str))
    }
    #[cfg(not(target_os = "macos"))]
    Lang::En
}

/// Where the language chosen in Preferences is kept (absent: follow the system). The native
/// menus are built before the web view exists, so they cannot read the frontend's storage.
fn language_pref_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/com.ww.ppt/language")
}

/// Remembers the language picked in Preferences for the next launch: "zh-Hant", "zh-Hans",
/// "en", or "system" to follow the system again.
#[tauri::command]
fn set_language_pref(value: String) -> Result<(), String> {
    let path = language_pref_path();
    if value == "system" {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if !matches!(value.as_str(), "zh-Hant" | "zh-Hans" | "en") {
        return Err("unsupported language".into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, value).map_err(|e| e.to_string())
}

/// The app's display name: "旺-Point" in Chinese, "Want-Point" in English.
fn app_name() -> &'static str {
    lang().pick("旺-Point", "旺-Point", "Want-Point")
}

fn build_menu(app: &tauri::App) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let name = app_name();
    let l = lang();
    let m = |zh_hant: &'static str, zh_hans: &'static str, en: &'static str| l.pick(zh_hant, zh_hans, en);
    let item = |id: &str, text: &str, accel: Option<&str>| {
        let b = MenuItemBuilder::with_id(id, text);
        match accel {
            Some(a) => b.accelerator(a).build(app),
            None => b.build(app),
        }
    };
    let app_menu = SubmenuBuilder::new(app, name)
        .item(&item("about", &tr("關於 {}", "关于 {}", "About {}", &[&name]), None)?)
        .separator()
        .item(&item("preferences", m("偏好設定…", "偏好设置…", "Preferences…"), Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some(&tr("隱藏 {}", "隐藏 {}", "Hide {}", &[&name])))?)
        .item(&PredefinedMenuItem::hide_others(app, Some(m("隱藏其他", "隐藏其他", "Hide Others")))?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some(&tr("結束 {}", "退出 {}", "Quit {}", &[&name])))?)
        .build()?;
    let file = SubmenuBuilder::new(app, m("檔案", "文件", "File"))
        .item(&item("new", m("新增簡報", "新建演示文稿", "New Presentation"), Some("CmdOrCtrl+N"))?)
        .item(&item("open", m("開啟…", "打开…", "Open…"), Some("CmdOrCtrl+O"))?)
        .separator()
        .item(&item("save", m("儲存", "保存", "Save"), Some("CmdOrCtrl+S"))?)
        .item(&item("save_as", m("另存新檔…", "另存为…", "Save As…"), Some("CmdOrCtrl+Shift+S"))?)
        .separator()
        .item(&item("export_pdf", m("匯出為 PDF…", "导出为 PDF…", "Export as PDF…"), Some("CmdOrCtrl+Shift+E"))?)
        .item(&item("export_video", m("匯出為視訊…", "导出为视频…", "Export as Video…"), None)?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, Some(m("關閉視窗", "关闭窗口", "Close Window")))?)
        .build()?;
    // Undo/redo/select-all go to the editor core instead of WebKit's own handlers.
    // Cut/copy/paste stay predefined so the webview receives clipboard events.
    let edit = SubmenuBuilder::new(app, m("編輯", "编辑", "Edit"))
        .item(&item("undo", m("復原", "撤销", "Undo"), Some("CmdOrCtrl+Z"))?)
        .item(&item("redo", m("取消復原", "重做", "Redo"), Some("CmdOrCtrl+Shift+Z"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some(m("剪下", "剪切", "Cut")))?)
        .item(&PredefinedMenuItem::copy(app, Some(m("拷貝", "拷贝", "Copy")))?)
        .item(&PredefinedMenuItem::paste(app, Some(m("貼上", "粘贴", "Paste")))?)
        .item(&item("select_all", m("全選", "全选", "Select All"), Some("CmdOrCtrl+A"))?)
        .separator()
        .item(&item("find", m("尋找…", "查找…", "Find…"), Some("CmdOrCtrl+F"))?)
        .item(&item("replace", m("取代…", "替换…", "Replace…"), Some("CmdOrCtrl+Alt+F"))?)
        .item(&item("find_next", m("尋找下一個", "查找下一个", "Find Next"), Some("CmdOrCtrl+G"))?)
        .item(&item("find_previous", m("尋找上一個", "查找上一个", "Find Previous"), Some("CmdOrCtrl+Shift+G"))?)
        .build()?;
    let insert = SubmenuBuilder::new(app, m("插入", "插入", "Insert"))
        .item(&item("new_slide", m("新增投影片", "新建幻灯片", "New Slide"), Some("CmdOrCtrl+Shift+N"))?)
        .item(&item("duplicate_slide", m("複製投影片", "复制幻灯片", "Duplicate Slide"), None)?)
        .separator()
        .item(&item("insert_picture", m("圖片…", "图片…", "Picture…"), None)?)
        .item(&item("insert_text_box", m("文字方塊", "文本框", "Text Box"), None)?)
        .item(&item("insert_rect", m("矩形", "矩形", "Rectangle"), None)?)
        .item(&item("insert_ellipse", m("橢圓", "椭圆", "Oval"), None)?)
        .separator()
        .item(&item("insert_link", m("超連結…", "超链接…", "Hyperlink…"), Some("CmdOrCtrl+K"))?)
        .build()?;
    let arrange = SubmenuBuilder::new(app, m("排列", "排列", "Arrange"))
        .item(&item("group", m("組成群組", "组合", "Group"), Some("CmdOrCtrl+Alt+G"))?)
        .item(&item("ungroup", m("取消群組", "取消组合", "Ungroup"), Some("CmdOrCtrl+Alt+Shift+G"))?)
        .build()?;
    let view = SubmenuBuilder::new(app, m("檢視", "视图", "View"))
        .item(&item("zoom_in", m("放大", "放大", "Zoom In"), Some("CmdOrCtrl+="))?)
        .item(&item("zoom_out", m("縮小", "缩小", "Zoom Out"), Some("CmdOrCtrl+-"))?)
        .item(&item("zoom_fit", m("符合視窗大小", "适应窗口大小", "Fit to Window"), Some("CmdOrCtrl+0"))?)
        .separator()
        .item(&item("toggle_notes", m("備忘稿", "备注", "Notes"), None)?)
        .build()?;
    let show = SubmenuBuilder::new(app, m("投影片放映", "幻灯片放映", "Slide Show"))
        .item(&item("show_from_beginning", m("從頭播放", "从头开始", "Play from Start"), Some("CmdOrCtrl+Shift+Enter"))?)
        .item(&item("show_from_current", m("從目前的投影片播放", "从当前幻灯片开始", "Play from Current Slide"), Some("CmdOrCtrl+Enter"))?)
        .item(&item("show_presenter", m("簡報者檢視", "演示者视图", "Presenter View"), Some("Alt+Enter"))?)
        .build()?;
    let window = SubmenuBuilder::new(app, m("視窗", "窗口", "Window"))
        .item(&PredefinedMenuItem::minimize(app, Some(m("縮到最小", "最小化", "Minimize")))?)
        .item(&PredefinedMenuItem::fullscreen(app, Some(m("進入全螢幕", "进入全屏幕", "Enter Full Screen")))?)
        .build()?;
    MenuBuilder::new(app).items(&[&app_menu, &file, &edit, &insert, &arrange, &view, &show, &window]).build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before anything that names things in the interface language (menus, new decks, errors).
    set_lang(detect_lang());
    tauri::Builder::default()
        // Narration recording needs the microphone. The web view would ask by itself on every
        // launch; macOS's own one-time authorization (the usage text in Info.plist) is the real
        // consent, so the web view just passes the request on. Nothing else is granted.
        .on_permission_request(|_, kind| match kind {
            tauri::webview::PermissionKind::Microphone => tauri::webview::PermissionResponse::Allow,
            _ => tauri::webview::PermissionResponse::Default,
        })
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
            set_language_pref,
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
