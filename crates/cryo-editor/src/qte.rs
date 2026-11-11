//! Quick time events: a prompt on screen that the player must answer in time.
//!
//! Started from scripts (`qte.press("e", 1.5)`) or from a Timeline marker. The result
//! comes back as `on qte_success(id)` or `on qte_fail(id)`.

use crate::runtime::InputState;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum QteKind {
    /// Press the key once.
    #[default]
    Press,
    /// Press the key `count` times.
    Mash,
    /// Keep the key down for `hold` seconds without letting go.
    Hold,
    /// Press the keys in order; a wrong key fails.
    Sequence,
}

impl QteKind {
    pub const ALL: [QteKind; 4] = [QteKind::Press, QteKind::Mash, QteKind::Hold, QteKind::Sequence];
}

#[derive(Clone, Debug, PartialEq)]
pub struct Qte {
    /// Name given by the game; sent back with the result.
    pub id: String,
    pub kind: QteKind,
    /// One key, or the keys of a sequence, in order.
    pub keys: Vec<String>,
    pub count: u32,
    pub hold: f32,
    /// Seconds the player has.
    pub limit: f32,
    pub elapsed: f32,
    /// Presses done (Mash), seconds held (Hold) or keys done (Sequence).
    pub progress: f32,
    /// Entity that asked for it; `None` sends the result to every script.
    pub owner: Option<u64>,
    /// Animations wait until the player answers (used by cutscenes).
    pub freeze: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Running,
    Success,
    Fail,
}

impl Qte {
    pub fn new(id: &str, kind: QteKind, keys: Vec<String>, limit: f32) -> Qte {
        Qte {
            id: id.to_string(),
            kind,
            keys: keys.into_iter().map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect(),
            count: 10,
            hold: 1.0,
            limit: limit.max(0.1),
            elapsed: 0.0,
            progress: 0.0,
            owner: None,
            freeze: false,
        }
    }

    /// The key the player must use right now.
    pub fn current_key(&self) -> &str {
        let i = if self.kind == QteKind::Sequence { self.progress as usize } else { 0 };
        self.keys.get(i.min(self.keys.len().saturating_sub(1))).map(|s| s.as_str()).unwrap_or("")
    }

    /// How much of the task is done, 0 to 1.
    pub fn fraction(&self) -> f32 {
        let total = match self.kind {
            QteKind::Press => 1.0,
            QteKind::Mash => self.count.max(1) as f32,
            QteKind::Hold => self.hold.max(0.01),
            QteKind::Sequence => self.keys.len().max(1) as f32,
        };
        (self.progress / total).clamp(0.0, 1.0)
    }

    /// How much time is left, 1 down to 0.
    pub fn time_left(&self) -> f32 {
        (1.0 - self.elapsed / self.limit).clamp(0.0, 1.0)
    }

    /// Reads the player's input for one frame. `dt` is real time, never slowed down.
    pub fn update(&mut self, input: &InputState, dt: f32) -> Outcome {
        if self.keys.is_empty() {
            return Outcome::Fail;
        }
        let key = self.current_key().to_string();
        match self.kind {
            QteKind::Press => {
                if input.pressed.contains(&key) {
                    return Outcome::Success;
                }
            }
            QteKind::Mash => {
                if input.pressed.contains(&key) {
                    self.progress += 1.0;
                    if self.progress >= self.count.max(1) as f32 {
                        return Outcome::Success;
                    }
                }
            }
            QteKind::Hold => {
                if input.down.contains(&key) {
                    self.progress += dt;
                    if self.progress >= self.hold {
                        return Outcome::Success;
                    }
                } else {
                    self.progress = 0.0;
                }
            }
            QteKind::Sequence => {
                if input.pressed.contains(&key) {
                    self.progress += 1.0;
                    if self.progress as usize >= self.keys.len() {
                        return Outcome::Success;
                    }
                } else if input.pressed.iter().any(|k| !k.starts_with("mouse") && k != "shift") {
                    return Outcome::Fail; // wrong key
                }
            }
        }
        self.elapsed += dt;
        if self.elapsed >= self.limit {
            Outcome::Fail
        } else {
            Outcome::Running
        }
    }
}

/// Text drawn on the key cap: "e" becomes "E", "space" becomes "SPACE", arrows become arrows.
pub fn key_label(key: &str) -> String {
    match key {
        "up" | "arrowup" => "↑".into(),
        "down" | "arrowdown" => "↓".into(),
        "left" | "arrowleft" => "←".into(),
        "right" | "arrowright" => "→".into(),
        "mouse_left" => "LMB".into(),
        "mouse_right" => "RMB".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(pressed: &[&str], down: &[&str]) -> InputState {
        let mut i = InputState::default();
        i.pressed = pressed.iter().map(|s| s.to_string()).collect();
        i.down = down.iter().chain(pressed).map(|s| s.to_string()).collect();
        i
    }

    #[test]
    fn press_succeeds_in_time_and_fails_late() {
        let mut q = Qte::new("dodge", QteKind::Press, vec!["E".into()], 1.0);
        assert_eq!(q.update(&input(&[], &[]), 0.5), Outcome::Running);
        assert_eq!(q.update(&input(&["q"], &[]), 0.1), Outcome::Running); // wrong key is forgiven
        assert_eq!(q.update(&input(&["e"], &[]), 0.1), Outcome::Success);

        let mut q = Qte::new("dodge", QteKind::Press, vec!["e".into()], 1.0);
        assert_eq!(q.update(&input(&[], &[]), 0.6), Outcome::Running);
        assert_eq!(q.update(&input(&[], &[]), 0.6), Outcome::Fail);
    }

    #[test]
    fn mash_counts_presses() {
        let mut q = Qte::new("door", QteKind::Mash, vec!["space".into()], 3.0);
        q.count = 3;
        assert_eq!(q.update(&input(&["space"], &[]), 0.1), Outcome::Running);
        assert_eq!(q.update(&input(&[], &["space"]), 0.1), Outcome::Running); // holding is not mashing
        assert_eq!(q.update(&input(&["space"], &[]), 0.1), Outcome::Running);
        assert!((q.fraction() - 2.0 / 3.0).abs() < 1e-5);
        assert_eq!(q.update(&input(&["space"], &[]), 0.1), Outcome::Success);
    }

    #[test]
    fn hold_resets_when_released() {
        let mut q = Qte::new("lever", QteKind::Hold, vec!["e".into()], 5.0);
        q.hold = 1.0;
        assert_eq!(q.update(&input(&[], &["e"]), 0.6), Outcome::Running);
        assert_eq!(q.update(&input(&[], &[]), 0.1), Outcome::Running);
        assert_eq!(q.progress, 0.0);
        assert_eq!(q.update(&input(&[], &["e"]), 0.6), Outcome::Running);
        assert_eq!(q.update(&input(&[], &["e"]), 0.6), Outcome::Success);
    }

    #[test]
    fn sequence_needs_the_right_order() {
        let keys = || vec!["w".to_string(), "a".to_string(), "d".to_string()];
        let mut q = Qte::new("combo", QteKind::Sequence, keys(), 5.0);
        assert_eq!(q.update(&input(&["w"], &[]), 0.1), Outcome::Running);
        assert_eq!(q.current_key(), "a");
        assert_eq!(q.update(&input(&["a"], &[]), 0.1), Outcome::Running);
        assert_eq!(q.update(&input(&["d"], &[]), 0.1), Outcome::Success);

        let mut q = Qte::new("combo", QteKind::Sequence, keys(), 5.0);
        assert_eq!(q.update(&input(&["w"], &[]), 0.1), Outcome::Running);
        assert_eq!(q.update(&input(&["d"], &[]), 0.1), Outcome::Fail);
    }
}
