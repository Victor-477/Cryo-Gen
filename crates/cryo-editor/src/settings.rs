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
    /// "high", "medium" or "low": how much of a scene's effects and shadows are drawn.
    #[serde(default)]
    pub graphics: String,
    /// Most frames per second a running game draws: 30, 60, 120, or 0 for no limit.
    /// Not set: 60.
    #[serde(default)]
    pub fps_limit: Option<u32>,
    /// Volumes, 0 to 1 (not set: full).
    #[serde(default)]
    pub volume_master: Option<f32>,
    #[serde(default)]
    pub volume_music: Option<f32>,
    #[serde(default)]
    pub volume_effects: Option<f32>,
    /// The player's control changes, per project (folder name), name -> inputs.
    #[serde(default)]
    pub bindings: std::collections::HashMap<String, std::collections::HashMap<String, Vec<String>>>,
    /// The game's own options (menus.json), per project: id -> value.
    #[serde(default)]
    pub game_options: std::collections::HashMap<String, std::collections::HashMap<String, f64>>,
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
    /// The frame-rate limit in use (0 = none).
    pub fn fps(&self) -> u32 {
        match self.fps_limit {
            Some(v) if [0, 30, 60, 120].contains(&v) => v,
            _ => 60,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rate_limit_defaults_to_60_and_accepts_only_the_offered_values() {
        let mut s = Settings::default();
        assert_eq!(s.fps(), 60);
        for v in [0, 30, 60, 120] {
            s.fps_limit = Some(v);
            assert_eq!(s.fps(), v);
        }
        s.fps_limit = Some(45);
        assert_eq!(s.fps(), 60);
        // old settings files have no limit: they still load
        let old: Settings = serde_json::from_str(r#"{"language":"pt","graphics":"low"}"#).unwrap();
        assert_eq!((old.fps(), old.graphics.as_str()), (60, "low"));
    }
}
