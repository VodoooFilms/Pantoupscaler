use std::path::PathBuf;

use panto_upscaler_lib::ffmpeg::{self, ConversionRequest};
use panto_upscaler_lib::state::AppState;

fn find_ffmpeg() -> Option<PathBuf> {
    ffmpeg::find_binary("ffmpeg")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn upscales_832x480_to_1080p_with_24px_bars() {
    let Some(ffmpeg) = find_ffmpeg() else {
        eprintln!("ffmpeg no está instalado; se omite la prueba de integración.");
        return;
    };

    let directory = tempfile::tempdir().expect("temp dir");
    let input = directory.path().join("cortofinal.mp4");

    let status = std::process::Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=832x480:rate=24:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&input)
        .status()
        .expect("generate source video");
    assert!(status.success(), "no se pudo generar el vídeo de prueba");

    let state = AppState::new(Some(ffmpeg.clone()));
    let result = ffmpeg::convert(
        &ffmpeg,
        &state,
        ConversionRequest {
            input_path: input.to_string_lossy().into_owned(),
            target_id: "1080".to_string(),
            mode: "pad".to_string(),
            output_directory: Some(directory.path().to_string_lossy().into_owned()),
        },
        |_| {},
    )
    .await
    .expect("conversion should succeed");

    assert!(std::path::Path::new(&result.output_path).is_file());
    assert_eq!(result.plan.target_width, 1920);
    assert_eq!(result.plan.target_height, 1080);
    assert_eq!(result.plan.scaled_width, 1872);
    assert_eq!(result.plan.bar_left, 24);
    assert_eq!(result.plan.bar_right, 24);
    assert!(result.output_bytes > 0);

    let info = ffmpeg::inspect(&ffmpeg, std::path::Path::new(&result.output_path))
        .await
        .expect("inspect output");
    assert_eq!(info.width, 1920);
    assert_eq!(info.height, 1080);
    assert!(info.has_audio);
}
