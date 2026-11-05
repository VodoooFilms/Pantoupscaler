use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use base64::Engine;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::plan::{self, Mode, Plan};
use crate::state::{ActiveConversion, AppState};

const SUPPORTED_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi"];

static RE_DURATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Duration:\s*(\d+):(\d+):(\d+(?:\.\d+)?)").unwrap());
static RE_DIMENSIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d{2,5})x(\d{2,5})").unwrap());
static RE_FPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([\d.]+)\s+fps").unwrap());
static RE_AUDIO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Audio:\s*([A-Za-z0-9_]+)").unwrap());
static RE_AUDIO_BITRATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)\s*kb/s").unwrap());
static RE_ROTATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"rotate\s*:\s*(-?\d+(?:\.\d+)?)").unwrap());
static RE_ROTATION_OF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"rotation of\s*(-?\d+(?:\.\d+)?)").unwrap());

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: f64,
    pub codec: String,
    pub audio_codec: Option<String>,
    pub audio_kbps: u32,
    pub has_audio: bool,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Debug)]
struct ProbeInfo {
    width: u32,
    height: u32,
    duration: f64,
    fps: f64,
    codec: String,
    audio_codec: Option<String>,
    audio_kbps: u32,
}

impl ProbeInfo {
    fn has_audio(&self) -> bool {
        self.audio_codec.is_some()
    }

    fn audio_can_be_copied(&self) -> bool {
        matches!(
            self.audio_codec.as_deref(),
            Some("aac" | "mp3" | "ac3" | "eac3" | "alac")
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequest {
    pub input_path: String,
    pub target_id: String,
    pub mode: String,
    pub output_directory: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub percent: i32,
    pub elapsed: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    pub output_path: String,
    pub output_name: String,
    pub output_bytes: u64,
    pub input_bytes: u64,
    pub thumbnail: Option<String>,
    pub elapsed_seconds: f64,
    pub plan: Plan,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub available: bool,
    pub path: Option<String>,
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| supported.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(false)
}

pub fn resolve_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    let executable = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join("ffmpeg").join(executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let dev_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("ffmpeg")
        .join(executable);
    if dev_candidate.is_file() {
        return Some(dev_candidate);
    }

    find_binary("ffmpeg")
}

pub fn find_binary(name: &str) -> Option<PathBuf> {
    if name == "ffmpeg" {
        if let Some(override_path) = std::env::var_os("PANTO_FFMPEG") {
            let candidate = PathBuf::from(override_path);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };

    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(&executable);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let common_directories = [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/snap/bin",
    ];
    for directory in common_directories {
        let candidate = Path::new(directory).join(&executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

async fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = command
        .spawn()
        .map_err(|error| format!("FFMPEG_MISSING: {error}"))?;

    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => Err(format!("FFMPEG_FAILED: {error}")),
        Err(_) => Err("FFMPEG_TIMEOUT: FFmpeg tardó demasiado en responder.".to_string()),
    }
}

async fn probe(ffmpeg: &Path, path: &Path) -> Result<ProbeInfo, String> {
    let args = vec![
        "-hide_banner".to_string(),
        "-i".to_string(),
        path.to_string_lossy().into_owned(),
    ];
    let output = run_ffmpeg(ffmpeg, &args, Duration::from_secs(45)).await?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    let duration = RE_DURATION
        .captures(&stderr)
        .map(|captures| {
            let hours: f64 = captures[1].parse().unwrap_or(0.0);
            let minutes: f64 = captures[2].parse().unwrap_or(0.0);
            let seconds: f64 = captures[3].parse().unwrap_or(0.0);
            hours * 3600.0 + minutes * 60.0 + seconds
        })
        .unwrap_or(0.0);

    let mut width = 0;
    let mut height = 0;
    let mut fps = 0.0;
    let mut codec = String::new();
    let mut audio_codec: Option<String> = None;
    let mut audio_kbps = 0;
    let mut rotation = 0.0_f64;
    let mut found_video = false;

    for line in stderr.lines() {
        let trimmed = line.trim();

        if !found_video && trimmed.contains("Video:") && !trimmed.contains("attached pic") {
            if let Some(captures) = RE_DIMENSIONS.captures(trimmed) {
                width = captures[1].parse().unwrap_or(0);
                height = captures[2].parse().unwrap_or(0);
                if width > 0 && height > 0 {
                    found_video = true;
                }
            }
            if let Some(captures) = RE_FPS.captures(trimmed) {
                fps = captures[1].parse().unwrap_or(0.0);
            }
            if let Some(after) = trimmed.split("Video:").nth(1) {
                codec = after
                    .trim()
                    .split(|character: char| {
                        character == ' ' || character == ',' || character == '('
                    })
                    .next()
                    .unwrap_or("")
                    .to_string();
            }
        }

        if audio_codec.is_none() && trimmed.contains("Audio:") {
            if let Some(captures) = RE_AUDIO.captures(trimmed) {
                audio_codec = Some(captures[1].to_string());
            }
            if let Some(captures) = RE_AUDIO_BITRATE.captures(trimmed) {
                audio_kbps = captures[1].parse().unwrap_or(0);
            }
        }

        if rotation == 0.0 {
            if let Some(captures) = RE_ROTATE.captures(trimmed) {
                rotation = captures[1].parse().unwrap_or(0.0);
            } else if let Some(captures) = RE_ROTATION_OF.captures(trimmed) {
                rotation = captures[1].parse().unwrap_or(0.0);
            }
        }
    }

    if width == 0 || height == 0 {
        return Err("UNSUPPORTED: no se encontró una pista de vídeo.".to_string());
    }

    if audio_codec.is_some() && audio_kbps == 0 {
        audio_kbps = 192;
    }

    if rotation.rem_euclid(180.0) >= 89.5 && rotation.rem_euclid(180.0) <= 90.5 {
        std::mem::swap(&mut width, &mut height);
    }

    Ok(ProbeInfo {
        width,
        height,
        duration,
        fps,
        codec,
        audio_codec,
        audio_kbps,
    })
}

async fn thumbnail(ffmpeg: &Path, path: &Path, duration: f64) -> Option<String> {
    let timestamp = if duration > 3.0 {
        (duration * 0.12).min(3.0)
    } else {
        0.0
    };
    let args = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(),
        "error".to_string(),
        "-ss".to_string(),
        format!("{timestamp:.3}"),
        "-i".to_string(),
        path.to_string_lossy().into_owned(),
        "-frames:v".to_string(),
        "1".to_string(),
        "-vf".to_string(),
        "scale=480:-2:flags=lanczos".to_string(),
        "-f".to_string(),
        "image2pipe".to_string(),
        "-vcodec".to_string(),
        "png".to_string(),
        "pipe:1".to_string(),
    ];

    match run_ffmpeg(ffmpeg, &args, Duration::from_secs(20)).await {
        Ok(output)
            if output.status.success()
                && !output.stdout.is_empty()
                && output.stdout.len() <= 12 * 1024 * 1024 =>
        {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&output.stdout);
            Some(format!("data:image/png;base64,{encoded}"))
        }
        _ => None,
    }
}

pub async fn inspect(ffmpeg: &Path, path: &Path) -> Result<MediaInfo, String> {
    if !is_supported(path) {
        return Err("UNSUPPORTED: formato de vídeo no compatible.".to_string());
    }

    let metadata = std::fs::metadata(path)
        .map_err(|_| "UNSUPPORTED: el archivo no existe o no se puede leer.".to_string())?;
    if !metadata.is_file() {
        return Err("UNSUPPORTED: el archivo no existe o no se puede leer.".to_string());
    }

    let info = probe(ffmpeg, path).await?;
    let preview = thumbnail(ffmpeg, path, info.duration).await;
    let has_audio = info.has_audio();

    Ok(MediaInfo {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("video")
            .to_string(),
        size: metadata.len(),
        width: info.width,
        height: info.height,
        duration: info.duration,
        fps: info.fps,
        codec: info.codec,
        audio_codec: info.audio_codec.clone(),
        audio_kbps: info.audio_kbps,
        has_audio,
        thumbnail: preview,
    })
}

pub async fn convert<F>(
    ffmpeg: &Path,
    state: &AppState,
    request: ConversionRequest,
    emit: F,
) -> Result<ConversionResult, String>
where
    F: Fn(ProgressPayload) + Send + Sync + 'static,
{
    let input = PathBuf::from(&request.input_path);
    if !is_supported(&input) {
        return Err("UNSUPPORTED: formato de vídeo no compatible.".to_string());
    }

    let input_metadata = std::fs::metadata(&input)
        .map_err(|_| "UNSUPPORTED: el archivo no existe o no se puede leer.".to_string())?;
    if !input_metadata.is_file() {
        return Err("UNSUPPORTED: el archivo no existe o no se puede leer.".to_string());
    }

    {
        let active = state
            .active
            .lock()
            .map_err(|_| "STATE: no se pudo bloquear el estado.".to_string())?;
        if active.is_some() {
            return Err("BUSY: ya hay una conversión en curso.".to_string());
        }
    }

    let probe = probe(ffmpeg, &input).await?;
    let target = plan::target_by_id(&request.target_id);
    let mode = Mode::parse(&request.mode);
    let plan = plan::compute_plan(
        probe.width,
        probe.height,
        probe.duration,
        probe.audio_kbps,
        &request.target_id,
        &request.mode,
    );

    let output_directory = request
        .output_directory
        .filter(|directory| Path::new(directory).is_dir())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            input
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });

    let output = unique_output_path(&input, target.label, &output_directory);
    let filter = plan::build_filter(&plan, mode);
    let args = build_arguments(&input, &output, &filter, &probe);

    let mut command = Command::new(ffmpeg);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| format!("FFMPEG_MISSING: {error}"))?;
    let pid = child.id().unwrap_or(0);
    let cancel = Arc::new(AtomicBool::new(false));

    {
        let mut active = state
            .active
            .lock()
            .map_err(|_| "STATE: no se pudo bloquear el estado.".to_string())?;
        *active = Some(ActiveConversion {
            pid,
            cancel: cancel.clone(),
        });
    }

    let started = Instant::now();
    let duration = probe.duration;
    let emit = Arc::new(emit);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "FFMPEG_FAILED: sin salida de progreso.".to_string())?;
    let progress_emit = Arc::clone(&emit);
    let progress_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        let mut last_percent = -1;
        while let Ok(Some(line)) = lines.next_line().await {
            let Some(value) = line.strip_prefix("out_time_us=") else {
                continue;
            };
            let Ok(microseconds) = value.trim().parse::<i64>() else {
                continue;
            };
            if duration <= 0.0 {
                continue;
            }
            let percent =
                ((microseconds as f64 / 1_000_000.0 / duration) * 100.0).clamp(0.0, 99.0) as i32;
            if percent != last_percent {
                last_percent = percent;
                progress_emit(ProgressPayload {
                    percent,
                    elapsed: started.elapsed().as_secs_f64(),
                });
            }
        }
    });

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "FFMPEG_FAILED: sin salida de errores.".to_string())?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut buffer = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if buffer.len() < 64_000 {
                buffer.push_str(&line);
                buffer.push('\n');
            }
        }
        buffer
    });

    let status = child.wait().await;
    let _ = progress_task.await;
    let stderr_text = stderr_task.await.unwrap_or_default();
    let was_cancelled = cancel.load(Ordering::SeqCst);

    if let Ok(mut active) = state.active.lock() {
        *active = None;
    }

    if was_cancelled {
        let _ = std::fs::remove_file(&output);
        return Err("CANCELLED: conversión cancelada.".to_string());
    }

    if !matches!(status, Ok(status) if status.success()) {
        let _ = std::fs::remove_file(&output);
        let detail = stderr_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" ");
        return Err(format!(
            "FFMPEG_FAILED: {}",
            if detail.is_empty() {
                "FFmpeg terminó con un error."
            } else {
                &detail
            }
        ));
    }

    emit(ProgressPayload {
        percent: 100,
        elapsed: started.elapsed().as_secs_f64(),
    });

    let output_bytes = std::fs::metadata(&output)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let output_thumbnail = thumbnail(ffmpeg, &output, probe.duration).await;

    Ok(ConversionResult {
        output_path: output.to_string_lossy().into_owned(),
        output_name: output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("video_escalado.mp4")
            .to_string(),
        output_bytes,
        input_bytes: input_metadata.len(),
        thumbnail: output_thumbnail,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        plan,
    })
}

fn build_arguments(input: &Path, output: &Path, filter: &str, probe: &ProbeInfo) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".to_string(),
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().into_owned(),
        "-map".to_string(),
        "0:v:0".to_string(),
    ];

    if probe.has_audio() {
        args.push("-map".to_string());
        args.push("0:a?".to_string());
        if probe.audio_can_be_copied() {
            args.push("-c:a".to_string());
            args.push("copy".to_string());
        } else {
            args.push("-c:a".to_string());
            args.push("aac".to_string());
            args.push("-b:a".to_string());
            args.push("192k".to_string());
        }
    } else {
        args.push("-an".to_string());
    }

    args.extend([
        "-vf".to_string(),
        filter.to_string(),
        "-c:v".to_string(),
        "libx264".to_string(),
        // Quality-first encode: veryslow squeezes the most out of every bit and
        // CRF 14 keeps the Lanczos upscale visually transparent.
        "-preset".to_string(),
        "veryslow".to_string(),
        "-crf".to_string(),
        "14".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
        "-movflags".to_string(),
        "+faststart".to_string(),
        "-map_metadata".to_string(),
        "0".to_string(),
        "-map_chapters".to_string(),
        "0".to_string(),
        "-metadata:s:v:0".to_string(),
        "rotate=0".to_string(),
        "-progress".to_string(),
        "pipe:1".to_string(),
        "-nostats".to_string(),
        output.to_string_lossy().into_owned(),
    ]);

    args
}

fn unique_output_path(input: &Path, label: &str, directory: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let safe_label = label.to_lowercase();
    let base = format!("{stem}_{safe_label}");

    let mut candidate = directory.join(format!("{base}.mp4"));
    let mut count = 2;
    while candidate.exists() {
        candidate = directory.join(format!("{base}-{count}.mp4"));
        count += 1;
    }
    candidate
}

pub fn terminate(pid: u32) {
    if pid == 0 {
        return;
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
    }
}

pub fn reveal(path: &str) {
    let path = Path::new(path);
    if !path.exists() {
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}
