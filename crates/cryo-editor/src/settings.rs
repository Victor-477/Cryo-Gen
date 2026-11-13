//! Per-user editor settings (language, tutorial progress, last project), stored in
//! `~/.cryo-engine/settings.json`. Set `CRYO_HOME` to use another folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Settings {
    /// "en" or "pt".
    #[serde(default)]
    pub language: String,
    /// The welcome tour was already shown once.
    #[serde(default)]
    pub tour_seen: bool,
    /// Ids of the tutorials the user finished.
    #[serde(default)]
    pub tutorials_done: Vec<String>,
    #[serde(default)]
    pub last_project: Option<String>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Where a game's `save.set` values are kept: one file per project, by folder name.
pub fn save_file(project: &std::path::Path) -> Option<PathBuf> {
    let name: String = project
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("game")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    settings_dir().map(|d| d.join("saves").join(format!("{name}.json")))
}

fn settings_dir() -> Option<PathBuf> {
    match std::env::var_os("CRYO_HOME") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home().map(|h| h.join(".cryo-engine")),
    }
}

impl Settings {
    pub fn load() -> Settings {
        settings_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Best effort: a read-only home folder must never stop the editor.
    pub fn save(&self) {
        if let Some(dir) = settings_dir() {
            if std::fs::create_dir_all(&dir).is_ok() {
                if let Ok(json) = serde_json::to_string_pretty(self) {
                    std::fs::write(dir.join("settings.json"), json).ok();
                }
            }
        }
    }
}

/// Where the example project lives when the editor is started without a project folder.
pub fn default_project_dir() -> PathBuf {
    match std::env::var_os("CRYO_HOME") {
        Some(dir) => PathBuf::from(dir).join("FrostpeakIsles"),
        None => home().unwrap_or_else(|| PathBuf::from(".")).join("CryoProjects").join("FrostpeakIsles"),
    }
}
