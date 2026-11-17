//! Game controllers and remappable controls.
//!
//! A gamepad's buttons become input names that scripts read like keys (`pad_a`, `pad_b`,
//! `pad_x`, `pad_y`, `pad_lb`, `pad_rb`, `pad_lt`, `pad_rt`, `pad_start`, `pad_select`,
//! `pad_up`, `pad_down`, `pad_left`, `pad_right`, `pad_ls`, `pad_rs`). The left stick adds to
//! `input.axis("horizontal")` and `"vertical"`, the right stick moves like the mouse (and is
//! also `input.axis("look_x")`, `"look_y"`), and the triggers are `"pad_lt"`, `"pad_rt"` axes.
//!
//! **Bindings** let one name stand for several inputs: with `space = [space, pad_a]`, a script
//! that reads `input.pressed("space")` also jumps with the gamepad's A button. A project may
//! list its own in `input.json`; the player can change them in the pause menu (Controls), and
//! those changes are kept in the user's settings.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// One name a game reads (`input.down("space")`), with what the player can press for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    /// What it does, shown in the Controls menu.
    #[serde(default)]
    pub label: String,
    pub inputs: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    bindings: Vec<Binding>,
}

/// Bindings every project starts with: the keys the examples use, each with a gamepad button.
pub fn defaults() -> Vec<Binding> {
    let b = |name: &str, label: String, inputs: &[&str]| Binding {
        name: name.into(),
        label,
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
    };
    vec![
        b("space", tr!("Jump / roll", "Pular / rolar"), &["space", "pad_a"]),
        b("shift", tr!("Run / guard", "Correr / defender"), &["shift", "pad_lb"]),
        b("mouse_left", tr!("Attack / fire", "Atacar / atirar"), &["mouse_left", "pad_rt"]),
        b("mouse_right", tr!("Heavy attack / aim", "Ataque forte / mirar"), &["mouse_right", "pad_lt"]),
        b("j", tr!("Attack", "Atacar"), &["j", "pad_x"]),
        b("k", tr!("Heavy attack", "Ataque forte"), &["k", "pad_y"]),
        b("e", tr!("Use", "Usar"), &["e", "pad_x"]),
        b("f", tr!("Special", "Especial"), &["f", "pad_y"]),
        b("q", tr!("Item", "Item"), &["q", "pad_rb"]),
        b("r", tr!("Reload", "Recarregar"), &["r", "pad_b"]),
        b("enter", tr!("Skip / confirm", "Pular cena / confirmar"), &["enter", "pad_a"]),
    ]
}

/// The project's bindings (`input.json`, else the defaults) with the player's changes on top.
pub fn load(project_dir: &Path, player: Option<&HashMap<String, Vec<String>>>) -> Vec<Binding> {
    let mut list = std::fs::read_to_string(project_dir.join("input.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<File>(&s).ok())
        .map(|f| f.bindings)
        .filter(|b| !b.is_empty())
        .unwrap_or_else(defaults);
    if let Some(changes) = player {
        for b in &mut list {
            if let Some(inputs) = changes.get(&b.name) {
                b.inputs = inputs.clone();
            }
        }
    }
    list
}

/// The state of the first connected gamepad.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pad {
    pub name: Option<String>,
    pub buttons: HashSet<&'static str>,
    /// Left and right sticks (x right, y up), and the triggers, after the dead zone.
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub lt: f32,
    pub rt: f32,
}

pub struct Gamepads {
    gilrs: Option<gilrs::Gilrs>,
}

fn dead(v: f32) -> f32 {
    let d = 0.18;
    if v.abs() < d {
        0.0
    } else {
        (v - d * v.signum()) / (1.0 - d)
    }
}

impl Gamepads {
    pub fn new() -> Gamepads {
        Gamepads { gilrs: gilrs::Gilrs::new().ok() }
    }

    /// Reads the controllers (call once a frame).
    pub fn poll(&mut self) -> Pad {
        use gilrs::{Axis, Button};
        let Some(g) = &mut self.gilrs else { return Pad::default() };
        while g.next_event().is_some() {}
        let Some((_, pad)) = g.gamepads().find(|(_, p)| p.is_connected()) else { return Pad::default() };
        let mut out = Pad { name: Some(pad.name().to_string()), ..Default::default() };
        for (button, name) in [
            (Button::South, "pad_a"),
            (Button::East, "pad_b"),
            (Button::West, "pad_x"),
            (Button::North, "pad_y"),
            (Button::LeftTrigger, "pad_lb"),
            (Button::RightTrigger, "pad_rb"),
            (Button::LeftTrigger2, "pad_lt"),
            (Button::RightTrigger2, "pad_rt"),
            (Button::Start, "pad_start"),
            (Button::Select, "pad_select"),
            (Button::DPadUp, "pad_up"),
            (Button::DPadDown, "pad_down"),
            (Button::DPadLeft, "pad_left"),
            (Button::DPadRight, "pad_right"),
            (Button::LeftThumb, "pad_ls"),
            (Button::RightThumb, "pad_rs"),
        ] {
            if pad.is_pressed(button) {
                out.buttons.insert(name);
            }
        }
        out.lx = dead(pad.value(Axis::LeftStickX));
        out.ly = dead(pad.value(Axis::LeftStickY));
        out.rx = dead(pad.value(Axis::RightStickX));
        out.ry = dead(pad.value(Axis::RightStickY));
        out.lt = pad.button_data(Button::LeftTrigger2).map(|d| d.value()).unwrap_or(0.0);
        out.rt = pad.button_data(Button::RightTrigger2).map(|d| d.value()).unwrap_or(0.0);
        // analog triggers count as pressed past half way
        if out.lt > 0.5 {
            out.buttons.insert("pad_lt");
        }
        if out.rt > 0.5 {
            out.buttons.insert("pad_rt");
        }
        out
    }
}

/// Turns raw inputs (keys, mouse buttons, gamepad buttons) into what the game reads: a bound
/// name is down while any of its inputs is down.
#[derive(Default)]
pub struct Resolver {
    down: HashSet<String>,
}

impl Resolver {
    /// `raw_down`: what is held now; `raw_pressed`/`raw_released`: what changed this frame (a
    /// quick tap can be pressed and released within one frame). Returns (down, pressed,
    /// released) as the game sees them.
    pub fn resolve(
        &mut self,
        raw_down: &HashSet<String>,
        raw_pressed: &HashSet<String>,
        raw_released: &HashSet<String>,
        bindings: &[Binding],
    ) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
        let bound: HashSet<&str> = bindings.iter().map(|b| b.name.as_str()).collect();
        // a bound name follows its inputs only (its own key may have been moved elsewhere)
        let free = |k: &String| !bound.contains(k.as_str());
        let mut down: HashSet<String> = raw_down.iter().filter(|k| free(k)).cloned().collect();
        let mut pressed: HashSet<String> = raw_pressed.iter().filter(|k| free(k)).cloned().collect();
        let mut released: HashSet<String> = raw_released.iter().filter(|k| free(k)).cloned().collect();
        for b in bindings {
            if b.inputs.iter().any(|i| raw_down.contains(i)) {
                down.insert(b.name.clone());
            }
            if b.inputs.iter().any(|i| raw_pressed.contains(i)) && !self.down.contains(&b.name) {
                pressed.insert(b.name.clone());
            }
        }
        for b in bindings {
            if self.down.contains(&b.name) && !down.contains(&b.name) {
                released.insert(b.name.clone());
            }
        }
        self.down = down.clone();
        (down, pressed, released)
    }
}

/// How an input is shown in the Controls menu.
pub fn label(input: &str) -> String {
    match input {
        "pad_a" => "A".into(),
        "pad_b" => "B".into(),
        "pad_x" => "X".into(),
        "pad_y" => "Y".into(),
        "pad_lb" => "LB".into(),
        "pad_rb" => "RB".into(),
        "pad_lt" => "LT".into(),
        "pad_rt" => "RT".into(),
        "pad_start" => "Start".into(),
        "pad_select" => "Select".into(),
        "pad_up" => "D-pad ↑".into(),
        "pad_down" => "D-pad ↓".into(),
        "pad_left" => "D-pad ←".into(),
        "pad_right" => "D-pad →".into(),
        "pad_ls" => "L3".into(),
        "pad_rs" => "R3".into(),
        "mouse_left" => "Mouse L".into(),
        "mouse_right" => "Mouse R".into(),
        "mouse_middle" => "Mouse M".into(),
        other => crate::qte::key_label(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(keys: &[&str]) -> HashSet<String> {
        keys.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bindings_join_keys_and_gamepad_buttons_and_can_be_remapped() {
        let mut r = Resolver::default();
        let b = defaults();
        let none = set(&[]);
        // the gamepad's A button is "space" for the game
        let (down, pressed, _) = r.resolve(&set(&["pad_a"]), &set(&["pad_a"]), &none, &b);
        assert!(down.contains("space") && pressed.contains("space") && pressed.contains("pad_a"));
        // held: no new press; let go: released
        let (_, pressed, _) = r.resolve(&set(&["pad_a"]), &none, &none, &b);
        assert!(!pressed.contains("space"));
        let (down, _, released) = r.resolve(&none, &none, &set(&["pad_a"]), &b);
        assert!(!down.contains("space") && released.contains("space"));
        // keys nobody bound pass through untouched
        let (down, pressed, _) = r.resolve(&set(&["w"]), &set(&["w"]), &none, &b);
        assert!(down.contains("w") && pressed.contains("w"));
        // a tap within one frame is still a press
        let (down, pressed, _) = r.resolve(&none, &set(&["space"]), &set(&["space"]), &b);
        assert!(!down.contains("space") && pressed.contains("space"));

        // the player moves jump to K: the space bar no longer jumps
        let mut changes = HashMap::new();
        changes.insert("space".to_string(), vec!["k".to_string(), "pad_a".to_string()]);
        let dir = std::env::temp_dir().join(format!("cryo_input_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mine = load(&dir, Some(&changes));
        let mut r = Resolver::default();
        let (down, _, _) = r.resolve(&set(&["space"]), &set(&["space"]), &none, &mine);
        assert!(!down.contains("space"));
        let (down, _, _) = r.resolve(&set(&["k"]), &set(&["k"]), &none, &mine);
        assert!(down.contains("space"));

        // a project can bring its own list
        std::fs::write(dir.join("input.json"), r#"{"bindings":[{"name":"jump","label":"Jump","inputs":["w","pad_a"]}]}"#).unwrap();
        let own = load(&dir, None);
        assert_eq!(own.len(), 1);
        let mut r = Resolver::default();
        assert!(r.resolve(&set(&["pad_a"]), &none, &none, &own).0.contains("jump"));
        std::fs::remove_dir_all(dir).ok();
    }
}
