//! 匯出視訊: the frontend sends the deck (saved as PPTX) and the settings; the
//! video is rendered and encoded on a background thread, reporting progress
//! with `video-progress` events and the outcome with `video-done`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Emitter, State};

/// Set to stop the running export.
#[derive(Default)]
pub struct VideoExport {
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

#[derive(Clone, Serialize)]
struct Done {
    ok: bool,
    cancelled: bool,
    error: Option<String>,
    path: String,
}

fn header<'a>(request: &'a Request<'_>, name: &str) -> Option<&'a str> {
    request.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Starts exporting. Headers: `path` (percent-encoded), `width`, `fps`,
/// `seconds` (per slide), `timings` ("1" to use recorded timings); body: PPTX bytes.
#[tauri::command]
pub fn export_video(app: AppHandle, state: State<'_, VideoExport>, request: Request<'_>) -> Result<(), String> {
    let path = crate::percent_decode(header(&request, "path").ok_or("missing path header")?);
    let num = |name: &str| header(&request, name).and_then(|v| v.parse::<f64>().ok());
    let opts = ppt_video::VideoOptions {
        width: num("width").map_or(1920, |v| v as u32),
        fps: num("fps").map_or(30, |v| v as u32),
        seconds_per_slide: num("seconds").unwrap_or(5.0),
        use_timings: header(&request, "timings") == Some("1"),
    };
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    if state.running.swap(true, Ordering::SeqCst) {
        return Err(ppt_model::tr("已經在匯出視訊", "已在导出视频", "A video export is already running", &[]));
    }
    let pres = match ppt_ooxml::parse_pptx(bytes) {
        Ok(p) => p,
        Err(e) => {
            state.running.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }
    };
    let (cancel, running) = (state.cancel.clone(), state.running.clone());
    cancel.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let mut shown = 0;
        let result = ppt_video::export(&pres, &opts, std::path::Path::new(&path), &mut |done, total| {
            // About every percent.
            if done * 100 / total != shown || done == total {
                shown = done * 100 / total;
                let _ = app.emit("video-progress", Progress { done, total });
            }
            !cancel.load(Ordering::SeqCst)
        });
        running.store(false, Ordering::SeqCst);
        let done = match result {
            Ok(finished) => Done { ok: finished, cancelled: !finished, error: None, path },
            Err(e) => Done { ok: false, cancelled: false, error: Some(e), path },
        };
        let _ = app.emit("video-done", done);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_video_export(state: State<'_, VideoExport>) {
    state.cancel.store(true, Ordering::SeqCst);
}
