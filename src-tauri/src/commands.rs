use std::path::Path;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, State};

use crate::ffmpeg::{self, ConversionRequest, ConversionResult, FfmpegStatus, MediaInfo};
use crate::plan::{self, Plan};
use crate::state::AppState;

#[tauri::command]
pub async fn select_video(
    state: State<'_, AppState>,
    language: String,
) -> Result<Option<MediaInfo>, String> {
    let (title, filter) = if language == "en" {
        ("Select a video", "Videos")
    } else {
        ("Selecciona un vídeo", "Vídeos")
    };

    let file = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, &["mp4", "mov", "m4v", "mkv", "webm", "avi"])
        .pick_file()
        .await;

    let Some(file) = file else {
        return Ok(None);
    };

    let path = file.path().to_path_buf();
    let ffmpeg = state.ffmpeg.clone().ok_or("FFMPEG_MISSING")?;
    ffmpeg::inspect(&ffmpeg, &path).await.map(Some)
}

#[tauri::command]
pub async fn inspect_video(state: State<'_, AppState>, path: String) -> Result<MediaInfo, String> {
    let ffmpeg = state.ffmpeg.clone().ok_or("FFMPEG_MISSING")?;
    ffmpeg::inspect(&ffmpeg, Path::new(&path)).await
}

#[tauri::command]
pub fn compute_plan(
    width: u32,
    height: u32,
    duration: f64,
    audio_kbps: u32,
    target: String,
    mode: String,
) -> Plan {
    plan::compute_plan(width, height, duration, audio_kbps, &target, &mode)
}

#[tauri::command]
pub async fn choose_output_folder(language: String) -> Result<Option<String>, String> {
    let title = if language == "en" {
        "Choose output folder"
    } else {
        "Elige la carpeta de salida"
    };

    Ok(rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await
        .map(|folder| folder.path().to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn start_conversion(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ConversionRequest,
) -> Result<ConversionResult, String> {
    let ffmpeg = state.ffmpeg.clone().ok_or("FFMPEG_MISSING")?;
    ffmpeg::convert(&ffmpeg, state.inner(), request, move |payload| {
        let _ = app.emit("conversion-progress", payload);
    })
    .await
}

#[tauri::command]
pub fn cancel_conversion(state: State<'_, AppState>) -> bool {
    let Ok(active) = state.active.lock() else {
        return false;
    };

    let Some(conversion) = active.as_ref() else {
        return false;
    };

    conversion.cancel.store(true, Ordering::SeqCst);
    ffmpeg::terminate(conversion.pid);
    true
}

#[tauri::command]
pub fn show_in_folder(path: String) {
    ffmpeg::reveal(&path);
}

#[tauri::command]
pub fn ffmpeg_status(state: State<'_, AppState>) -> FfmpegStatus {
    FfmpegStatus {
        available: state.ffmpeg.is_some(),
        path: state
            .ffmpeg
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
    }
}
