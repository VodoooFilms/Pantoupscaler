pub mod commands;
pub mod ffmpeg;
pub mod plan;
pub mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let ffmpeg = ffmpeg::resolve_ffmpeg(app.handle());
            app.manage(state::AppState::new(ffmpeg));
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let state = window.state::<state::AppState>();
                let pid = state.active.lock().ok().and_then(|active| {
                    active.as_ref().map(|conversion| {
                        conversion
                            .cancel
                            .store(true, std::sync::atomic::Ordering::SeqCst);
                        conversion.pid
                    })
                });
                if let Some(pid) = pid {
                    ffmpeg::terminate(pid);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::select_video,
            commands::inspect_video,
            commands::compute_plan,
            commands::choose_output_folder,
            commands::start_conversion,
            commands::cancel_conversion,
            commands::show_in_folder,
            commands::ffmpeg_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Panto Upscaler");
}
