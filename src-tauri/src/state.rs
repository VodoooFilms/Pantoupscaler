use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct ActiveConversion {
    pub pid: u32,
    pub cancel: Arc<AtomicBool>,
}

pub struct AppState {
    pub ffmpeg: Option<PathBuf>,
    pub active: Mutex<Option<ActiveConversion>>,
}

impl AppState {
    pub fn new(ffmpeg: Option<PathBuf>) -> Self {
        Self {
            ffmpeg,
            active: Mutex::new(None),
        }
    }
}
