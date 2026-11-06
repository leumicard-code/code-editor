// ============================================================================
// config.rs — Automatic Session & Preferences Persistence (JSON)
// ============================================================================

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme_idx: usize,
    pub font_size: i32,
    pub show_minimap: bool,
    pub show_sidebar: bool,
    pub show_whitespace: bool,
    pub show_indent_guides: bool,
    pub rainbow_brackets: bool,
    pub sticky_scroll: bool,
    pub inline_color_previews: bool,
    pub vim_enabled: bool,
    pub workspace_dir: Option<PathBuf>,
    pub open_files: Vec<PathBuf>,
    pub active_doc: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme_idx: 0,
            font_size: 15,
            show_minimap: true,
            show_sidebar: true,
            show_whitespace: false,
            show_indent_guides: true,
            rainbow_brackets: true,
            sticky_scroll: true,
            inline_color_previews: true,
            vim_enabled: false,
            workspace_dir: std::env::current_dir().ok(),
            open_files: Vec::new(),
            active_doc: 0,
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("studio_code_editor").join("session.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(raw) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Self>(&raw) {
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, json);
        }
    }
}
