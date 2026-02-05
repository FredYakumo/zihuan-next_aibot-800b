use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub width: f32,
    pub height: f32,
    pub x: i32,
    pub y: i32,
}

impl WindowState {
    pub fn from_window(window: &slint::Window) -> Self {
        let size = window.size();
        let position = window.position();
        WindowState {
            width: size.width as f32,
            height: size.height as f32,
            x: position.x,
            y: position.y,
        }
    }
}

pub fn load_window_state() -> Option<WindowState> {
    let path = state_file_path()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_window_state(state: &WindowState) -> std::io::Result<()> {
    let path = match state_file_path() {
        Some(path) => path,
        None => return Ok(()),
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(state)
        .unwrap_or_else(|_| "{\"width\":1200.0,\"height\":800.0,\"x\":0,\"y\":0}".to_string());
    fs::write(path, json)
}

pub fn apply_window_state(window: &slint::Window, state: &WindowState) {
    let min_width = 800.0;
    let min_height = 600.0;
    let width = state.width.max(min_width);
    let height = state.height.max(min_height);

    window.set_size(slint::LogicalSize::new(width, height));
    window.set_position(slint::PhysicalPosition::new(state.x, state.y));
}

fn state_file_path() -> Option<PathBuf> {
    let base_dir = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .ok()
            .map(PathBuf::from)
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|home| PathBuf::from(home).join(".config"))
            })
    }?;

    Some(base_dir.join("zihuan-next_aibot").join("window_state.json"))
}
